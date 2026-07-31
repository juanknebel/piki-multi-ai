# CLAUDE.md

**IMPORTANT:** Always read and follow `AGENTS.md` at the project root before starting any task.

## Branch flow

`main` only receives merges from `nightly` when cutting a release (via `scripts/release.sh <version>` or the equivalent manual flow). Intermediate work — bugfixes, refactors, dependency bumps, CI changes — stays on `nightly` and rides along with the next release merge.

- Push fixes to `origin/nightly` only.
- Do NOT `git merge nightly` into `main` mid-cycle.
- The change reaches `main` when `release.sh` runs the `--no-ff` merge as part of the release flow.
- The `Clean Main` ruleset on `main` blocks branch deletion and non-fast-forward pushes; merge commits from the release flow are allowed.

## CI parity check

Before pushing, run what GitHub Actions runs:

```bash
just ci
```

That is the single source of truth for the parity list — the recipes live in the `justfile` at the repo root (`cargo install just`). It expands to:

```bash
# nightly.yml::test — push to nightly + every PR (matrix: ubuntu + macos)
cargo fmt --all -- --check                    # ubuntu leg only in CI
cargo clippy --workspace --exclude piki-desktop --all-targets -- -D warnings
cargo test --workspace --exclude piki-desktop

# nightly.yml::build-desktop — push to nightly only; builds the desktop bundle
cd crates/desktop/frontend && npm run build   # = tsc && vite build
```

Notes:
- The `test` job excludes `piki-desktop` because `tauri-build` needs `frontend/dist` to exist. Its Rust *is* linted and tested, in `build-desktop`, after the frontend build (`just lint-desktop` locally).
- The frontend's TypeScript is only typechecked via `npm run build` in `nightly.yml::build-desktop`; the `test` job does not touch it.
- Everything must be clean before pushing to `nightly`. PRs run the `test` job too; `build`/`build-desktop`/`release` are gated on `github.event_name == 'push'` so a PR never publishes artifacts.
- The `build` and `build-desktop` jobs have `needs: test`, so a failing test blocks the nightly artifacts from publishing.
- `.github/workflows/audit.yml` runs `cargo audit` weekly and on any dependency change. Advisories we cannot act on are ignored in `.cargo/audit.toml`, each with a reason and the condition that clears it — add entries there, never by silencing the job.
- The `cargo fmt` baseline commit is listed in `.git-blame-ignore-revs`; run `git config blame.ignoreRevsFile .git-blame-ignore-revs` once so local blame skips it.

## Subagents

Delegate specialized work to the agents in `.claude/agents/`:

- **ratatui-expert** — UI rendering, widgets, layouts, snapshot tests
- **rust-backend-expert** — async Rust, storage, PTY, git operations
- **ui-designer** — desktop interface design with Tauri
- **ui-expert** — desktop frontend implementation with Rust backend
- **qa-expert** — testing, bug reports, quality checks
