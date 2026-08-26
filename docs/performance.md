# TUI performance model

How the TUI event loop stays cheap as the number of open workspaces/tabs
grows, and which further optimizations were considered and deliberately
deferred. Written after the 2026-07-18 performance work (commits
`085e411..c4eaa5f`), which was designed by comparing this codebase against
[herdr](https://github.com/ogulcancelik/herdr) — a multi-agent terminal
manager on the same stack (Rust + ratatui + tokio + portable-pty) that does
not degrade with many open sessions.

## The problem this solved

With many projects open, the TUI got sluggish. Root causes, all in
`crates/tui/src/event_loop.rs` and `crates/core`:

1. Per-tab bookkeeping (idle detection, OSC drains, liveness, watcher polls)
   ran on **every wakeup** — every keystroke paid an O(workspaces × tabs)
   sweep taking a mutex per tab.
2. The Agents-pane spinner set `needs_redraw` on every 50ms tick while *any*
   agent anywhere was `Running` — with several projects open, the full UI
   re-rendered at 20 fps indefinitely.
3. Passive agent-state detection (hookless providers, e.g. Codex) locked each
   matching tab's vt100 parser and materialized the **entire screen** as
   strings on every tick, contending with the PTY reader thread.
4. `FileWatcher` used notify's `RecursiveMode::Recursive`, which on Linux
   registers an inotify kernel watch for **every directory** in the worktree
   (including `target/`, `node_modules/`, `.git/`); the ignore filter only
   ran on events already delivered to userspace.
5. The loop ticked at a fixed 50ms forever because PTY output was invisible
   to it (detected only by polling byte counters), so it could never sleep.

## Current architecture — invariants

Preserve these when touching `event_loop.rs`, `pty/session.rs`, or
`workspace/watcher.rs`:

- **O(workspaces × tabs) work lives only in `poll_workspaces()`, which runs
  only on the tick.** The per-wakeup path (keystrokes, stream events) does
  O(1) work: agent-selection sync, nucleo matcher ticks, toast expiry.
- **PTY output wakes the loop through `piki_core::pty::PtyOutputSignal`** —
  an app-wide dirty-bit + `tokio::sync::Notify` pair cloned into every
  spawned session. Reader threads `raise()` it after each parser flush,
  *after* releasing the parser lock; the raise is coalesced (only the first
  raise after a consumer `take()` notifies), so N streaming sessions cost one
  wakeup per frame. The event loop's handler is one atomic load
  (`check_active_tab_output`).
- **`TICK_RATE` is 250ms and is a fallback only.** It bounds the staleness of
  periodic bookkeeping (liveness, idle detection, OSC drains, git-refresh
  scheduling — all second-scale concerns). Nothing latency-sensitive may rely
  on a fast tick.
- **Renders are capped at ~30 fps** (`MIN_RENDER_INTERVAL` = 33ms). When a
  redraw is deferred by the cap, a `render_deadline` select branch wakes the
  loop exactly when the frame becomes eligible — output-driven wakeups can
  arrive per PTY read chunk and must not turn into per-chunk full-UI
  rebuilds.
- **The spinner has its own cadence** (`SPINNER_INTERVAL` = 150ms,
  `App::last_spinner_at`) — it, not the tick, bounds the steady-state frame
  rate while agents run. It is `Instant`-based on purpose so tick changes
  don't silently alter the frame rate.
- **Passive agent-state detection is double-gated**: the sweep runs at most
  every `PASSIVE_DETECT_INTERVAL` (300ms), and per tab only when
  `bytes_processed` advanced since the last scrape (`Tab::last_detect_bytes`,
  a lock-free atomic load). A quiet hookless tab costs one atomic load; the
  parser lock + full-screen sample is only paid on new output. Hook-bridged
  agents (Claude, Antigravity) are event-driven and unaffected.
- **The file watcher registers directories selectively on Linux**: a manual
  walk skips ignored directories (`is_ignored_dir`: `.git`, `target`,
  `node_modules`, `.claude`, `.venv`, `dist`, `build`) and symlinks entirely,
  registering one `NonRecursive` watch per remaining directory. Directories
  created later are queued by the event callback and registered lazily in
  `try_recv`/`drain`. macOS/Windows keep the recursive watch (there it is a
  single cheap kernel-side subscription) and rely on the event-side
  `should_ignore` filter. Keep `is_ignored_dir` a superset of the directory
  names `should_ignore` filters.

## Considered and deferred (the rest of the herdr playbook)

These are the herdr techniques **not** adopted, with what they'd buy and what
they'd cost. Revisit if the TUI ever feels slow again *after* profiling —
none of them addresses per-project scaling (that's solved above); they reduce
constant per-frame/idle costs.

Recommended order if picking these up: **1 → 2 → 3**; 4 only for the detach
feature; 5–6 only if their specific need appears.

### 1. DEC 2026 synchronized output (moderate effort, best next step)

Modern TUIs (Claude Code included) wrap each logical frame in
`CSI ?2026h` … `CSI ?2026l` so terminals apply it atomically. herdr tracks
this mode per pane and suppresses render requests while a sync block is open
— one render per *logical child frame*, never a render of a half-drawn
state.

piki today raises `PtyOutputSignal` per read/flush, which doesn't align with
the child's frame boundaries: mild tearing is possible (in practice rare —
a frame usually arrives in one read) and up to 30 renders/s happen when ~10
logical frames/s would do.

Implementation sketch: the reader thread already runs `OscParser` over every
chunk (OSC 133/777); extend it to recognize the `?2026` CSI pair and skip
`raise()` while a block is open, raising on close. Needs a safety timeout
(~150ms, what real emulators use) in case a child dies mid-block and the
closing sequence never arrives. Contained to `shell_integration/parser.rs` +
`pty/session.rs`. Note the vt100 parser still processes all bytes — this
only changes *when the loop is woken*.

### 2. Render profiling hooks (small effort, do before #3)

herdr has env-gated counters (`HERDR_RENDER_PROF`) for PTY bytes, VT write
time, dirty-collection and frame-preparation time — that instrumentation is
how they justified their retained-frame path. If further render optimization
is ever on the table, add the equivalent first and let the numbers decide.

### 3. VT emulator with native damage tracking (large effort, high risk)

herdr vendors **libghostty-vt** (Ghostty's terminal core, Zig, via FFI —
~4.2k lines of bindings + ~3.7k wrapper) and gets per-row dirty state
(`Clean`/`Partial`/`Full`). Rendering reads only dirty rows, and a
"retained frame" path bypasses ratatui entirely: dirty rows are patched onto
the last frame, and a fully-clean screen with an unmoved cursor emits
nothing.

piki's `vt100` crate has no damage tracking: each frame,
`tui_term::PseudoTerminal` walks the **whole grid** of the active tab
(~10k cells at 200×50) into the ratatui buffer, which then diffs. Output is
efficient; compute is O(screen) per frame even for a one-character change.

This cost is constant — it does not scale with project count (only the
active tab renders), and the 30 fps cap bounds it. That's why it was
deferred. If a profile ever shows the active-tab render as the hotspot
(think huge terminals), the options are: (a) migrate to
`alacritty_terminal`, which has damage tracking (`TermDamage`) and is pure
Rust — but rewrites the render/scrollback/selection/search integration and
drops `tui_term`; (b) vendor libghostty-vt like herdr — adds a Zig toolchain
to the build; (c) fork `vt100` to add per-row dirty flags — least new code,
permanent fork maintenance. The emulator is load-bearing for rendering,
scrollback, selection, terminal search, passive-detection scrapes and
snapshot tests; all paths are invasive.

### 4. Headless client/server architecture — IMPLEMENTED (a feature, not perf)

In herdr a headless server owns all PTYs and VT state; the attached TUI
client is a thin dumb terminal receiving pre-diffed frames, and sessions
survive detach (tmux-style). We wanted the detach/attach feature, so this is
now **shipped** — not for perf, but so PTY tabs survive quitting/crashing the
app and re-attach on the next launch. Unlike herdr's pre-diffed frames, our
daemon fans out **raw PTY bytes** (+ a restore buffer generated from a
daemon-side `vt100` parser) and each client keeps its own emulator, so the
render/scrollback/selection paths above were untouched. See
`docs/persistent-sessions.md` and `crates/core/src/session/`. The daemon is
opt-out via `[sessions] enabled = false`; with it off, tabs run in-process
exactly as this document's other sections describe.

### 5. Fully deadline-based loop, no tick (small effort, marginal gain)

herdr has no tick at all: every periodic concern contributes an
`Option<Instant>` deadline and the loop sleeps until `min()` of them — a
fully idle app does zero wakeups. piki kept a 250ms fallback tick: idle cost
is 4 wakeups/s of bounded work (≈0% CPU). Converting liveness/idle/OSC
drains to per-concern deadlines would close that gap but adds real risk of a
concern silently never being scheduled. Not worth it on current numbers.

### 6. Git-status caching/dedup across workspaces (only if requirements change)

herdr runs git status on an interval on a background thread with a path-keyed
cache, deduplicating workspaces that share a repo. piki refreshes only the
*active* workspace (3s period / 500ms debounce), so this isn't a hotspot.
Becomes relevant only if live git badges for *all* sidebar workspaces are
ever wanted — dedup by `source_repo` would be the way.

### Non-issues checked and dismissed

- **Scrollback bounds**: herdr caps scrollback by bytes (10MB/session); piki
  caps by lines (1000 per tab in `vt100::Parser::new`) — equivalent role,
  nothing to fix.
- **PTY read batching**: both batch reads before locking the parser (piki:
  16KB reads, 64KB batches) — already fine.

## Desktop (Tauri)

The desktop app has no event loop of its own — xterm.js is the emulator and
the webview renders — so its hot paths are the IPC bridge and the DOM work
around panes. Phase 16 of the desktop roadmap (2026-08-25) fixed the three
that showed up in the audit; the invariants below are enforced in code
(`crates/desktop/src/pty_output.rs` tests, `frontend/src/mount-policy.test.ts`,
`frontend/src/pty-frame.test.ts`) and documented in `crates/desktop/CLAUDE.md`.

### Invariants

- **PTY output is coalesced before it crosses the IPC.** Each PTY reader
  pushes chunks to a per-tab `OutputBatcher`; an emitter thread ships at
  most one message per `BATCH_WINDOW` (8 ms) or `BATCH_MAX_BYTES` (64 KB),
  whichever comes first. The first byte of a batch never waits longer than
  the window, a batch never exceeds the cap unless a single `read()` did,
  bytes stay in order and `pty-exit` is emitted by the same thread *after*
  the last batch. Same philosophy as the TUI's `PtyOutputSignal`: readers
  never talk to the UI per read.
- **Bytes travel raw.** The batches go over a Tauri `Channel<Vec<u8>>`
  (`InvokeResponseBody::Raw`; frames ≥ 1 KB ride the binary fetch path)
  framed as `len(tab_id) · tab_id · bytes`. The base64 JSON `pty-output`
  event is only the fallback while no channel is registered. Structured
  events (`pty-shell-event`, `pty-agent-event`) stay JSON events — they are
  tiny and rare.
- **Hidden terminals do not parse.** Output for a terminal whose pane is
  not on screen is queued (`HiddenOutputBuffer`, 2 MB cap after which it is
  fed to xterm anyway — never dropped) and replayed on the next mount;
  `fit`/`resizePty` never run for a hidden instance.
- **A pane click renders nothing.** `setActivePane` emits
  `active-pane-changed` (highlight toggle + focus) and `active-tab-changed`
  (tab strip refresh). Only `pane-tree-changed` / `active-workspace-changed`
  run `render()`, and `render()` reconciles the tree against the DOM by
  pane id instead of rebuilding it, so a split or close touches only the
  changed nodes. `resyncPty` (daemon restore) fires once per content, on its
  first mount; focus lands only on the active pane's content.
- **One PTY resize per frame.** The `ResizeObserver` refits xterm locally on
  every frame of a divider drag, but the `resizePty` IPC is skipped when the
  grid is unchanged and otherwise coalesced to one call per instance per
  animation frame; the divider's mouseup flushes the exact final size.

### Reproducible benchmark

Run every step in an isolated instance:
`target/release/piki-desktop --data-dir /tmp/piki-bench` (build with
`just frontend && cargo build --release -p piki-desktop`). The debug counters
need a dev build (`cargo tauri dev`, `import.meta.env.DEV`): open the
devtools console and read `__pikiPerf.counters` / call `__pikiPerf.reset()`.

| # | Scenario | Steps | Metric |
|---|----------|-------|--------|
| 1 | `cat` 50 MB | `head -c 52428800 /dev/urandom \| base64 > /tmp/big.txt` (≈68 MB of printable text; use `head -c 52428800 /tmp/big.txt` for exactly 50 MB), then in a shell tab: `time cat /tmp/big.txt` | wall time reported by `time` (the shell prompt returns only once the PTY drained) + `pty.batch` / `pty.bytes` counters (IPC messages per byte) + does the UI still answer a click on another pane during the stream |
| 2 | Click between 4 panes | one tab, split right, split down twice → 4 shell panes; `__pikiPerf.reset()`; click each pane in turn, 10 rounds | `pane.render`, `terminal.mount`, `terminal.resync` after 40 clicks |
| 3 | Drag a divider | same 4-pane tab; `__pikiPerf.reset()`; drag the vertical divider left/right for ~3 s | `terminal.resizePty`, `pane.render` |
| — | Coalescer micro-bench | `cargo test --release -p piki-desktop bench_ -- --ignored --nocapture` | batches per 3200 reads of 16 KB (50 MB) |
| — | Decode micro-bench | `cd crates/desktop/frontend && PIKI_BENCH=1 npx vitest run src/pty-frame.test.ts --reporter=verbose` | ms to decode 8 MB per path |

### Numbers (2026-08-25, Linux, release build where noted)

| Scenario | Before | After |
|----------|--------|-------|
| Coalescer, 50 MB as 16 KB reads | 3200 IPC messages (one per read, by construction) | **800 batches** (0.25 per read, mean 64 KB) in 74 ms of batching overhead — measured, `bench_coalescer_50mb` |
| Decode 8 MB in the webview thread | 762.7 ms (`Uint8Array.from(atob(), cb)`, per-byte callback) | **37.6 ms** with the indexed base64 loop (fallback path); **0.1 ms** on the raw channel (a subarray view, no decode) — measured under vitest/node, `pty-frame.test.ts` |
| IPC messages per 50 MB `cat` | ≈3200+ JSON events, each carrying 4/3× the bytes as base64 and parsed as JSON | ≈800 binary frames, no base64, no JSON parse (derived from the two rows above) |
| `cat` 50 MB wall time to quiescence | not measured — needs an interactive session (no headless driver for the webview on this machine) | not measured — same; the coalescer and decode rows bound the IPC-side cost |
| 40 clicks between 4 panes | 40 full renders, 160 `mountTab`, 160 `resyncPty` (every click rebuilt the tree and remounted every pane — reasoning over the old `render()` + `mountTerminalInto`) | **0 renders, 0 mounts, 0 resyncs** by construction (`pane.render` fires only on `pane-tree-changed`; `shouldResync` is false after the first mount — `mount-policy.test.ts`); not counted live — needs an interactive session |
| 3 s divider drag | one `resizePty` per pane per `ResizeObserver` callback (≈60/s × 2 panes) | ≤ 1 per pane per frame and only when the grid changed, + one exact flush on mouseup; not counted live — needs an interactive session |

Rows marked "not measured" need someone in front of the GUI: run the steps
above in a dev build and paste the counters here.
