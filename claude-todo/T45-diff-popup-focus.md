# T45 — Diff popup doesn't get focus on open

**Status**: DONE
**Blocked by**: —

## Problem

When opening a diff via Enter in the file list, `AppMode::Diff` is set but:
1. `active_pane` stays on `FileList` instead of switching to `MainPanel`
2. `interacting` is not set to `true`

This means `handle_diff_interaction()` is never reached because it requires
`active_pane == MainPanel && interacting == true && mode == Diff`. Keys go to
the file list handler instead, so the diff can't be scrolled or closed with Esc.

## Fix

In `Action::OpenDiff`, after setting `app.mode = AppMode::Diff`, also set
`app.active_pane = ActivePane::MainPanel` and `app.interacting = true`.

## Files to modify

- `src/main.rs` — `Action::OpenDiff` handler
