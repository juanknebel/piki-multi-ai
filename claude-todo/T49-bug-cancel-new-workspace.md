# Task: Add Cancel option to New Workspace dialog (DONE)

Currently, when the dialog to create a new workspace is opened, there is no way to cancel the operation. We need to add "Accept" and "Cancel" buttons/options so the user can exit without creating a workspace.

## Proposed Changes
1. Update `src/ui/workspaces.rs` to render "Accept" and "Cancel" buttons in the dialog.
2. Update `src/app.rs` or `src/ui/mod.rs` to handle the "Cancel" action (closing the dialog without calling the workspace creation logic).
3. Ensure the current navigation/interaction keys (like `Ctrl+g`) also close the dialog.

## Implementation Details
1. Updated `src/main.rs` to handle `KeyCode::Esc` in `handle_new_workspace_input`.
2. Updated `src/ui/layout.rs` to show `[Esc] Cancel` in the dialog and footer.
3. Verified that `Esc` and `Ctrl+g` both correctly close the dialog.
