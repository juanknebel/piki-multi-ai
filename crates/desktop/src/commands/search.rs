use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
}

#[tauri::command]
pub async fn fuzzy_file_list(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<String>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    // Use git ls-files for gitignore-aware file listing
    let output = tokio::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git ls-files failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();

    Ok(files)
}
