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
            ".", "-type", "f",
            "-not", "-path", "*/.git/*",
            "-not", "-path", "*/node_modules/*",
            "-not", "-path", "*/target/*",
            "-not", "-path", "*/.next/*",
            "-not", "-path", "*/dist/*",
            "-not", "-path", "*/__pycache__/*",
            "-not", "-path", "*/.venv/*",
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

#[derive(Serialize, Clone)]
pub struct SearchMatch {
    pub path: String,
    pub line_num: u32,
    pub text: String,
}

#[tauri::command]
pub async fn project_search(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    query: String,
) -> Result<Vec<SearchMatch>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let ws_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    // Try ripgrep first (fast, respects .gitignore), fall back to grep
    let output = match tokio::process::Command::new("rg")
        .args(["--no-heading", "--line-number", "--color=never", &query])
        .current_dir(&ws_path)
        .output()
        .await
    {
        Ok(out) => out,
        Err(_) => {
            // Fallback to grep -rn
            tokio::process::Command::new("grep")
                .args(["-rn", "--color=never", "-I", &query, "."])
                .current_dir(&ws_path)
                .output()
                .await
                .map_err(|e| format!("grep failed: {e}"))?
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<SearchMatch> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .take(100)
        .filter_map(|line| {
            // Format: file:line_num:text
            let mut parts = line.splitn(3, ':');
            let raw_path = parts.next()?;
            let path = raw_path.strip_prefix("./").unwrap_or(raw_path);
            let line_num: u32 = parts.next()?.parse().ok()?;
            let text = parts.next().unwrap_or("").to_string();
            Some(SearchMatch {
                path: path.to_string(),
                line_num,
                text,
            })
        })
        .collect();

    Ok(results)
}
