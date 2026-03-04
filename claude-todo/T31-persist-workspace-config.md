# T31 — Persist workspace configuration to disk

**Status:** DONE
**Phase:** 10 — Persistence
**Blocks:** —
**Blocked by:** T30

## Description

Save workspace configuration to `~/.local/share/piki-multi/workspaces/` so the app can re-discover and restore workspaces on restart. Currently all workspace state lives only in memory (`Vec<Workspace>` in `App`) and is lost when the app exits.

## Storage location

```
~/.local/share/piki-multi/
  worktrees/<project>/<name>/   # git worktrees (already exists via T30)
  workspaces/<project>.json     # workspace config per project (NEW)
```

## Config file format (proposed)

```json
{
  "project_root": "/home/user/my-project",
  "workspaces": [
    {
      "name": "ws-1",
      "branch": "piki-multi/ws-1",
      "worktree_path": "/home/user/.local/share/piki-multi/worktrees/my-project/ws-1",
      "source_repo": "/home/user/my-project"
    }
  ]
}
```

## Behavior

- **On workspace create**: add entry to the config file
- **On workspace delete**: remove entry from the config file
- **On app startup**: read config file, restore workspaces whose worktrees still exist on disk
- **On app exit**: ensure config is up to date (should already be if we save on create/delete)
- Handle gracefully: missing config file (first run), stale entries (worktree was manually deleted), corrupted JSON

## Acceptance Criteria

- [ ] Config file is created/updated at `~/.local/share/piki-multi/workspaces/<project>.json`
- [ ] Workspaces are restored on app restart (PTY sessions re-spawned)
- [ ] Deleting a workspace removes its entry from the config
- [ ] Missing or corrupted config files are handled gracefully (no crash)
- [ ] Stale entries (worktree dir no longer exists) are cleaned up on load
- [ ] cargo test passes
- [ ] cargo clippy clean (no new warnings)
- [ ] cargo build compiles
- [ ] README.md updated with persistence info

## Files to modify

- `src/workspace/manager.rs` — add save/load config logic
- `src/main.rs` — load config on startup, save on create/delete actions
- `src/app.rs` — possibly add serde derives to Workspace or a separate config struct
- `README.md` — document persistence behavior
