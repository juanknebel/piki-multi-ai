use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use parking_lot::Mutex;
use tauri::{AppHandle, State};

use piki_core::AIProvider;
use piki_core::cli_agent::bridge_for_command;

use crate::pty_raw::RawPtySession;
use crate::state::{DesktopApp, DesktopTab};

/// The user's configured shell from the settings blob in UI prefs, if set.
fn configured_shell(app: &DesktopApp) -> Option<String> {
    app.storage
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
        })
}

/// Launch plan for a plain shell tab — shared by the editor tab and the
/// open-directory-in-terminal command, which both just want "a shell here".
fn shell_launch_plan(app: &DesktopApp) -> Result<piki_core::pty::LaunchPlan, String> {
    piki_core::pty::launch_plan(
        &AIProvider::Shell,
        None,
        Some(&app.provider_manager),
        &app.paths,
        configured_shell(app).as_deref(),
    )
    .map_err(|e| e.to_string())
}

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
        // Kanban/Api: never a Custom provider, so no idle config applies.
        let mut tab = DesktopTab::new(ai_provider, None);
        let tab_id = tab.id.clone();
        tab.alive = true;
        let mut app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].tabs.push(tab);
        app.workspaces[workspace_idx].active_tab = app.workspaces[workspace_idx].tabs.len() - 1;
        return Ok(tab_id);
    }

    // Resolve the provider's providers.toml entry (cloned, so the lock is
    // released before the spawn below) for its per-provider idle knobs.
    // Built-in providers (Shell/…) have no entry → universal defaults.
    let provider_cfg = if let AIProvider::Custom(ref name) = ai_provider {
        state.lock().provider_manager.get(name).cloned()
    } else {
        None
    };

    // Command, args, env, shell integration and the cli-agent channel all come
    // from core, so this app and the TUI make the identical decisions. This
    // used to be resolved here separately and had drifted: passive agent-state
    // detection (Codex) was missing entirely, and shell tabs never got the
    // cli-agent FIFO, so a manually-typed `claude` never showed up as an agent.
    let plan = {
        let app = state.lock();
        piki_core::pty::launch_plan(
            &ai_provider,
            None,
            Some(&app.provider_manager),
            &app.paths,
            configured_shell(&app).as_deref(),
        )
        .map_err(|e| e.to_string())?
    };

    let mut tab = DesktopTab::new(ai_provider.clone(), provider_cfg.as_ref());
    let tab_id = tab.id.clone();

    let worktree_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    // The bridge exists for this agent but couldn't be installed (its hook
    // scripts need `jq`). The tab still runs — only its status degrades to the
    // byte-silence heuristic — so this warns instead of failing the spawn.
    let bridge = match ai_provider {
        AIProvider::Custom(_) => bridge_for_command(&plan.command),
        _ => None,
    };
    if let Some(b) = bridge
        && !plan.integration_on
    {
        let missing = piki_core::cli_agent::missing_prerequisites(b).join(", ");
        crate::events::emit_toast(
            &app_handle,
            &format!(
                "{} status unavailable — install {missing} and reopen the tab",
                b.label()
            ),
            "info",
        );
    }

    let pty = RawPtySession::spawn(
        app_handle,
        tab_id.clone(),
        &worktree_path,
        24,
        80,
        &plan.command,
        &plan.args,
        &plan.env,
        &plan.extra_args,
        plan.integration_on,
        plan.cli_agent_sock,
    )
    .map_err(|e| format!("Failed to spawn PTY: {e}"))?;

    tab.pty = Some(pty);
    tab.alive = true;

    let mut app = state.lock();
    if workspace_idx < app.workspaces.len() {
        app.workspaces[workspace_idx].tabs.push(tab);
        app.workspaces[workspace_idx].active_tab = app.workspaces[workspace_idx].tabs.len() - 1;
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
                    return pty
                        .write(&bytes)
                        .map_err(|e| format!("PTY write error: {e}"));
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
                    return pty
                        .resize(rows, cols)
                        .map_err(|e| format!("PTY resize error: {e}"));
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
    // Same shell resolution as a Shell tab, through the shared planner.
    let plan = shell_launch_plan(&state.lock())?;

    // Resolve $EDITOR from login environment
    let editor = piki_core::shell_env::user_login_env()
        .get("EDITOR")
        .cloned()
        .unwrap_or_else(|| "vi".to_string());

    let mut tab = DesktopTab::new(AIProvider::Shell, None);
    let tab_id = tab.id.clone();

    let worktree_path = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[workspace_idx].info.path.clone()
    };

    let mut pty = RawPtySession::spawn(
        app_handle,
        tab_id.clone(),
        &worktree_path,
        24,
        80,
        &plan.command,
        &plan.args,
        &plan.env,
        &plan.extra_args,
        plan.integration_on,
        plan.cli_agent_sock,
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
        app.workspaces[workspace_idx].active_tab = app.workspaces[workspace_idx].tabs.len() - 1;
    }

    Ok(tab_id)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Spawns a Shell tab whose working directory is `dir` (workspace-relative).
/// Powers the file tree's "Open in Terminal" action. Rejects paths that
/// escape the workspace root.
#[tauri::command]
pub async fn spawn_terminal_at(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    dir: String,
) -> Result<String, String> {
    use std::path::{Component, Path};

    let rel = Path::new(&dir);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!("Invalid path: {dir}"));
    }

    let mut tab = DesktopTab::new(AIProvider::Shell, None);
    let tab_id = tab.id.clone();

    let (worktree_path, plan) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            app.workspaces[workspace_idx].info.path.clone(),
            shell_launch_plan(&app)?,
        )
    };

    let cwd = worktree_path.join(rel);
    let pty = RawPtySession::spawn(
        app_handle,
        tab_id.clone(),
        &cwd,
        24,
        80,
        &plan.command,
        &plan.args,
        &plan.env,
        &plan.extra_args,
        plan.integration_on,
        plan.cli_agent_sock,
    )
    .map_err(|e| format!("Failed to spawn PTY: {e}"))?;

    tab.pty = Some(pty);
    tab.alive = true;

    let mut app = state.lock();
    if workspace_idx < app.workspaces.len() {
        app.workspaces[workspace_idx].tabs.push(tab);
        app.workspaces[workspace_idx].active_tab = app.workspaces[workspace_idx].tabs.len() - 1;
    }

    Ok(tab_id)
}

#[tauri::command]
pub async fn set_active_tab(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    tab_idx: usize,
) -> Result<(), String> {
    let mut app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    let is_visible = app.active_workspace == workspace_idx;
    let ws = &mut app.workspaces[workspace_idx];
    if tab_idx >= ws.tabs.len() {
        return Err("Tab index out of range".to_string());
    }
    ws.active_tab = tab_idx;
    // Looking at a tab acknowledges its "unseen news" marker, so the agent
    // attention badge clears the same way it does in the TUI's event loop.
    if is_visible
        && let Some(shell) = ws.tabs[tab_idx].pty.as_ref().and_then(|p| p.shell())
        && let Some(agent) = shell.lock().state.cli_agent.as_mut()
    {
        agent.acknowledge();
    }
    Ok(())
}

fn parse_provider(s: &str) -> Result<AIProvider, String> {
    match s {
        "Shell" => Ok(AIProvider::Shell),
        "Kanban" => Ok(AIProvider::Kanban),
        "CodeReview" => Ok(AIProvider::CodeReview),
        "Api" => Ok(AIProvider::Api),
        other => Ok(AIProvider::Custom(other.to_string())),
    }
}
