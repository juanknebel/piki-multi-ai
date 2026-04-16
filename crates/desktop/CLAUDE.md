# piki-desktop

Tauri v2 desktop GUI for piki-multi. **Depends on `piki-core`, `piki-api-client`, and `piki-agent`.**

## Architecture

- `src/main.rs` — Tauri entry point, setup, command registration.
- `src/state.rs` — `DesktopApp`, `DesktopWorkspace`, `DesktopTab` state structs (managed by Tauri).
- `src/pty_raw.rs` — `RawPtySession`: streams raw PTY bytes to frontend via Tauri events (no `vt100`).
- `src/events.rs` — Tauri event emission helpers (sysinfo, git refresh, toast).
- `src/commands/` — Tauri IPC commands: `workspace.rs`, `pty.rs`, `git.rs`, `system.rs`, `chat.rs` (includes `chat_send_agent_message` for agentic tool-use, `chat_set_agent_mode`/`chat_get_agent_mode`), `lsp.rs` (LSP server lifecycle commands).
- `src/lsp/` — LSP WebSocket proxy: `registry.rs` (server config from `lsp.toml`), `server.rs` (`LspManager` with TTL idle shutdown, max concurrent cap), `proxy.rs` (WebSocket server bridging JSON-RPC to child processes). `LspManager` uses `tokio::sync::Mutex` (not `parking_lot`) for async server spawn operations.
- `frontend/` — Vanilla TypeScript + xterm.js + CodeMirror 6 web frontend, built with Vite.

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
