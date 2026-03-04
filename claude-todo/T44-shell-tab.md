# T44 — Add shell tab alongside AI provider tabs

**Status**: DONE
**Blocked by**: —

## Problem

Users need a plain terminal/shell in the workspace path for running commands (build, test, git, etc.) without leaving the app. Currently the sub-tabs only show AI providers (Claude, Codex).

## Solution

Add a new tab in the provider sub-tabs area that spawns the user's default shell (`$SHELL` or fallback to `/bin/sh`) in the workspace's worktree path. It should behave like the existing PTY sessions but without launching an AI assistant.

### Details

- Add a "Shell" tab as the last sub-tab for each workspace
- Spawn the default shell (`$SHELL` env var, fallback `/bin/sh`) via `PtySession`
- Working directory set to the workspace's worktree path
- Full PTY interaction: keyboard input forwarding, terminal rendering, same as AI tabs
- Shell tab should persist across tab switches (don't kill/respawn on switch)
- Tab label: "Shell" or terminal icon

### Considerations

- Reuse existing `PtySession` infrastructure — the shell is just a different command
- The provider enum/model may need a new variant or the tab system needs to support non-provider tabs
- Shell tab doesn't need file watching or diff integration

## Files to modify

- `src/app.rs` — add shell tab variant to tab/provider model, store shell PTY session per workspace
- `src/main.rs` — spawn shell session, handle input forwarding for shell tab
- `src/ui/tabs.rs` — render "Shell" tab in sub-tab bar
- `src/ui/terminal.rs` — reuse for shell rendering (should work as-is)
- `src/pty/session.rs` — may need minor changes if command spawning assumes AI provider
