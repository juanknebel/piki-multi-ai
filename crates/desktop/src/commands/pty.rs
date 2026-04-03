use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use parking_lot::Mutex;
use tauri::{AppHandle, State};

use piki_core::AIProvider;

use crate::pty_raw::RawPtySession;
use crate::state::{DesktopApp, DesktopTab};

#[tauri::command]
pub async fn spawn_tab(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    provider: String,
) -> Result<String, String> {
    let ai_provider = parse_provider(&provider)?;

    // Kanban tabs don't need a PTY session
    if ai_provider == AIProvider::Kanban {
        let mut tab = DesktopTab::new(ai_provider);
        let tab_id = tab.id.clone();
        tab.alive = true;
        let mut app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].tabs.push(tab);
        app.workspaces[workspace_idx].active_tab =
            app.workspaces[workspace_idx].tabs.len() - 1;
        return Ok(tab_id);
    }

    // Non-PTY providers (CodeReview, Api) are handled in Phase 2+
    let command = ai_provider.resolved_command();
    if command.is_empty() {
        return Err(format!("{provider} does not use a terminal session"));
    }

    let mut tab = DesktopTab::new(ai_provider);
    let tab_id = tab.id.clone();

    let worktree_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    // Spawn PTY session
    let args: Vec<String> = Vec::new();
    let pty = RawPtySession::spawn(
        app_handle,
        tab_id.clone(),
        &worktree_path,
        24,
        80,
        &command,
        &args,
    )
    .map_err(|e| format!("Failed to spawn PTY: {e}"))?;

    tab.pty = Some(pty);
    tab.alive = true;

    let mut app = state.lock();
    if workspace_idx < app.workspaces.len() {
        app.workspaces[workspace_idx].tabs.push(tab);
        app.workspaces[workspace_idx].active_tab =
            app.workspaces[workspace_idx].tabs.len() - 1;
    }

    Ok(tab_id)
}

#[tauri::command]
pub async fn write_pty(
    state: State<'_, Mutex<DesktopApp>>,
    tab_id: String,
    data: String,
) -> Result<(), String> {
    let bytes = BASE64
        .decode(&data)
        .map_err(|e| format!("Invalid base64: {e}"))?;

    let mut app = state.lock();
    for ws in &mut app.workspaces {
        for tab in &mut ws.tabs {
            if tab.id == tab_id {
                if let Some(ref mut pty) = tab.pty {
                    return pty.write(&bytes).map_err(|e| format!("PTY write error: {e}"));
                }
                return Err("Tab has no PTY session".to_string());
            }
        }
    }
    Err("Tab not found".to_string())
}

#[tauri::command]
pub async fn resize_pty(
    state: State<'_, Mutex<DesktopApp>>,
    tab_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let app = state.lock();
    for ws in &app.workspaces {
        for tab in &ws.tabs {
            if tab.id == tab_id {
                if let Some(ref pty) = tab.pty {
                    return pty.resize(rows, cols).map_err(|e| format!("PTY resize error: {e}"));
                }
                return Err("Tab has no PTY session".to_string());
            }
        }
    }
    Err("Tab not found".to_string())
}

#[tauri::command]
pub async fn close_tab(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    tab_idx: usize,
) -> Result<(), String> {
    let mut app = state.lock();

    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }

    let ws = &mut app.workspaces[workspace_idx];
    if tab_idx >= ws.tabs.len() {
        return Err("Tab index out of range".to_string());
    }

    // Kill PTY if present (Drop will handle cleanup)
    let mut tab = ws.tabs.remove(tab_idx);
    if let Some(ref mut pty) = tab.pty {
        let _ = pty.kill();
    }

    if ws.active_tab >= ws.tabs.len() && !ws.tabs.is_empty() {
        ws.active_tab = ws.tabs.len() - 1;
    }

    Ok(())
}

fn parse_provider(s: &str) -> Result<AIProvider, String> {
    match s {
        "Claude" => Ok(AIProvider::Claude),
        "Gemini" => Ok(AIProvider::Gemini),
        "OpenCode" => Ok(AIProvider::OpenCode),
        "Kilo" => Ok(AIProvider::Kilo),
        "Codex" => Ok(AIProvider::Codex),
        "Shell" => Ok(AIProvider::Shell),
        "Kanban" => Ok(AIProvider::Kanban),
        "CodeReview" => Ok(AIProvider::CodeReview),
        "Api" => Ok(AIProvider::Api),
        _ => Err(format!("Unknown provider: {s}")),
    }
}
