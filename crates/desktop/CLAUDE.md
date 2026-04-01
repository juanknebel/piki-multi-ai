# piki-desktop

Tauri v2 desktop GUI for piki-multi. **Depends on `piki-core` and `piki-api-client`.**

## Architecture

- `src/main.rs` — Tauri entry point, setup, command registration.
- `src/state.rs` — `DesktopApp`, `DesktopWorkspace`, `DesktopTab` state structs (managed by Tauri).
- `src/pty_raw.rs` — `RawPtySession`: streams raw PTY bytes to frontend via Tauri events (no `vt100`).
- `src/events.rs` — Tauri event emission helpers (sysinfo, git refresh, toast).
- `src/commands/` — Tauri IPC commands: `workspace.rs`, `pty.rs`, `git.rs`, `system.rs`.
- `frontend/` — Vanilla TypeScript + xterm.js web frontend, built with Vite.

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
