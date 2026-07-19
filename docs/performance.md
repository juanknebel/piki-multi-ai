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

### 4. Headless client/server architecture (rewrite-scale; a feature, not perf)

In herdr a headless server owns all PTYs and VT state; the attached TUI
client is a thin dumb terminal receiving pre-diffed frames, and sessions
survive detach (tmux-style). This is what makes their retained-frame path
natural. For piki it would be a structural rewrite whose real value is the
detach/attach feature — pursue it only if that feature is wanted.

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
