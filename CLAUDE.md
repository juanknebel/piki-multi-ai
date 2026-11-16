# CLAUDE.md

**IMPORTANT:** Always read and follow `AGENTS.md` at the project root before starting any task.
Per-crate rules live in each crate's `AGENTS.md` (`crates/*/AGENTS.md`).

Everything below is Claude-specific. Generic workflow, build commands, branch flow,
CI parity, architecture and conventions live in `AGENTS.md` — do not duplicate them here.

## Subagents

Delegate specialized work to the agents in `.claude/agents/`:

- **ratatui-expert** — UI rendering, widgets, layouts, snapshot tests
- **rust-backend-expert** — async Rust, storage, PTY, git operations
- **ui-designer** — desktop interface design with Tauri
- **ui-expert** — desktop frontend implementation with Rust backend
- **qa-expert** — testing, bug reports, quality checks
