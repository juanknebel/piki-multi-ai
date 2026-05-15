# CLAUDE.md

**IMPORTANT:** Always read and follow `AGENTS.md` at the project root before starting any task.

## Branch flow

`main` only receives merges from `nightly` when cutting a release (via `scripts/release.sh <version>` or the equivalent manual flow). Intermediate work — bugfixes, refactors, dependency bumps, CI changes — stays on `nightly` and rides along with the next release merge.

- Push fixes to `origin/nightly` only.
- Do NOT `git merge nightly` into `main` mid-cycle.
- The change reaches `main` when `release.sh` runs the `--no-ff` merge as part of the release flow.
- The `Clean Main` ruleset on `main` blocks branch deletion and non-fast-forward pushes; merge commits from the release flow are allowed.

## CI parity check

Before pushing, run the same commands GitHub Actions runs so failures are caught locally:

```bash
# nightly.yml::test — runs on push to nightly (matrix: ubuntu + macos)
cargo clippy --workspace --exclude piki-desktop --all-targets -- -D warnings
cargo test --workspace --exclude piki-desktop

# nightly.yml::build-desktop — runs on push to nightly; builds the desktop bundle
cd crates/desktop/frontend && npm run build   # = tsc && vite build
```

Notes:
- The `test` job excludes `piki-desktop` — its Rust code is only built by `nightly.yml::build-desktop` / `release.yml`.
- The frontend's TypeScript is only typechecked via `npm run build` in `nightly.yml::build-desktop`; the `test` job does not touch it.
- All three commands must be clean before pushing to `nightly` (the only branch that triggers `nightly.yml`).
- The `build` and `build-desktop` jobs have `needs: test`, so a failing test blocks the nightly artifacts from publishing.

## Subagents

Delegate specialized work to the agents in `.claude/agents/`:

- **ratatui-expert** — UI rendering, widgets, layouts, snapshot tests
- **rust-backend-expert** — async Rust, storage, PTY, git operations
- **ui-designer** — desktop interface design with Tauri
- **ui-expert** — desktop frontend implementation with Rust backend
- **qa-expert** — testing, bug reports, quality checks
