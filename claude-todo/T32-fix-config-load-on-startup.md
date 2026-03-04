# T32 — Fix configuration not loading on startup

**Status:** DONE
**Phase:** 10 — Persistence (bugfix)
**Blocks:** —
**Blocked by:** T31

## Bug Description

When the application starts, it does not restore existing workspace configurations from disk. The restore logic in `main.rs:41-87` depends on:

1. `std::env::current_dir()` resolving to the original project directory
2. `WorkspaceManager::git_root(&cwd)` succeeding (fails if not inside a git repo)
3. `config_path(git_root)` using `git_root.file_name()` matching the name used when saving via `source_repo`

This means:
- If the user runs the binary from a different directory than the original project, configs are not found
- If the user runs the binary from outside any git repo, `git_root()` fails and the entire restore block is skipped
- There could be a mismatch between `git_root` (used on load) and `source_repo` (used on save) if they resolve to different paths

## Expected Behavior

On startup the app should discover and load ALL persisted workspace configs from `~/.local/share/piki-multi/workspaces/`, regardless of the current working directory.

## Current Behavior

Only loads config for the project matching `cwd`'s git root. If cwd is not inside a git repo or doesn't match the saved project, nothing is restored.

## Root Cause

- `config::load()` requires a `git_root: &Path` argument to locate the config file
- Startup code only checks the single project matching `cwd`
- No mechanism to scan the config directory for all existing project configs

## Proposed Fix

1. Add a `config::load_all()` function that scans `~/.local/share/piki-multi/workspaces/*.json` and returns all valid workspace entries grouped by project
2. On startup, call `load_all()` instead of `load(&git_root)` to restore all persisted workspaces
3. Alternatively, if multi-project is not desired at startup, accept a `--project` CLI arg or default to scanning all configs

## Files to modify

- `src/workspace/config.rs` — add `load_all()` function
- `src/main.rs` — change startup restore logic to use `load_all()` or scan config dir

## Acceptance Criteria

- [ ] App restores saved workspaces on startup regardless of cwd
- [ ] Stale entries (worktree dir deleted) are still filtered out
- [ ] Missing or corrupted config files handled gracefully
- [ ] `cargo test` passes
- [ ] `cargo clippy` clean
- [ ] `cargo build` compiles
