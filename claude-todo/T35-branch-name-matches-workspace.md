# T35 — Branch name matches workspace name

**Status:** DONE
**Phase:** 12 — Naming
**Blocks:** —
**Blocked by:** —

## Description

Remove the `piki-multi/` prefix from branch names so that the git branch is named exactly the same as the workspace name.

## Changes

- `src/workspace/manager.rs`: Removed `BRANCH_PREFIX` constant and use `name.to_string()` directly for branch names in both `create()` and `remove()`.

## Acceptance Criteria

- [x] Branch name equals the workspace name (no prefix)
- [x] Worktree directory name equals the workspace name
- [x] `cargo build` compiles
- [x] `cargo test` passes
