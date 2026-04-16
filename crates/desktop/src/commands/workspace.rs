use parking_lot::Mutex;
use tauri::State;

use piki_core::{WorkspaceInfo, WorkspaceStatus};
use piki_core::workspace::watcher::FileWatcher;

use crate::state::{DesktopApp, DesktopWorkspace, WorkspaceDetail};

#[tauri::command]
pub async fn list_workspaces(state: State<'_, Mutex<DesktopApp>>) -> Result<Vec<WorkspaceInfo>, String> {
    let app = state.lock();
    Ok(app.workspaces.iter().map(|ws| ws.info.clone()).collect())
}

#[tauri::command]
pub async fn switch_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
) -> Result<WorkspaceDetail, String> {
    // Scope the lock: set active workspace and extract path
    let ws_path = {
        let mut app = state.lock();
        if index >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.active_workspace = index;
        app.workspaces[index].info.path.clone()
    };

    // Async git operations (no lock held)
    let files = piki_core::git::get_changed_files(&ws_path)
        .await
        .unwrap_or_default();
    let ahead_behind = piki_core::git::get_ahead_behind(&ws_path).await;

    // Re-lock to update state and return detail
    let mut app = state.lock();
    if index < app.workspaces.len() {
        app.workspaces[index].changed_files = files;
        app.workspaces[index].ahead_behind = ahead_behind;
        Ok(app.workspaces[index].to_detail())
    } else {
        Err("Workspace removed during switch".to_string())
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    name: String,
    description: String,
    prompt: String,
    dir: String,
    ws_type: String,
    group: Option<String>,
    kanban_path: Option<String>,
) -> Result<WorkspaceInfo, String> {
    // Extract manager info with scoped lock
    let (manager, storage) = {
        let app = state.lock();
        let manager = piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone());
        let storage = std::sync::Arc::clone(&app.storage);
        (manager, storage)
    };

    let source_repo = std::path::PathBuf::from(&dir);

    // Async workspace creation (no lock held)
    let info = match ws_type.as_str() {
        "Simple" => {
            manager
                .create_simple(&name, &description, &prompt, kanban_path.clone(), &source_repo)
                .await
                .map_err(|e| e.to_string())?
        }
        "Project" => {
            manager
                .create_project(&name, &description, &prompt, kanban_path.clone(), &source_repo)
                .await
                .map_err(|e| e.to_string())?
        }
        _ => {
            manager
                .create(&name, &description, &prompt, kanban_path, &source_repo)
                .await
                .map_err(|e| e.to_string())?
        }
    };

    let mut result_info = info.clone();
    result_info.group = group.clone();

    let watcher = FileWatcher::new(result_info.path.clone(), result_info.name.clone()).ok();

    // Re-lock to update state
    let mut app = state.lock();
    let order = app
        .workspaces
        .iter()
        .map(|ws| ws.info.order)
        .max()
        .unwrap_or(0)
        + 1;
    result_info.order = order;

    app.workspaces.push(DesktopWorkspace {
        info: result_info.clone(),
        status: WorkspaceStatus::Idle,
        changed_files: Vec::new(),
        ahead_behind: None,
        tabs: Vec::new(),
        active_tab: 0,
        watcher,
    });

    // Save to storage — use the new workspace's source_repo as the key
    let all_infos: Vec<WorkspaceInfo> = app.workspaces.iter().map(|ws| ws.info.clone()).collect();
    let _ = storage
        .workspaces
        .save_workspaces(&result_info.source_repo, &all_infos);

    Ok(result_info)
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
) -> Result<(), String> {
    // Scope the lock: remove workspace and extract info for cleanup
    let (ws_name, ws_source_repo, manager) = {
        let mut app = state.lock();

        if index >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }

        let ws = app.workspaces.remove(index);
        if app.active_workspace >= app.workspaces.len() && !app.workspaces.is_empty() {
            app.active_workspace = app.workspaces.len() - 1;
        }

        // Save to storage — use the removed workspace's source_repo as the key
        let all_infos: Vec<WorkspaceInfo> = app.workspaces.iter().map(|w| w.info.clone()).collect();
        let _ = app
            .storage
            .workspaces
            .save_workspaces(&ws.info.source_repo, &all_infos);

        let manager =
            piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone());
        (ws.info.name, ws.info.source_repo, manager)
    };

    // Async worktree removal (no lock held)
    let _ = manager.remove(&ws_name, &ws_source_repo).await;

    Ok(())
}

#[tauri::command]
pub async fn update_workspace(
    state: State<'_, Mutex<DesktopApp>>,
    index: usize,
    prompt: Option<String>,
    group: Option<String>,
    description: Option<String>,
    kanban_path: Option<String>,
) -> Result<(), String> {
    let mut app = state.lock();
    if index >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }

    let ws = &mut app.workspaces[index];
    if let Some(p) = prompt {
        ws.info.prompt = p;
    }
    if let Some(g) = group {
        ws.info.group = if g.is_empty() { None } else { Some(g) };
    }
    if let Some(d) = description {
        ws.info.description = d;
    }
    if let Some(k) = kanban_path {
        ws.info.kanban_path = if k.is_empty() { None } else { Some(k) };
    }

    // Persist — use the updated workspace's source_repo as the key
    let source_repo = app.workspaces[index].info.source_repo.clone();
    let all_infos: Vec<WorkspaceInfo> = app.workspaces.iter().map(|w| w.info.clone()).collect();
    let _ = app
        .storage
        .workspaces
        .save_workspaces(&source_repo, &all_infos);

    Ok(())
}
