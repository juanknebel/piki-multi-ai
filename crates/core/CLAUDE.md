# piki-core

UI-agnostic library crate. **Must NOT depend on `crates/tui` or `crates/api-client`.**

## Modules

- `domain.rs` — `AIProvider`, `WorkspaceInfo`, `WorkspaceType` and other shared types. Re-exported from `lib.rs`.
- `storage/` — Trait-based storage layer. Traits in `mod.rs` (`WorkspaceStorage`, `ApiHistoryStorage`, `UiPrefsStorage`, `AgentProfileStorage`), SQLite backend in `sqlite.rs`, legacy JSON in `json.rs`.
- `workspace/` — Git worktree CRUD (`WorkspaceManager`), `FileWatcher` (notify crate), config persistence.
- `pty/` — `PtySession` wrapping `portable-pty` with `spawn_blocking` for non-blocking reads, `vt100::Parser` for terminal state.
- `diff/` — Git diff pipeline: `git diff | delta` with plain fallback, ANSI output handling.
- `git.rs` — Low-level git helpers via `tokio::process::Command`.
- `github.rs` — `gh` CLI wrappers for PR operations and unified diff parsing.
- `paths.rs` — `DataPaths` struct centralizing all directory paths.
- `sysinfo.rs` / `preflight.rs` — Runtime environment checks.

## Conventions

- Error handling: `anyhow::Result` for application errors.
- Thread safety: `parking_lot::Mutex<Connection>` for SQLite, traits are `Send + Sync`.
- Schema changes: add migration in `sqlite.rs`, bump version constant.
- Storage traits: add new trait in `mod.rs`, implement in `sqlite.rs`, wire through `AppStorage`.
- All public types that cross crate boundaries go in `domain.rs`.
