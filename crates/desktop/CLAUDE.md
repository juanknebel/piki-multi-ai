# piki-desktop

Tauri v2 desktop GUI for piki-multi. **Depends on `piki-core`, `piki-api-client`, and `piki-agent`.**

## Architecture

- `src/main.rs` — Tauri entry point, setup, command registration.
- `src/state.rs` — `DesktopApp`, `DesktopWorkspace`, `DesktopTab { custom_title: Option<String> }` + `TabInfo { custom_title }` with `display_label()`/`getTabLabel()` (`custom_title` > `provider.label()`, 40-char cap) and `rename_tab` Tauri command; state structs managed by Tauri. `TabInfo.custom_title` is set via `rename_tab` (workspace_idx + tab_id + Option<title>) and mirrored in `frontend/src/state.ts:renameTab()` + `frontend/src/types.ts:getTabLabel()`.
- `src/pty_raw.rs` — `RawPtySession`: streams raw PTY bytes to frontend via Tauri events (no `vt100`).
- `src/events.rs` — Tauri event emission helpers (sysinfo, git refresh, toast).
- `src/commands/` — Tauri IPC commands: `workspace.rs`, `pty.rs`, `git.rs`, `system.rs`, `chat.rs` (includes `chat_send_agent_message` for agentic tool-use, `chat_set_agent_mode`/`chat_get_agent_mode`), `lsp.rs` (LSP server lifecycle commands).
- `src/lsp/` — LSP WebSocket proxy: `registry.rs` (server config from `lsp.toml`), `server.rs` (`LspManager` with TTL idle shutdown, max concurrent cap), `proxy.rs` (WebSocket server bridging JSON-RPC to child processes). `LspManager` uses `tokio::sync::Mutex` (not `parking_lot`) for async server spawn operations.
- `frontend/` — Vanilla TypeScript + xterm.js + CodeMirror 6 web frontend, built with Vite. Frontend-only tab types (Markdown, CodeEditor, WebPreview) use synthesized tab ids (e.g. `web-${Date.now()}`) and `appState.addTab` without calling `ipc.spawnTab`; their close handlers in `tab-bar.ts` skip `ipc.closeTab` and destroy local state directly. Any tab/pane close must go through `tearDownAndCloseWsTab` / `tearDownAndClosePane` (tab-bar.ts): dirty-editor prompt + PTY/panel teardown — never call `appState.closePane`/`closeWsTab` directly from UI code. Shell integration: `pty_raw.rs` runs an `OscParser` on shell-tab byte streams and emits `pty-shell-event` Tauri events (cwd, command exit codes); the frontend listens in `main.ts` and updates `appState.applyShellEvent` so the status bar shows cwd and the tab bar shows ✓/✗ badges. Provider-tab idle watcher polls in `events::spawn_idle_watcher_loop` and emits `pty-attention` events that mark workspace badges. Agents panel: `agents-panel.ts`, docked at the bottom of `#sidebar` below the switchable views wrapper `#sidebar-views` — ALWAYS visible no matter which view (Explorer/Files/Source Control) is active (not a switchable view itself; the activity-bar "agents" icon opens the profile-manager dialog), height resizable via the `#agents-resize-h` drag handle in `sidebar.ts` (clamped 32px–75% of the sidebar, persisted as `settings.agentsPanelHeight`), lists live agent tabs across ALL workspaces from `commands::agents::list_agent_rows` (backend-sourced because the frontend only hydrates a workspace's tabs on first visit, `label` is `DesktopTab::display_label()` so a renamed tab shows its custom title); click jumps via `switchWorkspace` + `setActiveTab`. The workspace list (`workspace-list.ts`) renders a per-row agent rollup glyph — collapsed worktree families aggregate hidden children by `family_key`/`source_repo` — using `types.ts::agentStatusSeverity`/`actionableStatusView` (mirrors of `piki_core::cli_agent::status_severity` and the TUI's `actionable_status_view`). Viewing a tab acknowledges its agent's attention marker backend-side (`set_active_tab` / `switch_workspace`). Tab renaming: top bar `tab-bar.ts` `wsTabTitle()` uses `getTabLabel()` and triggers `triggerRename()` on dblclick/contextmenu (prompt, `appState.renameTab()` + `ipc.renameTab()`); status bar and dashboard also use `getTabLabel()`. Keyboard shortcuts: `shortcuts.ts` is the single registry (rebindable defs with `category` + non-rebindable `fixedShortcuts`); the help dialog derives from `helpSections()` and the menu bar reads keys via `getShortcutKey` — never hardcode a key string in either.

## Build

```bash
cd crates/desktop/frontend && npm install && cd -
cargo build -p piki-desktop
```

## Conventions

- Shared frontend primitives — use these, never re-implement them:
  - `components/confirm.ts` `showConfirm()` — every destructive/confirm overlay (renders `.ws-delete-confirm` markup with Escape/Enter, focus trap, safe-button initial focus, focus restore). Never hand-roll a confirm overlay.
  - `components/toast.ts` `reportError(context, err)` — every user-initiated failure path (error toast + console.error). A bare `console.error` is only acceptable for background/polling failures.
  - `components/a11y.ts` `makeInteractive(el, role?)` — any clickable `div` row/tab (tabindex + role + Enter/Space). Keyboard focus styling comes from the global `:focus-visible` rule in `styles/reset.css`.
  - `components/fuzzy.ts` — `fuzzyScore`/`fuzzyScorePath` + `mruBump`/`mruRank` for palette-style overlays (command palette and fuzzy file search already use them).
- Dialog backdrops are scoped per family: `backdrop.className = "dialog-backdrop <family>-backdrop"` and open() removes only `.<family>-backdrop` — removing bare `.dialog-backdrop` destroys unrelated open dialogs.
- No raw palette hex in feature CSS: use `variables.css` tokens, including the semantic aliases `--success-color`/`--error-color`/`--warning-color` and `--on-accent` (text over accent backgrounds).
- Full re-renders must preserve UI state: keep `scrollTop`, input caret/focus across `innerHTML` rebuilds (see workspace-list, file-tree, source-control for the pattern), and prefer patching in place (status bar LSP segment, kanban search) over rebuilding.
- PTY output is base64-encoded for JSON transport via Tauri events.
- All Tauri commands lock `Mutex<DesktopApp>` — scope the lock guard in a `{ }` block and drop before `.await`.
- Frontend components are classes managing their own DOM subtree in `frontend/src/components/`.
- CSS uses custom properties defined in `frontend/src/styles/variables.css`.
- Domain types mirrored in `frontend/src/types.ts` — keep in sync with `piki-core::domain`.
