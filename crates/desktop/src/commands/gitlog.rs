use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;
use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct GitLogEntry {
    pub sha: Option<String>,
    pub line: String,
}

#[tauri::command]
pub async fn get_git_log(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<GitLogEntry>, String> {
    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let output = piki_core::shell_env::command("git")
        .args(["log", "--oneline", "--graph", "--decorate", "--all", "-50"])
        .current_dir(&ws_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run git log: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = stdout
        .lines()
        .map(|line| {
            let trimmed = line.trim_start_matches(|c: char| "*|/\\ ".contains(c));
            let sha = trimmed
                .split_whitespace()
                .next()
                .filter(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit()))
                .map(String::from);
            GitLogEntry {
                sha,
                line: line.to_string(),
            }
        })
        .collect();

    Ok(entries)
}
