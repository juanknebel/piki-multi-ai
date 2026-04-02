use parking_lot::Mutex;
use tauri::State;

use crate::state::DesktopApp;

#[tauri::command]
pub async fn read_markdown_file(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
) -> Result<String, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let full_path = ws_path.join(&file_path);
    tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", full_path.display()))
}
