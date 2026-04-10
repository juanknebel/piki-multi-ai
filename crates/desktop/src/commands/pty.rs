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

    // Non-PTY providers don't need a terminal session
    if ai_provider == AIProvider::Kanban || ai_provider == AIProvider::Api {
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

    // Resolve command: Shell uses $SHELL, Custom uses ProviderManager, built-in uses command()
    let (command, default_args) = if ai_provider == AIProvider::Shell {
        // Check user-configured shell from settings
        let app = state.lock();
        let custom_shell = app
            .storage
            .ui_prefs
            .as_ref()
            .and_then(|p| p.get_preference("settings").ok().flatten())
            .and_then(|json| {
                serde_json::from_str::<serde_json::Value>(&json)
                    .ok()?
                    .get("shell")?
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            });
        drop(app);
        (custom_shell.unwrap_or_else(|| ai_provider.resolved_command()), Vec::new())
    } else if let AIProvider::Custom(ref name) = ai_provider {
        let app = state.lock();
        if let Some(config) = app.provider_manager.get(name) {
            (config.command.clone(), config.default_args.clone())
        } else {
            return Err(format!("Unknown custom provider: {name}"));
        }
    } else {
        (ai_provider.resolved_command(), Vec::new())
    };
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

    // Spawn PTY session (use default_args from provider config)
    let pty = RawPtySession::spawn(
        app_handle,
        tab_id.clone(),
        &worktree_path,
        24,
        80,
        &command,
        &default_args,
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

#[tauri::command]
pub async fn spawn_editor_tab(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    file_path: String,
) -> Result<String, String> {
    // Resolve shell command (same logic as Shell provider in spawn_tab)
    let shell_command = {
        let app = state.lock();
        let custom_shell = app
            .storage
            .ui_prefs
            .as_ref()
            .and_then(|p| p.get_preference("settings").ok().flatten())
            .and_then(|json| {
                serde_json::from_str::<serde_json::Value>(&json)
                    .ok()?
                    .get("shell")?
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            });
        drop(app);
        custom_shell.unwrap_or_else(|| AIProvider::Shell.resolved_command())
    };

    // Resolve $EDITOR from login environment
    let editor = piki_core::shell_env::user_login_env()
        .get("EDITOR")
        .cloned()
        .unwrap_or_else(|| "vi".to_string());

    let mut tab = DesktopTab::new(AIProvider::Shell);
    let tab_id = tab.id.clone();

    let worktree_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let args: Vec<String> = Vec::new();
    let mut pty = RawPtySession::spawn(
        app_handle,
        tab_id.clone(),
        &worktree_path,
        24,
        80,
        &shell_command,
        &args,
    )
    .map_err(|e| format!("Failed to spawn PTY: {e}"))?;

    // Write editor command to PTY stdin
    let cmd = format!("{} {}\n", editor, shell_quote(&file_path));
    pty.write(cmd.as_bytes())
        .map_err(|e| format!("Failed to write editor command: {e}"))?;

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

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
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
        other => Ok(AIProvider::Custom(other.to_string())),
    }
}
