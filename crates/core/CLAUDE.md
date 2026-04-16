# piki-core

UI-agnostic library crate. **Must NOT depend on `crates/tui` or `crates/api-client`.**

## Modules

- `chat.rs` — `ChatMessage`, `ChatRole` (System, User, Assistant, Tool), `ChatConfig`, `ChatServerType`, `ToolDefinition`, `ToolCall`, `ToolResult` — domain types for the global AI chat panel with tool-use support. `ChatServerType` enum (Ollama, LlamaCpp) selects which server backend to use. `ChatMessage` has optional `tool_calls` and `tool_call_id` fields for agentic tool-use flow. Used by both TUI and Desktop frontends.
- `domain.rs` — `AIProvider` (with `Custom(String)` variant), `WorkspaceInfo`, `WorkspaceType` and other shared types. Re-exported from `lib.rs`.
- `providers.rs` — `ProviderConfig`, `PromptFormat`, `ProviderManager` — user-configurable providers loaded from `providers.toml`. Manages custom provider binaries, args, prompt format, and agent directories.
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
