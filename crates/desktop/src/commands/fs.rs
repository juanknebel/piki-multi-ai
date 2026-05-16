use std::path::{Component, Path, PathBuf};

use parking_lot::Mutex;
use tauri::State;

use piki_core::DirEntry;

use crate::state::DesktopApp;

/// Resolves a workspace-relative path against `root`, rejecting absolute
/// paths and any `..` components so a mutation can't escape the workspace.
/// An empty string resolves to the root itself.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!("Invalid path: {rel}"));
    }
    Ok(root.join(rel_path))
}

fn workspace_path(
    state: &State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<PathBuf, String> {
    let app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    Ok(app.workspaces[workspace_idx].info.path.clone())
}

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
    let ws_path = workspace_path(&state, workspace_idx)?;
    let full_path = ws_path.join(&path);
    Ok(piki_core::workspace::manager::read_dir_entries(&full_path, show_hidden).await)
}

/// Creates an empty file at `path`. Fails if it already exists.
#[tauri::command]
pub async fn fs_create_file(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    path: String,
) -> Result<(), String> {
    let ws_path = workspace_path(&state, workspace_idx)?;
    let full = safe_join(&ws_path, &path)?;
    tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to create file: {e}"))
}

/// Creates a directory at `path`. Fails if it already exists.
#[tauri::command]
pub async fn fs_create_dir(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    path: String,
) -> Result<(), String> {
    let ws_path = workspace_path(&state, workspace_idx)?;
    let full = safe_join(&ws_path, &path)?;
    tokio::fs::create_dir(&full)
        .await
        .map_err(|e| format!("Failed to create directory: {e}"))
}

/// Renames / moves `from` to `to` (both workspace-relative).
#[tauri::command]
pub async fn fs_rename(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    from: String,
    to: String,
) -> Result<(), String> {
    let ws_path = workspace_path(&state, workspace_idx)?;
    let from_full = safe_join(&ws_path, &from)?;
    let to_full = safe_join(&ws_path, &to)?;
    if tokio::fs::try_exists(&to_full).await.unwrap_or(false) {
        return Err(format!("Already exists: {to}"));
    }
    tokio::fs::rename(&from_full, &to_full)
        .await
        .map_err(|e| format!("Failed to rename: {e}"))
}

/// Deletes `path` — recursively if it's a directory.
#[tauri::command]
pub async fn fs_delete(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    path: String,
) -> Result<(), String> {
    let ws_path = workspace_path(&state, workspace_idx)?;
    let full = safe_join(&ws_path, &path)?;
    let meta = tokio::fs::symlink_metadata(&full)
        .await
        .map_err(|e| format!("Failed to stat: {e}"))?;
    let res = if meta.is_dir() {
        tokio::fs::remove_dir_all(&full).await
    } else {
        tokio::fs::remove_file(&full).await
    };
    res.map_err(|e| format!("Failed to delete: {e}"))
}
