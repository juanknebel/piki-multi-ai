# piki-desktop

Tauri v2 desktop GUI for piki-multi. **Depends on `piki-core`, `piki-api-client`, and `piki-agent`.**
Rust backend in `src/` wraps `piki-core` behind Tauri IPC commands; the frontend in `frontend/` is
vanilla TypeScript + xterm.js + CodeMirror 6, built with Vite. User-facing behaviour is documented in
`docs/technical.md` (desktop sections); this file holds the rules for changing the code.

## Build & test

```bash
cd crates/desktop/frontend && npm install && cd -
cargo build -p piki-desktop                 # needs frontend/dist — `just frontend` builds it
cd crates/desktop/frontend && npm test      # vitest, src/**/*.test.ts (node, no DOM)
just lint-desktop                           # frontend test + build, then clippy on this crate
```

- Frontend unit tests cover pure modules (registries, scoring, stores, state machines); DOM-bound
  components are exercised in the Rust/E2E layers. Every pure module listed below has a `*.test.ts`.
- Guard tests that fail the build on drift: `css-invariants.test.ts` (stylesheet rules),
  `components/icons.test.ts` (no emoji glyphs in component code), `docs-parity.test.ts` (the desktop
  shortcut table in `docs/technical.md` matches `shortcuts.ts`), `shortcuts.test.ts` (help sections).
- Bench: `cargo test --release -p piki-desktop bench_ -- --ignored --nocapture` (coalescer),
  `PIKI_BENCH=1 npx vitest run src/pty-frame.test.ts --reporter=verbose` (decode). Procedure and
  numbers: `docs/performance.md` § Desktop.

## Rust backend (`src/`)

- `main.rs` — Tauri entry point, setup, command registration; `--serve-sessions` is parsed before Tauri
  (session daemon entry). Resolves the shared settings at startup —
  `piki_core::app_settings::resolve(config_path, storage.ui_prefs)` (Settings ▸ General override in the
  DB > `[sessions]`/`[notifications]` in `config.toml` > default) — then `effective.notifications.apply()`
  and `connect_session_daemon(&paths, effective.sessions_enabled)`; the value it started with is
  `DesktopApp.sessions_enabled`. `tauri-plugin-window-state` restores size/position/maximized/fullscreen.
- `state.rs` — `DesktopApp`, `DesktopWorkspace`, `DesktopTab { custom_title: Option<String> }`,
  `TabInfo { custom_title }` with `display_label()` (`custom_title` > `provider.label()`, 40-char cap);
  state structs managed by Tauri. `rename_tab(workspace_idx, tab_id, Option<title>)` sets it, mirrored by
  `frontend/src/state.ts:renameTab()` + `frontend/src/types.ts:getTabLabel()`.
- `pty_raw.rs` — `RawPtySession` (`Local | Remote`): streams raw PTY bytes to the frontend (no `vt100`);
  runs an `OscParser` on shell-tab streams (`pty-shell-event`: cwd, command exit codes) and
  `handle_cli_agent` for the structured agent channel (see *Agent signals*).
- `pty_output.rs` — the PTY output path (see *PTY output & perf invariants*).
- `events.rs` — Tauri event emission helpers (sysinfo, git refresh, toast), `spawn_git_watcher`,
  `spawn_idle_watcher_loop` (`pty-attention` events), `acknowledge_agent_attention` / `emit_agent_ack`.
- `session.rs` — daemon attach at startup (`reattach_sessions`, `tab_from_session`).
- `log_buffer.rs` — in-memory tracing ring buffer (500 entries) for the log viewer.
- `lsp/` — LSP WebSocket proxy: `registry.rs` (server config from `lsp.toml`), `server.rs`
  (`LspManager`, TTL idle shutdown, max concurrent cap — uses `tokio::sync::Mutex`, not `parking_lot`,
  for async spawns), `proxy.rs` (WebSocket server bridging JSON-RPC to child processes).
- `commands/` — one module per IPC domain: `workspace.rs`, `pty.rs`, `git.rs`, `diff.rs`, `gitlog.rs`,
  `stash.rs`, `agents.rs`, `review.rs`, `theme.rs`, `logs.rs`, `search.rs`, `markdown.rs`, `fs.rs`,
  `kanban.rs`, `api.rs`, `providers.rs`, `system.rs`, `clipboard.rs`, `chat.rs`, `lsp.rs`,
  `session.rs`, `settings.rs`. Details per area below.
- **Locking**: all Tauri commands lock `Mutex<DesktopApp>` — scope the guard in a `{ }` block and drop it
  before any `.await` or `spawn_blocking`.
- Domain types are mirrored in `frontend/src/types.ts` — keep them in sync with `piki-core::domain`.

## Tabs & panes model

- A workspace owns top-level tabs; each tab owns a tree of panes (`pane-tree.ts`); each pane holds exactly
  one content (`ws.tabs`). Splitting makes a blank pane whose chooser opens content *into that pane*.
- Frontend-only contents (Markdown, CodeEditor, WebPreview — `types.ts isFrontendOnlyProvider`) use
  synthesized ids (e.g. `web-${Date.now()}`) and `appState.addTab` without `ipc.spawnTab`; their close
  handlers skip `ipc.closeTab` and destroy local state directly.
- **Backend indices**: `ws.tabs` holds contents the backend list does not — every index-based IPC
  (`close_tab`, `detach_tab`, `set_active_tab`) MUST use `appState.backendTabIndex(wsIdx, tabId)`; a backend
  index coming in (agent rows, session jumps) goes through `appState.setActiveBackendTab(wsIdx, backendIdx)`
  / `tabIndexForBackend`. Kanban / API contents have a PTY-less backend tab: closing them calls
  `ipc.closeTab` too (`tab-bar.ts releaseBackendOnly`) so the lists stay aligned.
- Singletons (`appState.isSingletonProvider`: Kanban, Api, WebPreview) picked for a pane while open
  elsewhere get a `showConfirm` *Move here* (`appState.moveContentToPane(contentId, paneId)`: target must
  be a blank leaf; the source pane collapses into its sibling, and when it was its tab's only pane that
  tab is dropped — same rule as `removeTab`) / *Go there* (`focusSingletonTab`) / Cancel.
- `pane-view.ts renderEmptyState()` is the per-workspace empty state (name · branch + Shell / providers /
  tools / Open file…) for both a tab-less workspace and a blank pane; `renderWelcome` is only for
  "no workspace at all".
- **Layout snapshot with contents** (`layout-snapshot.ts`, pure, vitest; key `wsTabsV2` in the settings
  document): `SavedContent { id, kind, path?, url?, title? }` per non-PTY content inside
  `SavedWsLayout.contents`; helpers `describeContent`, `parseSavedLayouts`, `missingSavedContents`,
  `remapContentIds`, `snapshotContents`. `appState.setContentRestorer(ContentRestorer)` — installed by
  `open-content.ts installContentRestorer()` in `main.ts` BEFORE `loadPaneTrees` — is how the store
  reaches the panels without importing them: `_hydrateLayout` re-registers editors / previews
  synchronously under their OLD id (`hasCodeEditorInstance` / `hasMarkdownEditorInstance` /
  `registerWebPreview` + `getWebPreviewUrl`), places a placeholder for Kanban / API and `_finishRestore`
  re-spawns them (`ipc.spawnTab`, new id → `_rebindContentId` remaps the trees) and `verify`s
  cold-restored editor files (missing → `appState.dropContent`: pane blank, toast). `setActiveWorkspace`
  keeps the frontend-only contents the backend list never had and flushes a pending layout save before
  re-hydrating.

## Opening content

- `components/open-content.ts` is the ONE opener: `openProvider(provider, { paneId? })` (Shell / agents /
  Kanban / API via `ipc.spawnTab`, `WebPreview` via `openWebPreviewTab({ paneId, url? })`),
  `openFileInEditor(wsIdx, path, { paneId?, forceCode? })` (`registerCodeFile` / `registerMarkdownFile`
  + `addTab` or `setPaneContent`), `openFileInExternalEditor`, `contentLabel(c)` (editors show their file
  name), `getPaneProviderChoices()` + `TOOL_CHOICES`. The menu bar, palette, sidebar icons, file tree,
  file/markdown viewers, Source Control and `main.ts` bindings all go through it — never spawn +
  `addTab` inline again. With `paneId` the content lands in that blank pane
  (`appState.setPaneContent`); without it, a new top-level tab.
- `openFuzzySearch({ paneId })` scopes the file finder to a pane.
- `provider-cache.ts` (`preloadProviderTabs` / `getCachedProviderTabs` / `invalidateProviderCache`,
  re-exported by `menu-bar.ts`) is the shared `providers.toml` list for every "open here" chooser;
  `menu-bar.ts preloadProviderTabs()` warms it at init and after `invalidateProviderCache`.
- `dialogs/needs-workspace.ts showNeedsWorkspace(why)` is the empty state for agent profiles / dispatch
  with zero workspaces (CTA → create).

## Tabs: close, rename, move, restart

- Any tab/pane close goes through `tearDownAndCloseWsTab` / `tearDownAndClosePane` (`tab-bar.ts`, both
  async): dirty-editor prompt → `showConfirm` naming every live PTY process (`liveProcessLine`, agent
  status included) with Close / Keep running (`ipc.detachTab`, only when `appState.sessionsAvailable`) /
  Cancel → `releasePtyContents` (highest index first; a tab that fails to release stays open) +
  `destroyTerminal` + panel teardown. Never call `appState.closePane` / `closeWsTab` / `ipc.closeTab`
  directly from UI code, and never `.catch(() => {})` a close/rename (use `reportError`).
  `tearDownAndCloseWsTab(wsIdx, i, { mode: "detach" })` skips the confirm (the menu's "Close keep running").
- `tab-bar.ts` renders a scrolling `.ws-tab-strip`; the `+` and the `⋯` all-tabs button
  (`allTabsMenuItems` → `openContextMenu`) sit outside it. Middle-click (`auxclick` button 1) and the tab
  context menu (`wsTabMenuItems`: Rename / Split / Move to workspace… / Close / Close keep running) all use
  the teardown above. Chips carry `data-ws-tab-id`.
- **Rename**: `wsTabTitle()` uses `getTabLabel(tab, termTitle?)` (`custom_title` > terminal OSC title
  (`TabShellState.title`, frontend only) > `provider.label()`); dblclick or menu → `beginInlineRename()`
  turns the label into an `.ws-tab-rename` input (Enter commits, Esc cancels, blur commits, empty clears;
  targets the ACTIVE pane's content via `activeContentId(wt)`; `renderWorkspaceTabBar` skips a refresh
  while the input exists). Commits via `appState.renameTab()` + `ipc.renameTab()`. Status bar, pane
  header and dashboard also use `getTabLabel()`; the Agents panel keeps the backend label.
- **Move**: `moveWsTabToWorkspace(wsIdx, wsTabIdx)` (and `moveActiveWsTabToWorkspace()` for menu bar +
  palette) is the ONLY move path: `createDropdown` inside `showConfirm` picks the target, `ipc.moveTab`
  per content (PTY contents only — editors/boards refuse), then `appState.moveWsTab` re-homes the pane
  tree and flushes the layout snapshot synchronously before the switch. Backend:
  `pty.rs::move_tab(from_workspace_idx, tab_id, to_workspace_idx) -> new_idx` re-parents a `DesktopTab`
  WITHOUT dropping it (no Detach/Kill; the xterm stays bound to the same id) and re-points a daemon
  session's `workspace_path` via `Daemon::set_meta` off-lock.
- **Dead tabs**: `markTabDead` (on `pty-exit`) emits `tabs-changed` + `pane-tree-changed` so the
  `.ws-tab--dead` chip, the pane-head Restart button (`restartPaneContent`: `ipc.closeTab` the exited tab →
  `ipc.spawnTab` same provider → `appState.replacePaneContent` into the same pane, custom title carried
  over) and the status bar's `(exited)` update immediately.
- **Delete workspace**: `dialogs/delete-workspace.ts confirmDeleteWorkspace(idx)` is the ONE delete confirm
  (sidebar menu + palette): wording by `workspace_type`, `ws.changedFiles.length`, live agents from
  `appState.agentRows`. `workspace.rs::delete_workspace` kills the workspace's tabs and removes their
  daemon sessions before the worktree goes (a plain drop would only detach them) — the confirm promises
  exactly that.

## Sidebar, switcher, labels

- `workspace-list.ts` rows have a single `⋯` + right-click → `workspaceMenuItems(idx)`; its ⚙ opens
  `showAgentManager(idx)` for THAT row's workspace. Per-row agent rollup glyph (collapsed worktree
  families aggregate hidden children by `family_key` / `source_repo`) via `types.ts::agentStatusSeverity`
  / `actionableStatusView` (mirrors of `piki_core::cli_agent::status_severity` and the TUI's
  `actionable_status_view` — change all together).
- `workspace-switcher.ts` ranks with the pure `mru.ts` (`mruBump` / `mruRank` / `rankItems`) over the
  `workspaceMru` settings list that `appState.setActiveWorkspace` bumps (the single choke point for
  switches); rows show `statusGlyph` (agent rollup or dirty git).
- `labels.ts branchLabel()` / `truncateMiddle()` is the one branch-label rule (28 chars, middle ellipsis,
  full text in `title`) — use it wherever a branch renders.
- Chevrons: `icon("chevron-right", { class: "group-chevron" })`, rotated 90° by the `.group-chevron` /
  `.ft-chevron` / `.theme-group-chevron` CSS when expanded — never add a new glyph.
- `sidebar.ts maxAgentsPanelHeight()` clamps the Agents panel so ≥4 workspace rows stay visible; it
  measures a live `.workspace-item` (never assume px).
- Full re-renders must preserve UI state: keep `scrollTop`, input caret/focus across `innerHTML` rebuilds
  (see workspace-list, file-tree, source-control for the pattern), and prefer patching in place (status bar
  LSP segment, kanban search, pane titles) over rebuilding.

## Agent signals

- `pty_raw.rs::handle_cli_agent` emits `pty-agent-event { status, kind, summary, attention }` per tab;
  `attention` is true for permission / idle-notification / stop until the user looks at the tab.
- Acks are backend-owned: `events::acknowledge_agent_attention(tab)` clears the marker and returns
  whether it did; `events::emit_agent_ack` sends `pty-agent-ack { tab_id }` (always after the
  `DesktopApp` lock is dropped). `set_active_tab` / `switch_workspace` call both; `handle_cli_agent` acks
  on the spot when the event lands on the active tab of the active workspace (TUI semantics — that event
  goes out with `attention: false`).
- Frontend: `appState.applyAgentEvent` / `applyAgentAck` keep `TabShellState.attention`; always pass it as
  the 2nd arg of `cliAgentStatusView` (tab bar, status bar, close-confirm line, Agents panel).
  `appState.applyShellEvent` handles `pty-shell-event` (status-bar cwd, ✓/✗ tab badges).
- `appState.agentRows` (+ `agentRowsFetchedAt`, event `agent-rows-changed`) is the ONE store for
  `list_agent_rows`: `startAgentRowsSync()` in `agents-panel.ts` (called once from `main.ts`) does the
  debounced fetch, and `setAgentRows` seeds per-tab shell state so a daemon-restored tab shows its dot;
  never fetch rows from a component. Consumers: Agents panel, `workspace-list.ts` rollup, `status-bar.ts`
  `● N need you` segment, `activity-bar.ts` amber badge on the Explorer icon, and `jumpToAttention()`
  (`Alt+A`, palette, Agents menu) built on the pure `agent-attention.ts` (`attentionRows`,
  `pickAttentionTarget` — severity order + cyclic walk —, `liveElapsedSecs`).
- `AgentRow.elapsed_secs` comes from `CliAgentState::run_started_at`; `formatElapsed` (`types.ts`) mirrors
  `piki_core::cli_agent::format_elapsed` and the panel ticks the label in place every second.
- Agents panel (`agents-panel.ts`) is docked at the bottom of `#sidebar` below `#sidebar-views` — ALWAYS
  visible whatever view is active (the activity-bar "agents" icon opens the profile-manager dialog);
  height resizable via `#agents-resize-h` in `sidebar.ts` (32px–75% of the sidebar, persisted as
  `settings.agentsPanelHeight`); rows are a keyboard `listbox` (Tab in, ↑/↓/Home/End, Enter/click →
  `jumpToAgentRow`; focus survives re-renders by `data-tab-id`); labels are `DesktopTab::display_label()`.

## Sessions (persistent-session daemon)

- `commands/session.rs`: `list_sessions` / `kill_session` / `remove_session` (Sessions dialog; jump uses
  `local_workspace_idx` / `local_tab_idx`, `workspace_idx` is the loaded workspace matching the session's
  path); `adopt_session(session_id, workspace_idx) -> tab_idx` (attach + `session::tab_from_session`, the
  same builder startup re-attach uses — keep them identical); `session_status` (async, `spawn_blocking`;
  `on` / `off` / `unavailable` + live count, plus `enabled_next` = `app_settings::resolve` re-read each poll
  so the status bar appends `(restart)` — polled every 3 s); `restore_summary` (what
  `session::reattach_sessions` restored, stored on `DesktopApp.restore_summary`, read once by `main.ts`
  for the toast + `appState.markRestored` badge); `quit_summary` (alive tabs split by `pty.is_remote()`,
  drives the quit dialog wording).
- Frontend: `dialogs/sessions-dialog.ts`, opened by `Alt+Shift+S` / View menu / palette / the `sessions N`
  status-bar segment. `sessions_available` (daemon present?) is read once at startup into
  `appState.sessionsAvailable` and gates the "Keep running" option.
- `commands/pty.rs::detach_tab` implements *Keep running*: removes the tab so `RawPtySession::Remote`'s
  Drop sends `Detach` (never `Kill`, never `remove_session`); refuses for a local (in-process) tab.
  **Do NOT** call `pty.kill()` on quit — dropping a `Remote` session must detach.

## Keyboard shortcuts

- `shortcuts.ts` is the single registry: rebindable `ShortcutDef`s (with `category`) + non-rebindable
  `fixedShortcuts`. The help dialog derives from `helpSections()`; the menu bar, palette, empty/welcome
  screens and tooltips read keys via `getShortcutKey` — never hardcode a key string anywhere.
- **The terminal owns its keys**: a def is outside-only by default (skipped while `focusOwnsKeys()` —
  xterm, `<input>` / `<textarea>`, contenteditable editors — is true); it captures everywhere only with
  `terminalCapture: true`, which the `ShortcutDef` union restricts to a `TerminalSafeCombo` default
  (`Alt+…`, `Ctrl+Shift+…`, `Ctrl+Alt+…`) — `tsc` fails on `{ defaultKey: "Ctrl+B", terminalCapture: true }`.
  At runtime `isTerminalSafeCombo(def.key)` re-applies the rule to user rebinds (`isDemotedShortcut`).
- `fixedShortcuts` rows must be keys that really fire somewhere (copy/paste are platform-aware there);
  `RESERVED_COMBOS` blocks rebinding onto them. `Alt+1…9` and `Ctrl+Tab` are dispatched from
  `handleGlobalKeydown` via `switch-workspace` / `switch-tab` DOM events handled in `main.ts`.
- `KEY_CODES` matches `=` / `-` / `0` on the physical key because Shift changes their character.
- **Adding a shortcut**: a def in `shortcuts.ts` (pick the `category`; `terminalCapture` only with a
  terminal-safe default), `bindAction(id, …)` in `main.ts`, a menu-bar entry and a palette command, then
  the row in the `docs/technical.md` desktop shortcut table (`docs-parity.test.ts` fails otherwise:
  every default key and label must appear, and the table may not list a `Ctrl+…` / `Alt+…` key nothing
  defines).

## Terminal

- `components/terminal-panel.ts` owns every xterm instance (`terminals` map, `TerminalInstance`);
  `activeTerminalInstance()` is the active pane's terminal. Addons: fit, search, webgl,
  `@xterm/addon-web-links` (handler gates on Ctrl/Cmd and calls `ipc.openExternalUrl`; `hover` / `leave`
  keep `instance.hoveredLink` for the context menu), `@xterm/addon-unicode11`
  (`terminal.unicode.activeVersion = "11"`).
- `ensureFontsReady()` waits for `document.fonts.load(--font-mono)` (2 s cap) before the FIRST
  `terminal.open()` — xterm measures its cell grid once at open.
- Right-click → `openContextMenu` with `terminalMenuItems(instance)` (the document-level `contextmenu`
  blocker in `main.ts` only kills the native menu). `onBell` → `tab-bar.ts flashTabChip(contentId)`
  (`.ws-tab--bell`, keyframes `bell-flash`); `onTitleChange` → `appState.applyTerminalTitle(tabId, title)`
  → `TabShellState.title`; pane titles are patched in place on `tab-shell-state-changed`
  (`refreshPaneTitles`), never a full re-render.
- **URLs**: `commands/system.rs::open_url(url)` is the ONE way to open a URL (http(s) only, refused
  otherwise, through `tauri_plugin_shell::ShellExt` — `#[allow(deprecated)]`; migrate to
  `tauri-plugin-opener` app-wide, not per call site). Frontend: `ipc.openExternalUrl`. Never
  `window.open` inside the webview.
- **Clipboard**: `ipc.clipboardCopy` / `clipboardPaste` go through `@tauri-apps/plugin-clipboard-manager`
  first; `commands/clipboard.rs` (`wl-copy` / `xclip` / `pbcopy` spawns) is only the FALLBACK when the
  plugin errors. `copy-on-select.ts createSelectionCopier()` is the pure "dirty on every
  `onSelectionChange`, ONE write on mouse-up" machine; the flush runs on a `setTimeout(0)` after a
  document-level mouseup because xterm's own mouseup fires after ours. Auto-copy is silent; only explicit
  Copy toasts. Gate it on `getTerminalSettings().copyOnSelect`. Middle-click: `mousedown(1)` arms, a
  native `paste` event (WebKitGTK primary selection) is consumed by the capture-phase paste handler,
  `auxclick(1)` falls back to `ipc.clipboardPaste()` after `MIDDLE_PASTE_GRACE_MS` when no native paste
  arrived; nothing while `terminal.modes.mouseTrackingMode !== "none"`. The custom key handler
  `preventDefault()`s its copy/paste chords so WebKit never doubles them.
- **Search bar**: per-instance `SearchState { query, regex, caseSensitive }` (remembered per tab);
  `searchAddon.onDidChangeResults` drives the `n/m` counter — it only fires when `decorations` (from
  `themeEngine.searchDecorations()`, #RRGGBB) are passed to `findNext` / `findPrevious`.
  `openTerminalSearch(tabId?)` refocuses an already open bar.
- **Literal-next** (`literal-next.ts`, pure): `armLiteralNext(tabId)` / `disarmLiteralNext()` /
  `literalNextVerdict(e)` / `consumeLiteralNext(e)` / `isLiteralPass(e)`. `handleGlobalKeydown` steps
  aside while armed (modifier keys keep it armed, Esc or the chord cancel, any other key is consumed and
  left to propagate); xterm's key handler returns `true` for `isLiteralPass(e)`; the textarea `blur` and
  `destroyTerminal` disarm. `pane-view.ts refreshLiteralHint()` (via `onLiteralNextChange`) shows
  `.pane-literal-hint`. Actions `toggleLiteralNext()` / `clearActiveTerminal()` (shortcuts `literal-next`
  `Ctrl+Shift+E`, `terminal-clear` `Ctrl+Shift+K`; Edit menu + palette).
- **Settings ▸ Terminal** (`terminal-settings.ts`): `TerminalSettings` (fontFamily, fontSize, lineHeight,
  scrollback, cursorStyle, cursorBlink, copyOnSelect), `normalizeTerminalSettings`,
  `terminalSettingsDiff`, `xtermOptionsFor(settings, zoom, fallbackFont)`, `effectiveTerminalFontSize`
  (pure) + store glue `getTerminalSettings()` / `setTerminalSettings(patch)` / `resetTerminalSettings()`
  — ONE object under the `terminal` settings key holding only non-default fields.
  `themeEngine.updateAllTerminals(zoom?)` is the single path pushing theme + options to every live xterm
  (refits when cell metrics change); `applyTerminalZoom` delegates to it, so font size composes as
  `setting × zoom` (`terminalFontSizeFor(zoom, base)` in `zoom.ts`); `createTerminal` reads the same
  option set. Dialog section: `dialogs/terminal-settings-section.ts` (`buildTerminalSettingsSection()`
  → `{ el, reset }`, styles in `styles/terminal-settings.css`).

## PTY output & perf invariants

- `pty_output.rs`: every reader (`RawLocalPty`, `RawRemotePty`) sends `OutMsg::Data` / `Exit` to a per-tab
  `OutputBatcher`; its emitter thread (`spawn_emitter`) ships ≤ `BATCH_WINDOW` (8 ms) / ≤
  `BATCH_MAX_BYTES` (64 KB) batches through `PtyOutputSink` (Tauri-managed state) — a raw
  `Channel<InvokeResponseBody>` the frontend registers once via `register_pty_output_channel`
  (frame = `len(tab_id) as u8 · tab_id · bytes`, `encode_frame` ↔ `pty-frame.ts decodePtyFrame`) —
  with the base64 `pty-output` event as the fallback. **Coalescing contract** (unit-tested with a fake
  reader): first byte of a batch waits ≤ 8 ms, a batch never exceeds 64 KB unless one read did, bytes stay
  in order, `pty-exit` is emitted by the emitter AFTER the last batch — never emit output or exit straight
  from a reader thread. Structured events (`pty-shell-event`, `pty-agent-event`) stay Tauri events.
- Frontend: base64 (`decodeBase64Bytes`, indexed loop — never `Uint8Array.from(atob(), cb)`) is only the
  fallback. `terminal-panel.ts deliverOutput` is the ONE entry for bytes: feeds xterm when
  `instance.visible`, else queues in `HiddenOutputBuffer` (`mount-policy.ts`, 2 MB cap then written
  anyway — never drop); `mountTerminalInto` replays the queue before the fit. `fitTerminal` is a no-op for
  a hidden instance; PTY resizes go through `scheduleResizePty` (skip when `rows×cols` unchanged, one
  `resizePty` per instance per animation frame, `flushPendingResizes()` on the divider mouseup).
- **Pane render invariants** (`pane-view.ts`): `render()` runs only on `pane-tree-changed` and
  `active-workspace-changed` and RECONCILES (`reconcileNode`: elements reused by `data-pane-id`,
  `patchLeaf` swaps content / exited header in place, retired elements go through
  `detachPanelElements` so their panels land in `#pane-holding`). `active-tab-changed` / `tabs-changed`
  only refresh the tab strip (+ pane titles); `active-pane-changed` toggles `.active` and
  `focusActivePane()`. A new emitter that changes the tree MUST emit `pane-tree-changed`; nothing else
  may trigger a render. `mountTab(tab, host, wsIdx, { focus })` — only the active pane's content gets
  `focus: true` (`shouldFocusOnMount`); `mountTerminalInto` calls `ipc.resyncPty` only on the FIRST mount
  of a content (`shouldResync(contentId, mountedOnce)`).
- Dev counters: `perf-counters.ts perfCount(name)` → `window.__pikiPerf.counters` (`pane.render`,
  `pane.buildLeaf`, `terminal.mount`, `terminal.resync`, `terminal.resizePty`, `pty.batch`, `pty.bytes`,
  `pty.buffered`, `pty.fallbackEvent`) — no-ops in production.

## Git panel

- `commands/git.rs`: `git_pull` (aborts a conflicting pull and names the files, passes a diverged refusal
  through), `git_amend(message: Option)`, `git_last_commit_message`, `git_discard_file(path, untracked)`
  (`restore --worktree` vs `clean -fd`; the flag is the caller's explicit choice),
  `git_list_branches(include_remotes)`, `git_checkout_branch(branch, remote)` — never `-f`, git's refusal
  is the error. Parsing/summary helpers live in `piki_core::git` with unit tests (`parse_branch_list`,
  `parse_ahead_behind`, `pull_summary`, `BRANCH_LIST_FORMAT`).
- `in-flight.ts runExclusive(key, fn)` / `isInFlight(key)` / `onInFlightChange(cb)` — the one-op-at-a-time
  guard for any user-triggered async action that must not run twice (push, pull, checkout): while `fn` is
  pending under `key` every other call resolves to `undefined` without running (the caller toasts
  "… already in progress"); a rejection releases the key. A panel re-renders its busy buttons from
  `onInFlightChange` + `isInFlight`.
- `components/git-actions.ts pushWorkspace(wsIdx)` / `pullWorkspace(wsIdx)` / `confirmDiscardFile(wsIdx,
  file, onDone)` / `refreshGitStatus(wsIdx)` — the ONE implementation of push / pull / discard used by the
  panel, the Git menu and the palette (guard keys `pushKey(wsIdx)` / `pullKey(wsIdx)`). Never call
  `ipc.gitPush` / `ipc.gitPull` from UI code directly.
- `source-control.ts focusCommitBox({ amend? })` is the entry for "Commit" / "Amend Last Commit"
  (module-level `amendMode` survives full re-renders). `dialogs/branch-picker.ts openBranchPicker()` is
  the branch switcher (palette-style overlay, `fuzzyScore`, `git-checkout:<ws>` guard; `switch-branch`
  shortcut, status-bar branch click, Git menu, palette).

## Search & file index

- `commands/search.rs fuzzy_file_list` returns `piki_core::search::FileIndex { files, truncated }`
  (workspace-relative, sorted). **Per-workspace cache pattern**: memoised on
  `DesktopWorkspace.file_index: Option<Arc<FileIndex>>`; the command reads the cache under the lock, walks
  off-lock in `spawn_blocking` (`piki_core::search::list_files`, `ignore::WalkBuilder` — `.gitignore` /
  `.ignore` / global excludes, hidden kept, `.git` pruned, `FILE_INDEX_CAP` = 50k) and stores it only if
  the workspace at that index still has the same path. Invalidation lives where the change is observed:
  `events::spawn_git_watcher` clears it when the drained events are anything but content edits of files
  already in the index (`FileIndex::contains`); `workspace.rs::switch_workspace` clears it on switch.
  Reuse this shape for any derived-from-disk list.
- `components/fuzzy-search.ts` renders before the IPC resolves (last list per workspace path shown
  meanwhile, `Indexing…` in `.palette-footer`); `Enter` / click → editor tab via `openFileInEditor`,
  `Alt+Enter` → read-only viewer, `Ctrl+E` → `$EDITOR`. `file-kind.ts` (`looksBinary`, `isMarkdownPath`,
  pure) decides which files never get an editor tab.
- `components/fuzzy.ts` — `fuzzyScore` / `fuzzyScorePath` for palette-style overlays (command palette,
  fuzzy file search, branch picker, switcher).

## Settings & shared app settings

- `settings.ts` / `settings-store.ts` — the ONLY way to read or write the `settings` JSON document
  (`settings` row of `UiPrefsStorage`). `await settingsStore.load()` runs first in `main.ts`; afterwards
  `settingsStore.get(key)` is synchronous and `settingsStore.patch(key, value)` (`undefined` deletes)
  schedules one debounced full-document write. Never call `ipc.getSettings` / `ipc.setSettings` from a
  component (the old per-caller read-modify-write raced). Keep the store IPC-free (`settings-store.ts`
  takes a `SettingsBackend`; `settings.ts` binds it to Tauri) so it stays unit-testable. Keys in use:
  `shortcuts`, `shell`, `terminal`, `uiZoom`, `appearance.density`, `settingsTab`, `wsTabsV2`,
  `workspaceMru`, `agentsPanelHeight`, sidebar/chat widths, file-tree state, `chat_config`.
- **Settings dialog** (`dialogs/settings-dialog.ts`) is only the shell: a left rail (`role=tablist`,
  ↑/↓/Home/End) + one panel, last tab remembered as `settingsTab`, footer *Reset <tab>* / *Restore
  Defaults* (`showConfirm`, danger; keeps the shell command + provider binaries — say so in the confirm
  if you add a reset) / Close. Every tab is a `SettingsSection { el, reset(), focus() }`
  (`dialogs/settings-controls.ts`: `settingsSection(title)`, `settingsGrid(parent).row(label, control)`,
  `settingsCheckbox`, `settingsHint`, `sourceBadge(overridden)`). Tabs: `general-settings-section.ts`,
  `appearance-settings-section.ts`, `shortcuts-settings-section.ts`, and Terminal (shell row +
  `terminal-settings-section.ts`) built inside `settings-dialog.ts`. Styles: `styles/dialog-settings.css`
  (`.settings-rail-tab`, `.settings-grid*`, `.settings-check`, `.settings-source`,
  `.settings-flag[data-kind=conflict|demoted]`, `.settings-toolbar`). Shortcut `settings` is `Ctrl+,`.
- Shortcuts tab logic is pure: `shortcut-table.ts` (`findConflicts(defs, reserved)` → id → other labels,
  `matchesShortcutQuery`, `groupByCategory`) over `getShortcuts()` / `getReservedCombos()` /
  `isOutsideOnly` / `isDemotedShortcut`.
- **Adding a setting**: pick the tab, add a row through `settingsGrid`, persist with `settingsStore.patch`
  (desktop-only) or the core override below; if it deserves a default, wire the tab's `reset()`.
- **Core setting override pattern** (a setting the TUI must honour too): field on
  `piki_core::app_settings::AppSettings` (+ merge + tests there) → `AppSettingsView`
  (`commands/settings.rs`, `get_app_settings` / `set_app_settings(overrides)` — the whole document each
  time, `None` = back to `config.toml`; apply live what can apply live, say "on restart" for the rest) →
  `ipc.getAppSettings` / `setAppSettings` → a row in the General tab with `sourceBadge`. Never write
  `config.toml` from the UI.
- **Density**: `density.ts` — `appearance.density` = `"compact" | "normal" | "comfortable"` (default
  `normal`), `applyDensity()` sets `document.documentElement.dataset.density` (attribute removed for
  `normal`), `initDensity()` runs in `main.ts` after `initUiZoom()`. The CSS side is in *CSS tokens*.

## Stylesheets

- `styles/index.css` is the single import (from `main.ts`) and its `@import` order is the cascade — add a
  new sheet there, never as a `<link>` in `index.html`. Foundation order: `fonts` → `variables.css` →
  `reset.css` → `primitives.css` → `motion.css`, then the shell and feature sheets (one per panel).
- Dialog family: `dialog-core.css` (backdrop, `.dialog` size budget, body/field/label, footer,
  `.dialog-dropdown-*`, badges, the `showConfirm` overlay) followed by `dialog-providers.css`,
  `dialog-agent.css`, `dialog-logs.css`, `dialog-help.css`, `dialog-workspace.css`, `dialog-git.css`,
  `toast.css`, `file-viewer.css`, `dialog-settings.css` — a new dialog's own rules go in a new
  `dialog-<feature>.css` imported in that group.
- All `@keyframes` live in `motion.css` (also the global `prefers-reduced-motion` block); feature CSS only
  references animation names (`overlay-in`, `dialog-enter`, `menu-enter`, `toast-in`, `pulse`,
  `bell-flash`, …).
- `css-invariants.test.ts` (reads the sheets with `node:fs`, typed by `test-node-shim.d.ts`) fails on:
  `outline: none` outside a `:focus-visible` rule that paints a replacement; a `z-index` that is not a
  `--z-*` token; a colour literal outside `variables.css`; `transition: all`; a sheet missing from
  `index.css`; a font name outside its token; a height token not scaled by `--density`.

## CSS tokens & colour rules

`styles/variables.css` is the token layer; feature CSS never repeats a literal font, size, z-index or
duration. Adding a token: define it in `variables.css` under the matching section with a one-line comment
on where it is used, consume via `var(--…)`, and if TS needs the literal go through `cssToken()`.

- **Fonts**: `--font-mono` is the ONLY place `"JetBrainsMono NF Mono"` appears in `styles/`, and
  `--font-ui` the only place `"piki-icons"` does (the `@font-face` blocks live in `src/fonts/fonts.css`).
  `--font-ui` = `"piki-icons", var(--font-mono)`. TS that must hand a literal to a non-CSS consumer reads
  it with `cssToken("--font-mono", fallback)` from `theme.ts`; inline `style=` uses
  `font-family:var(--font-mono)`.
- **Type scale & spacing**: `--font-size-base` (12.5px, `html`), `--fs-3xs…--fs-4xl`
  (7/9/10/11/12/12.5/13/14/16/18/20px), `--fs-display-sm/--fs-display/--fs-display-lg` (28/32/48px);
  `--leading-none/tight/normal/relaxed/loose`; `--sp-1…6` (2/4/6/8/12/16px, for `gap`);
  `--radius-xs/sm/md/lg/full`; `--control-height` (30px), `--control-height-sm` (22px), `--header-height`
  (44px); focus `--focus-ring-width` (2px), `--focus-ring`, `--focus-ring-error`, `--focus-ring-inset`.
- **Z layers**, low → high: `--z-canvas` (0) < `--z-panel` (1, sticky headers / resize handles;
  `calc(var(--z-panel) + 1)` to beat another sticky header) < `--z-sticky` (10, bars over panel content)
  < `--z-backdrop` (100, every modal backdrop incl. `showConfirm`) < `--z-dialog` (110, a modal stacked
  on an open dialog — set from TS as `el.style.zIndex = "var(--z-dialog)"`) < `--z-menu` (200, menu-bar
  dropdowns; click-away backdrop `calc(var(--z-menu) - 1)`) < `--z-popover` (300, context menus and
  `createDropdown` lists) < `--z-toast` (400) < `--z-tooltip` (500). Every `z-index` must be a
  `var(--z-*)` or a `calc()` of one.
- **Motion**: `--dur-fast/--dur-base/--dur-slow` (0.1/0.2/0.35s), `--dur-blink` (0.8s), `--dur-pulse`
  (1.5s); `--transition-fast/normal/slow`. Never `transition: all`.
- **Derived colours**: `computeDerived()` in `theme-derive.ts` (pure) recomputes on every theme apply:
  glows/muted tints, `--on-accent` and `--activity-bar-badge-fg` (WCAG luminance), `--selection-bg`,
  `--activity-bar-badge-glow`, the twelve `--icon-*` file-type colours, the kanban palette.
  `variables.css` keeps a static Obsidian default for each so the first paint is right. A preset colour
  must be in `ThemeColors` + `COLOR_GROUPS` + all five presets; don't add a key the CSS never consumes
  (`--statusbar-no-folder` paints the status bar with no workspace via `#app:not(:has(.workspace-item))`,
  `--activity-bar-badge` the Source Control count badge).
- **Colours never assume a dark background.** No `rgba()` / `rgb()` / hex literal outside
  `variables.css` — `grep -rnE 'rgba?\(|#[0-9a-fA-F]{3,8}\b' src/styles/ | grep -v variables.css` stays
  empty. A tint is `color-mix(in srgb, var(--token) N%, transparent)`; scrims are `--scrim` (dialog
  backdrops) / `--scrim-strong` (in-panel overlays) / `--scrim-soft` (palette); elevation is
  `--shadow-sm/md/lg`; a hairline is `var(--border-subtle)`; text over `--error-color` is `--on-error`;
  the secondary hue is `--accent-alt`; the iframe canvas is `--web-preview-canvas`; semantic aliases
  `--success-color` / `--error-color` / `--warning-color` and `--on-accent`. Kanban column colours are
  `--kanban-col-todo/in-progress/in-review/done`, swatches `--kanban-swatch-1…16` (`kanban-panel.ts`
  stores the `var()` string). A glyph that must follow the text colour is a text glyph or a
  `currentColor` gradient, never an SVG data-URI with a baked fill. Never import a shipped
  `highlight.js/styles/*.css`: `hljs-theme.ts` generates the `.hljs-*` block from the palette and
  `applyTheme` injects it as `<style id="hljs-theme">`.
- **Tone**: `applyTheme` stamps `data-theme-tone="dark|light"` on `<html>` from `themeTone()` (luminance
  of the effective `--bg-primary`, not the preset flag). Tone-dependent constants live in the
  `:root[data-theme-tone="light"]` block of `variables.css` (scrims, shadows) and `reset.css` (noise
  texture is dark-only); palette-dependent ones go through `computeDerived`.
- **Density**: `--density` (1) is redefined by `:root[data-density="compact"]` (0.8) and
  `:root[data-density="comfortable"]` (1.15). EVERY height token is `calc(<rem> * var(--density))`:
  `--menubar-height`, `--statusbar-height`, `--tab-height`, `--sidebar-header-height`, `--control-height`,
  `--control-height-sm`, `--header-height`, `--pane-header-height` (24px, `pane.css .pane-head`),
  `--row-height` (23px: `.ft-row`, `.file-item`, `.sc-subdir-item`), `--row-height-lg` (32px:
  `.workspace-item`), `--row-pad` (5px vertical padding of the two-line `.agent-row`). Rows set
  `min-height` + zero vertical padding so the multiplier changes the row count. A new bar/row height
  must be one of these or a new `calc(… * var(--density))` token; widths, `--fs-*` and `--sp-*` never
  scale with density.
- **Zoom**: `--ui-zoom` on `<html>` (`ui-zoom.ts`, persisted as `settings.uiZoom`, levels in the pure
  `zoom.ts`) multiplies `--font-size-base`; `--fs-*`, `--sp-*`, bar heights, `--activity-bar-width` and
  the activity-bar glyphs are rem so they scale — a new size that should zoom must be rem (px is for
  user-dragged budgets like `--sidebar-width`). Terminal font = `terminalFontSizeFor(zoom, base)`;
  `themeEngine.applyTerminalZoom()` → `updateAllTerminals(zoom)`. Shortcuts `zoom-in/out/reset` (bare
  `Ctrl+=`/`-`/`0`, outside-only — `Ctrl+-` is readline undo) and `zoom-*-terminal` (`Ctrl+Shift+…`).
- **Layout budget**: the sidebar and chat drag clamps both call `layout-budget.ts` (`clampSidebarWidth` /
  `clampChatWidth` with `visibleChatWidth()` / `visibleSidebarWidth()` / `activityBarWidth()`) so the
  editor column keeps `EDITOR_MIN_WIDTH` = `--editor-min-width` (320px); `layout.css` enforces the same
  floor with `minmax()` and caps the sidebar at `50vw`. `CHAT_OVERLAY_BREAKPOINT` (1000) must equal the
  `@media (max-width: 1000px)` in `layout.css`, below which the chat floats over the editor. Status-bar
  segments are block-level (`text-overflow: ellipsis`); only `.status-cwd/.status-agent/.status-lsp` shrink.

## UI primitives & dialogs

`styles/primitives.css` (token-built, modifiers wrapped in `:where()` so a feature rule of equal
specificity always wins). Add the primitive class next to the feature's own hook class and keep only
feature-specific modifiers in feature CSS; never re-declare a button/input/surface from scratch.

- `.ui-surface` — floating shell (bg, border, radius, `--shadow-lg`, flex column, `dialog-enter`). The
  caller owns the size: `<div class="dialog ui-surface">` (`.dialog` = width/max-height only); also
  `.palette`, `.diff-viewer`, `.kanban-edit-modal`, `.ws-delete-dialog`.
- `.ui-header` — title row (`min-height: var(--header-height)`) with `.ui-header-title` (ellipsis) and an
  optional `.ui-header-actions` slot (right-aligned, never shrinks).
- `.ui-btn` — `data-variant="primary|secondary|ghost|danger"`, `data-size="sm|md"` (md =
  `--control-height`, sm = `--control-height-sm`), `data-icon` (square glyph button), `.active`,
  `:disabled`, `[aria-busy="true"]` (pulses). Focus ring is an outset outline. From TS:
  `btn.className = "ui-btn"; btn.dataset.variant = "primary"; btn.dataset.size = "sm"`
  (`dataset.icon = ""` for `data-icon`).
- `.ui-input` — `<input>` / `<textarea>`: same height as `.ui-btn`, `--bg-input`, accent border +
  `--focus-ring` halo on `:focus-visible` (the only sanctioned `outline: none`), `[aria-invalid="true"]`,
  `[readonly]` / `:disabled` dimmed. `.dialog-dropdown-trigger` (`createDropdown`) shares the height.
- `.ui-empty` — centered muted block: `.ui-empty-icon`, `.ui-empty-title`, `.ui-empty-hint`,
  `.ui-empty-cta` (a `.ui-btn`), `data-fill`, `data-tone="error|loading"`. Every "No X" / "Loading…" /
  "Failed to …" message is one of these.
- **Keyboard focus is visible everywhere**: `reset.css` paints `--focus-ring-width` of `--accent-focus`
  as an inset outline on any `:focus-visible` element; `.ui-btn` moves it outside; text fields use the
  border + halo; full-pane editors (`.api-editor`, `.file-viewer-textarea`, the markdown `.ProseMirror`)
  use `--focus-ring-inset`. Not migrated on purpose (chrome, not controls): `.pane-btn`,
  `.ws-tab-close/-add/-more`, `.wc-btn`, `.mk-toolbar-btn`, `.chat-code-copy`, `.kanban-color-swatch`,
  menu/activity/status bars — they get the global ring.
- Shared components — use these, never re-implement them:
  - `components/confirm.ts showConfirm()` — every destructive/confirm overlay (`.ws-delete-confirm`
    markup, Escape/Enter, focus trap, safe-button initial focus, focus restore).
  - `components/toast.ts reportError(context, err)` — every user-initiated failure path (toast +
    `console.error`). A bare `console.error` is only for background/polling failures.
  - `components/a11y.ts makeInteractive(el, role?)` — any clickable `div` row/tab (tabindex + role +
    Enter/Space).
  - `components/context-menu.ts openContextMenu(x, y, items)` — every right-click / `⋯` menu
    (viewport-clamped, Esc / click-outside / blur close, Arrow/Home/End/Tab focus, focus restore;
    `CtxItem { label, action, danger, disabled, separator }`, styles in `context-menu.css`).
  - `components/dropdown.ts createDropdown(options, initial)` — the only select (never a native
    `<select>`): `.dialog-dropdown-trigger` sized like `.ui-input`, list at `--z-popover`.
- **How to add a dialog**: `backdrop.className = "dialog-backdrop <family>-backdrop"` (open() removes only
  `.<family>-backdrop` — removing bare `.dialog-backdrop` destroys unrelated open dialogs);
  `dialog.className = "dialog ui-surface"` (+ a `.<family>-dialog` width class in `dialog-<family>.css`
  if 560px is wrong); `.ui-header` with `.ui-header-title` + `.dialog-close ui-btn` (ghost, `data-icon`);
  `.dialog-body` of `.dialog-field` (`.dialog-label` + `.ui-input` / `createDropdown()` /
  `attachPathPicker`); `.dialog-footer` with secondary Cancel first, primary action last; destructive
  flows go through `showConfirm` instead. Escape/Enter and the close button wire up as in
  `dialogs/stash-dialog.ts`.

## Icons & fonts

- **Icons** (`components/icons.ts`) — the ONE icon set: inline SVG on a 16px grid, `currentColor` strokes
  (1.5) on the paths, sized 1em by `styles/icons.css`. `icon(name, { class?, label?, size? })` returns
  markup for `innerHTML` templates (`aria-hidden` unless `label` makes it `role="img"`); `iconEl()`
  returns an `SVGElement`; `IconName` is the union of `ICONS` keys. Names: `check warning folder gear eye
  close pencil undo history refresh more dot circle chevron-right chevron-down arrow-up/down/left/right
  branch play clock plus split-right split-down locate search`. Rules: one pencil for every edit/rename,
  `refresh` for refresh/reload/restart, `undo` for discard/reset, `history` for "restored from the
  daemon", `dot`/`circle` for alive/exited; an icon-only button keeps `title` + `aria-label`;
  `types.ts` status views (`cliAgentStatusView`, `actionableStatusView`) return an `icon: IconName` and
  the renderer calls `icon(v.icon)`. Emoji / dingbats are banned from `components/**` and `types.ts` by
  `components/icons.test.ts` (allow-list with a reason per file — today only `file-icons.ts`, the Nerd
  PUA table). Prose keeps text (toast/confirm bodies, tooltips — `×`, `…`, `·`, `↑N`/`↓N` counts are
  typography). A context rule like `.status-item svg { fill: currentColor }` cannot fill a stroke icon.
- **Fonts** (`src/fonts/`) — `JetBrainsMonoNerdFontMono-{Regular,Bold,Italic}.woff2` (≈1.0 MB each,
  lossless WOFF2 of the upstream Nerd Font TTFs — the terminal needs FULL PUA coverage, so this face is
  never subset) + `piki-icons.woff2` (≈10 KB: the Regular face cut down to the PUA code points
  `file-icons.ts` uses, `unicode-range: U+E000-F8FF`, `font-weight: 100 900`, first in `--font-ui`; same
  metrics as the full face). No plain (non-NF) JetBrains Mono text face is shipped on purpose. Adding a
  glyph to `file-icons.ts` means regenerating `piki-icons.woff2`. Regenerate (fonttools is not
  system-wide; `woff2_compress` is):
  ```bash
  python3 -m venv /tmp/ft && /tmp/ft/bin/pip install fonttools brotli
  cd crates/desktop/frontend/src/fonts
  for s in Regular Bold Italic; do woff2_compress /path/to/JetBrainsMonoNerdFontMono-$s.ttf; done   # lossless
  # PUA list = every U+E000-U+F8FF code point in components/file-icons.ts:
  U=$(python3 -c "import re;s=open('../components/file-icons.ts',encoding='utf8').read();print(','.join(sorted({'U+%04X'%ord(c) for c in s if 0xE000<=ord(c)<=0xF8FF})))")
  /tmp/ft/bin/pyftsubset /path/to/JetBrainsMonoNerdFontMono-Regular.ttf --unicodes=$U --flavor=woff2 --output-file=piki-icons.woff2
  ```

## Chat

- `chat-context.ts` is the pure layer: `fenceBlock(kind, meta, text)` (header line + fence that outgrows
  any backtick run, `truncateLines` at `CONTEXT_MAX_LINES` = 200 with `TRUNCATED_MARKER`),
  `appendToDraft`, `diffLinesToText` (`get_file_diff` `DiffLine[]` → unified text), `contextChoices`,
  `parseToolMessage` (history's `[name] [Error] text` back to card data), `prettyJson`,
  `formatDurationMs`, `ToolCard`.
- `chat-panel.ts addContextToChat()` (shortcut `add-chat-context` `Ctrl+Shift+I`, category Chat,
  `terminalCapture`; Chat menu; palette; the composer's `+`) injects a terminal selection directly, else
  `openContextMenu` chooser → `buildContextBlock(kind)`; resolves the active pane's content itself
  (`appState.activeTabTree` + `allLeaves`), reads editor selections via `code-editor-panel.ts
  getCodeEditorSelection(tabId)`, opens the panel with `ensureChatVisible()` (the overlay-under-1000px
  rule lives in `layout.css`). Chat config persists under the `chat_config` settings key.
- **Approval flow**: `commands/chat.rs` forwards the agent loop as `"chat-agent-event"` (`tool-calls` /
  `tool-executing` / `tool-result` / `approval-required`, `ipc.onChatAgentEvent`) next to the
  `"chat-token"` stream; an `ApprovalRequired` parks its oneshot in `DesktopApp.chat_pending_approvals`
  (by tool call id) and `chat_approve(tool_call_id, decision: allow|deny|allow_all)` (`ipc.chatApprove`)
  answers it via `resolve_approval` (unit-tested). Dropping a sender is a Deny — `chat_send_agent_message`,
  `chat_stop` and `chat_clear` clear the map — and the loop times out after 300 s; the frontend mirrors
  that with `settlePendingApprovals` and keeps its 60 s stream watchdog from firing while a card waits.
  Cards: `renderToolCard` (`<details class="chat-tool-card" data-status>`, icons from `icons.ts`, results
  folded past `RESULT_COLLAPSE_LINES`); history keeps the TUI's `[name] [Error] text` content (with
  `tool_call_id`, capped at 4000 chars) so a reload still renders cards.
