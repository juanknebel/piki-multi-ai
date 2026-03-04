# Task: Change navigation mode key to Ctrl+g (DONE)

The current `Esc` key to exit interaction mode (enter navigation mode) is conflicting with terminal applications (like vim). `Ctrl+Esc` was tried but many terminals don't send it, so `Ctrl+g` was chosen as it doesn't conflict with the user's Neovim/LazyVim setup.

## Proposed Changes
1. Change `Esc` to `Ctrl+g` in `handle_terminal_interaction` to exit interaction mode.
2. Ensure `Esc` (without Ctrl) is forwarded to the PTY in `handle_terminal_interaction`.
3. Update other interaction handlers if necessary (WorkspaceList, FileList, Diff).
4. Update help text or status bar.

## Implementation Plan
1. Update `src/main.rs`:
   - `handle_terminal_interaction`: Change `KeyCode::Esc` to `KeyCode::Char('g')` with `KeyModifiers::CONTROL`.
   - `handle_diff_interaction`: Same change.
   - `handle_workspace_interaction`: Same change.
   - `handle_filelist_interaction`: Same change.
2. Update `src/ui/layout.rs`:
   - Update help text and footer to show `Ctrl+g` (or `C-g`).
3. Update `README.md`.
