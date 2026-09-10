use parking_lot::Mutex;
use tauri::State;

use piki_core::projects::Project;

use crate::state::DesktopApp;

#[tauri::command]
pub fn list_projects(state: State<'_, Mutex<DesktopApp>>) -> Vec<Project> {
    let app = state.lock();
    app.storage
        .projects
        .as_ref()
        .map(|store| store.list_projects())
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_project(
    state: State<'_, Mutex<DesktopApp>>,
    mut project: Project,
) -> Result<i64, String> {
    let app = state.lock();
    let store = app
        .storage
        .projects
        .as_ref()
        .ok_or("project storage unavailable")?;
    if project.id.is_none() {
        // Same convention as workspaces: new entries land at the bottom.
        let max_order = store
            .list_projects()
            .iter()
            .map(|p| p.order)
            .max()
            .unwrap_or(0);
        project.order = max_order + 1;
    }
    store.save_project(&project).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_project(state: State<'_, Mutex<DesktopApp>>, id: i64) -> Result<(), String> {
    let app = state.lock();
    let store = app
        .storage
        .projects
        .as_ref()
        .ok_or("project storage unavailable")?;
    store.delete_project(id).map_err(|e| e.to_string())
}
