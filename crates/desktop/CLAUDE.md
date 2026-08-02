# piki-desktop

Tauri v2 desktop GUI for piki-multi. **Depends on `piki-core`, `piki-api-client`, and `piki-agent`.**

## Architecture

- `src/main.rs` — Tauri entry point, setup, command registration.
- `src/state.rs` — `DesktopApp`, `DesktopWorkspace`, `DesktopTab` state structs (managed by Tauri).
- `src/pty_raw.rs` — `RawPtySession`: streams raw PTY bytes to frontend via Tauri events (no `vt100`).
- `src/events.rs` — Tauri event emission helpers (sysinfo, git refresh, toast).
- `src/commands/` — Tauri IPC commands: `workspace.rs`, `pty.rs`, `git.rs`, `system.rs`, `chat.rs` (includes `chat_send_agent_message` for agentic tool-use, `chat_set_agent_mode`/`chat_get_agent_mode`), `lsp.rs` (LSP server lifecycle commands).
- `src/lsp/` — LSP WebSocket proxy: `registry.rs` (server config from `lsp.toml`), `server.rs` (`LspManager` with TTL idle shutdown, max concurrent cap), `proxy.rs` (WebSocket server bridging JSON-RPC to child processes). `LspManager` uses `tokio::sync::Mutex` (not `parking_lot`) for async server spawn operations.
- `frontend/` — Vanilla TypeScript + xterm.js + CodeMirror 6 web frontend, built with Vite. Frontend-only tab types (Markdown, CodeEditor, WebPreview) use synthesized tab ids (e.g. `web-${Date.now()}`) and `appState.addTab` without calling `ipc.spawnTab`; their close handlers in `tab-bar.ts` skip `ipc.closeTab` and destroy local state directly. Shell integration: `pty_raw.rs` runs an `OscParser` on shell-tab byte streams and emits `pty-shell-event` Tauri events (cwd, command exit codes); the frontend listens in `main.ts` and updates `appState.applyShellEvent` so the status bar shows cwd and the tab bar shows ✓/✗ badges. Provider-tab idle watcher polls in `events::spawn_idle_watcher_loop` and emits `pty-attention` events that mark workspace badges. Agents panel: `agents-panel.ts`, always visible below the workspace list inside `#explorer-view` (not a switchable view; the activity-bar "agents" icon opens the profile-manager dialog), height resizable via the `#agents-resize-h` drag handle in `sidebar.ts` (clamped 32px–75% of the explorer, persisted as `settings.agentsPanelHeight`), lists live agent tabs across ALL workspaces from `commands::agents::list_agent_rows` (backend-sourced because the frontend only hydrates a workspace's tabs on first visit); click jumps via `switchWorkspace` + `setActiveTab`. The workspace list (`workspace-list.ts`) renders a per-row agent rollup glyph — collapsed worktree families aggregate hidden children by `family_key`/`source_repo` — using `types.ts::agentStatusSeverity`/`actionableStatusView` (mirrors of `piki_core::cli_agent::status_severity` and the TUI's `actionable_status_view`). Viewing a tab acknowledges its agent's attention marker backend-side (`set_active_tab` / `switch_workspace`). Keyboard shortcuts: `shortcuts.ts` is the single registry (rebindable defs with `category` + non-rebindable `fixedShortcuts`); the help dialog derives from `helpSections()` and the menu bar reads keys via `getShortcutKey` — never hardcode a key string in either.

## Build

```bash
cd crates/desktop/frontend && npm install && cd -
cargo build -p piki-desktop
```

## Conventions

- PTY output is base64-encoded for JSON transport via Tauri events.
- All Tauri commands lock `Mutex<DesktopApp>` — scope the lock guard in a `{ }` block and drop before `.await`.
- Frontend components are classes managing their own DOM subtree in `frontend/src/components/`.
- CSS uses custom properties defined in `frontend/src/styles/variables.css`.
- Domain types mirrored in `frontend/src/types.ts` — keep in sync with `piki-core::domain`.
