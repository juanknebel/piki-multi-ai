# CLAUDE.md

**IMPORTANT:** Always read and follow `AGENTS.md` at the project root before starting any task.

## CI parity check

Before pushing, run the same commands GitHub Actions runs so failures are caught locally:

```bash
# ci.yml — runs on push to main/nightly and PRs to main
cargo clippy --workspace --exclude piki-desktop --all-targets -- -D warnings
cargo test --workspace --exclude piki-desktop

# nightly.yml — runs on push to nightly; builds the desktop bundle
cd crates/desktop/frontend && npm run build   # = tsc && vite build
```

Notes:
- `ci.yml` excludes `piki-desktop` entirely — its Rust code is only built by `nightly.yml` / `release.yml`.
- The frontend's TypeScript is only typechecked via `npm run build` in `nightly.yml`; `ci.yml` does not touch it.
- All three commands must be clean before pushing to `nightly` (the branch that triggers `nightly.yml`).

## Subagents

Delegate specialized work to the agents in `.claude/agents/`:

- **ratatui-expert** — UI rendering, widgets, layouts, snapshot tests
- **rust-backend-expert** — async Rust, storage, PTY, git operations
- **ui-designer** — desktop interface design with Tauri
- **ui-expert** — desktop frontend implementation with Rust backend
- **qa-expert** — testing, bug reports, quality checks
