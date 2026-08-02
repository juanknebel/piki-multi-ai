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

    // Walk the workspace directory, skipping common non-project dirs
    let output = tokio::process::Command::new("find")
        .args([
            ".",
            "-type",
            "f",
            "-not",
            "-path",
            "*/.git/*",
            "-not",
            "-path",
            "*/node_modules/*",
            "-not",
            "-path",
            "*/target/*",
            "-not",
            "-path",
            "*/.next/*",
            "-not",
            "-path",
            "*/dist/*",
            "-not",
            "-path",
            "*/__pycache__/*",
            "-not",
            "-path",
            "*/.venv/*",
        ])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("find failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.strip_prefix("./").unwrap_or(l))
        .map(String::from)
        .collect();

    Ok(files)
}

#[tauri::command]
pub async fn read_file_content(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    path: String,
) -> Result<String, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let full_path = ws_path.join(&path);
    tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))
}

#[tauri::command]
pub async fn write_file_content(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    path: String,
    content: String,
) -> Result<(), String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let full_path = ws_path.join(&path);
    tokio::fs::write(&full_path, content.as_bytes())
        .await
        .map_err(|e| format!("Failed to write file: {e}"))
}

// Content search lives in `piki_core::search` (rg with grep fallback),
// shared with the TUI's project-search overlay.
#[tauri::command]
pub async fn project_search(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    query: String,
) -> Result<Vec<piki_core::search::SearchMatch>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    piki_core::search::project_search(&ws_path, &query, 100)
        .await
        .map_err(|e| e.to_string())
}
