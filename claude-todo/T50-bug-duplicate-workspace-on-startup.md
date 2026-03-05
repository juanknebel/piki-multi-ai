# Task: Bug: Duplicate workspace on startup (DONE)

The app is currently duplicating a workspace when it starts up. This could be due to double initialization or incorrect loading of persisted configuration.

## Proposed Changes
1. Investigate `src/workspace/manager.rs` and `src/app.rs` to identify where workspaces are loaded during startup.
2. Check `src/workspace/config.rs` to see if the configuration file is being read/written twice.
3. Ensure that the loading logic correctly handles existing workspaces and doesn't add them again to the in-memory list.

## Implementation Details
1. Identified that `save` in `src/workspace/config.rs` was saving all in-memory workspaces to every project config file.
2. Updated `save` to only include workspaces that belong to the `git_root` being saved.
3. Updated `load_all` to de-duplicate workspaces by their `worktree_path` to handle any existing corrupted config files.
4. Verified that workspaces are no longer duplicated on startup.
