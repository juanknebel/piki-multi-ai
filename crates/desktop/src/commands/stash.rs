use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;
use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct StashEntry {
    pub index: usize,
    pub id: String,
    pub message: String,
}

#[tauri::command]
pub async fn git_stash_list(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<StashEntry>, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["stash", "list"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git stash list failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = stdout
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let id = line.split(':').next()?.to_string();
            let message = line
                .splitn(3, ':')
                .nth(2)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            Some(StashEntry {
                index: i,
                id,
                message,
            })
        })
        .collect();

    Ok(entries)
}

#[tauri::command]
pub async fn git_stash_save(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    message: String,
) -> Result<String, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;

    let output = piki_core::shell_env::command("git")
        .args(["stash", "push", "-m", &message])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git stash push failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Stash failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub async fn git_stash_pop(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    stash_index: usize,
) -> Result<String, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;
    let stash_ref = format!("stash@{{{stash_index}}}");

    let output = piki_core::shell_env::command("git")
        .args(["stash", "pop", &stash_ref])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git stash pop failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Stash pop failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub async fn git_stash_apply(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    stash_index: usize,
) -> Result<String, String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;
    let stash_ref = format!("stash@{{{stash_index}}}");

    let output = piki_core::shell_env::command("git")
        .args(["stash", "apply", &stash_ref])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git stash apply failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Stash apply failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub async fn git_stash_drop(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    stash_index: usize,
) -> Result<(), String> {
    let ws_path = get_ws_path(&state, workspace_idx)?;
    let stash_ref = format!("stash@{{{stash_index}}}");

    let output = piki_core::shell_env::command("git")
        .args(["stash", "drop", &stash_ref])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("git stash drop failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Stash drop failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

fn get_ws_path(
    state: &State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<std::path::PathBuf, String> {
    let app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    Ok(app.workspaces[workspace_idx].info.path.clone())
}
