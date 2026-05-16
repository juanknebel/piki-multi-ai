use parking_lot::Mutex;
use tauri::State;

use piki_core::DirEntry;

use crate::state::DesktopApp;

/// Lists the immediate children of `path` (relative to the workspace root).
///
/// Lazy / non-recursive — the frontend file tree calls this once per expanded
/// directory. Pass an empty string to list the workspace root itself.
#[tauri::command]
pub async fn fs_read_dir(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    path: String,
    show_hidden: bool,
) -> Result<Vec<DirEntry>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let full_path = ws_path.join(&path);
    Ok(piki_core::workspace::manager::read_dir_entries(&full_path, show_hidden).await)
}
