# T46 — File status not populated on startup

**Status**: DONE
**Blocked by**: —

## Problem

When the app launches and restores persisted workspaces, it never runs an
initial `refresh_changed_files()`. The file watcher only fires on *new*
filesystem events, so any modifications that already exist in the worktree
are not shown in the STATUS pane until something changes on disk.

## Fix

After restoring each workspace (and after starting its file watcher), call
`ws.refresh_changed_files().await` so the file list is populated immediately.

## Files to modify

- `src/main.rs` — workspace restore loop (after `spawn_all_providers`)
