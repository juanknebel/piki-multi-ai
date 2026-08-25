use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use piki_core::search::{FILE_INDEX_CAP, FileIndex};

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
}

/// Workspace-relative file list for the `Ctrl+F` finder (and the file tree's
/// filter). Served from `DesktopWorkspace::file_index` when the watcher has
/// not invalidated it; otherwise `piki_core::search::list_files` walks the
/// tree off-lock (gitignore-aware, capped at `FILE_INDEX_CAP`) and the
/// result is memoised for the next call.
#[tauri::command]
pub async fn fuzzy_file_list(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<FileIndex, String> {
    let (ws_path, cached) = {
        let app = state.lock();
        let Some(ws) = app.workspaces.get(workspace_idx) else {
            return Err("Workspace index out of range".to_string());
        };
        (ws.info.path.clone(), ws.file_index.clone())
    };
    if let Some(index) = cached {
        return Ok((*index).clone());
    }

    let root = ws_path.clone();
    let index =
        tokio::task::spawn_blocking(move || piki_core::search::list_files(&root, FILE_INDEX_CAP))
            .await
            .map_err(|e| format!("file walk failed: {e}"))?;

    {
        let mut app = state.lock();
        // The workspace list may have shifted while we walked; only memoise
        // onto the workspace whose tree we actually indexed.
        if let Some(ws) = app.workspaces.get_mut(workspace_idx)
            && ws.info.path == ws_path
        {
            ws.file_index = Some(Arc::new(index.clone()));
        }
    }
    Ok(index)
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
