use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::State;

use piki_core::cli_agent::install as cli_agent_install;
use piki_core::cli_agent::install_antigravity as agy_install;
use piki_core::cli_agent::{AgentBridge, bridge_for_command};
use piki_core::storage::AgentProfile;

use crate::state::DesktopApp;

// ── Types ──────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct AgentInfo {
    pub id: Option<i64>,
    pub name: String,
    pub provider: String,
    pub role: String,
    pub version: u32,
    pub last_synced_at: Option<String>,
}

impl From<AgentProfile> for AgentInfo {
    fn from(p: AgentProfile) -> Self {
        Self {
            id: p.id,
            name: p.name,
            provider: p.provider,
            role: p.role,
            version: p.version,
            last_synced_at: p.last_synced_at,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ScannedAgent {
    pub name: String,
    pub provider: String,
    pub role: String,
    pub exists: bool,
}

// ── Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn list_agents(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<AgentInfo>, String> {
    let (storage, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            std::sync::Arc::clone(&app.storage),
            app.workspaces[workspace_idx].info.source_repo.clone(),
        )
    };

    match &storage.agent_profiles {
        Some(s) => {
            let agents = s.load_agents(&source_repo).map_err(|e| e.to_string())?;
            Ok(agents.into_iter().map(AgentInfo::from).collect())
        }
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn save_agent(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    name: String,
    provider: String,
    role: String,
    id: Option<i64>,
) -> Result<(), String> {
    let (storage, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            std::sync::Arc::clone(&app.storage),
            app.workspaces[workspace_idx]
                .info
                .source_repo
                .to_string_lossy()
                .to_string(),
        )
    };

    let profile = AgentProfile {
        id,
        source_repo,
        name,
        provider,
        role,
        version: 1,
        last_synced_at: None,
    };

    match &storage.agent_profiles {
        Some(s) => s.save_agent(&profile).map_err(|e| e.to_string()),
        None => Err("Agent storage not available".to_string()),
    }
}

#[tauri::command]
pub async fn delete_agent(
    state: State<'_, Mutex<DesktopApp>>,
    agent_id: i64,
) -> Result<(), String> {
    let storage = {
        let app = state.lock();
        std::sync::Arc::clone(&app.storage)
    };

    match &storage.agent_profiles {
        Some(s) => s.delete_agent(agent_id).map_err(|e| e.to_string()),
        None => Err("Agent storage not available".to_string()),
    }
}

#[tauri::command]
pub async fn sync_agent_to_repo(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    agent_id: i64,
) -> Result<(), String> {
    let (storage, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            std::sync::Arc::clone(&app.storage),
            app.workspaces[workspace_idx].info.source_repo.clone(),
        )
    };

    let s = storage
        .agent_profiles
        .as_ref()
        .ok_or("Agent storage not available")?;

    let agents = s.load_agents(&source_repo).map_err(|e| e.to_string())?;
    let agent = agents
        .iter()
        .find(|a| a.id == Some(agent_id))
        .ok_or("Agent not found")?;

    let dir = {
        let app = state.lock();
        app.provider_manager
            .get(&agent.provider)
            .and_then(|c| c.agent_dir.clone())
            .ok_or_else(|| format!("Provider '{}' has no agent_dir configured", agent.provider))?
    };

    let agent_dir = source_repo.join(dir);
    std::fs::create_dir_all(&agent_dir).map_err(|e| format!("Failed to create directory: {e}"))?;
    std::fs::write(agent_dir.join(format!("{}.md", agent.name)), &agent.role)
        .map_err(|e| format!("Failed to write agent file: {e}"))?;

    s.mark_synced(agent_id).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn scan_repo_agents(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<Vec<ScannedAgent>, String> {
    let (storage, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            std::sync::Arc::clone(&app.storage),
            app.workspaces[workspace_idx].info.source_repo.clone(),
        )
    };

    let existing = storage
        .agent_profiles
        .as_ref()
        .and_then(|s| s.load_agents(&source_repo).ok())
        .unwrap_or_default();

    // Which directories to scan, what provider to attribute a file to, and
    // whether it is already imported are all decided in core, so this app and
    // the TUI can't disagree about them again (see core::agent_scan). This
    // used to hardcode five provider directories with labels that need not
    // match any configured provider, and compared "already imported" by name
    // alone — so a same-named agent under another provider was silently
    // skipped.
    let discovered: Vec<ScannedAgent> = {
        let app = state.lock();
        piki_core::agent_scan::scan_repo_agents(&source_repo, &app.provider_manager, &existing)
            .into_iter()
            .map(|a| ScannedAgent {
                name: a.name,
                provider: a.provider,
                role: a.role,
                exists: a.exists,
            })
            .collect()
    };

    Ok(discovered)
}

#[derive(Deserialize)]
pub struct ImportAgentEntry {
    pub name: String,
    pub provider: String,
    pub role: String,
}

#[tauri::command]
pub async fn import_agents(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    agents: Vec<ImportAgentEntry>,
) -> Result<usize, String> {
    let (storage, source_repo) = {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            std::sync::Arc::clone(&app.storage),
            app.workspaces[workspace_idx]
                .info
                .source_repo
                .to_string_lossy()
                .to_string(),
        )
    };

    let s = storage
        .agent_profiles
        .as_ref()
        .ok_or("Agent storage not available")?;

    let mut imported = 0;
    for agent in &agents {
        let profile = AgentProfile {
            id: None,
            source_repo: source_repo.clone(),
            name: agent.name.clone(),
            provider: agent.provider.clone(),
            role: agent.role.clone(),
            version: 1,
            last_synced_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        if s.save_agent(&profile).is_ok() {
            imported += 1;
        }
    }

    Ok(imported)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn dispatch_agent(
    state: State<'_, Mutex<DesktopApp>>,
    app_handle: tauri::AppHandle,
    workspace_idx: usize,
    provider: String,
    prompt: String,
    create_worktree: bool,
    ws_name: Option<String>,
    dispatch_card_id: Option<String>,
    dispatch_source_kanban: Option<String>,
    dispatch_agent_name: Option<String>,
    dispatch_card_title: Option<String>,
) -> Result<String, String> {
    // All dispatchable providers live in ProviderManager (providers.toml).
    let (ai_provider, command, default_args, prompt_format, provider_cfg) = {
        let app = state.lock();
        let config = app
            .provider_manager
            .get(&provider)
            .ok_or_else(|| format!("Unknown provider: {provider}"))?;
        (
            piki_core::AIProvider::Custom(provider.clone()),
            config.command.clone(),
            config.default_args.clone(),
            config.prompt_format.clone(),
            // Cloned so the lock drops here; drives the tab's IdleWatcher.
            config.clone(),
        )
    };
    if command.is_empty() {
        return Err(format!("{provider} does not use a terminal session"));
    }

    // If creating a new worktree workspace
    let target_ws_idx = if create_worktree {
        let (manager, source_dir, source_kanban_path) = {
            let app = state.lock();
            if workspace_idx >= app.workspaces.len() {
                return Err("Workspace index out of range".to_string());
            }
            let m = piki_core::workspace::manager::WorkspaceManager::with_paths(app.paths.clone());
            let dir = app.workspaces[workspace_idx].info.source_repo.clone();
            let kanban = app.workspaces[workspace_idx].info.kanban_path.clone();
            (m, dir, kanban)
        };

        let description = dispatch_card_title.as_deref().unwrap_or("Agent dispatch");

        let name = ws_name.unwrap_or_else(|| {
            format!(
                "agent-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("x")
            )
        });
        let info = manager
            .create(&name, description, &prompt, source_kanban_path, &source_dir)
            .await
            .map_err(|e| format!("Failed to create workspace: {e}"))?;

        let watcher =
            piki_core::workspace::watcher::FileWatcher::new(info.path.clone(), info.name.clone())
                .ok();

        let mut app = state.lock();
        let order = app
            .workspaces
            .iter()
            .map(|ws| ws.info.order)
            .max()
            .unwrap_or(0)
            + 1;
        let mut ws_info = info;
        ws_info.order = order;
        ws_info.dispatch_card_id = dispatch_card_id;
        ws_info.dispatch_source_kanban = dispatch_source_kanban;
        ws_info.dispatch_agent_name = dispatch_agent_name;

        let ws_source_repo = ws_info.source_repo.clone();
        app.workspaces.push(crate::state::DesktopWorkspace {
            info: ws_info,
            status: piki_core::WorkspaceStatus::Idle,
            changed_files: Vec::new(),
            ahead_behind: None,
            branch: None,
            tabs: Vec::new(),
            active_tab: 0,
            watcher,
            file_index: None,
        });

        let idx = app.workspaces.len() - 1;
        app.active_workspace = idx;

        // Persist to storage
        let all_infos: Vec<piki_core::WorkspaceInfo> =
            app.workspaces.iter().map(|w| w.info.clone()).collect();
        let _ = app
            .storage
            .workspaces
            .save_workspaces(&ws_source_repo, &all_infos);

        idx
    } else {
        workspace_idx
    };

    // Spawn the AI tab with prompt
    let (worktree_path, claude_hooks_dir, agy_hooks_dir) = {
        let app = state.lock();
        if target_ws_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        (
            app.workspaces[target_ws_idx].info.path.clone(),
            app.paths.claude_hooks_dir(),
            app.paths.antigravity_hooks_dir(),
        )
    };

    // Build args: default_args + prompt args
    let prompt_args = if prompt.is_empty() {
        Vec::new()
    } else {
        match &prompt_format {
            piki_core::providers::PromptFormat::Positional => vec![prompt.clone()],
            piki_core::providers::PromptFormat::Flag(flag) => vec![flag.clone(), prompt.clone()],
            piki_core::providers::PromptFormat::None => Vec::new(),
        }
    };
    let mut args = default_args;
    args.extend(prompt_args);

    let mut tab = crate::state::DesktopTab::new(ai_provider, Some(&provider_cfg));
    let tab_id = tab.id.clone();

    // Dispatched agents with a hook bridge (Claude Code, Antigravity) get the
    // structured cli-agent channel so the kanban flow sees precise lifecycle
    // status. Other providers run bare (no shell wrapper, no hooks).
    let (extra_env, extra_args, integration_on, cli_agent_sock) = match bridge_for_command(&command)
    {
        Some(AgentBridge::Claude) => match cli_agent_install::setup_for_claude(&claude_hooks_dir) {
            Ok(setup) => {
                let sock = setup.sock_path.clone();
                (
                    setup.env.into_iter().collect::<Vec<_>>(),
                    setup.extra_args,
                    true,
                    sock,
                )
            }
            Err(e) => {
                tracing::warn!(error = %e, "claude cli-agent hook setup failed");
                (Vec::new(), Vec::new(), false, None)
            }
        },
        Some(AgentBridge::Antigravity) => {
            // agy takes no hook args — the plugin lives in its own root.
            match agy_install::setup_for_antigravity(&agy_hooks_dir, &agy_install::plugins_root()) {
                Ok(setup) => {
                    let sock = setup.sock_path.clone();
                    (
                        setup.env.into_iter().collect::<Vec<_>>(),
                        Vec::new(),
                        true,
                        sock,
                    )
                }
                Err(e) => {
                    tracing::warn!(error = %e, "antigravity cli-agent hook setup failed");
                    (Vec::new(), Vec::new(), false, None)
                }
            }
        }
        None => (Vec::new(), Vec::new(), false, None),
    };

    // Prefer the session daemon so a dispatched agent survives an app restart;
    // fall back to an in-process PTY.
    let (daemon, order) = {
        let app = state.lock();
        (
            app.session_daemon.clone(),
            app.workspaces
                .get(target_ws_idx)
                .map(|w| w.tabs.len() as u32)
                .unwrap_or(0),
        )
    };
    let remote = daemon.as_ref().and_then(|d| {
        crate::session::spawn_remote_tab(
            &app_handle,
            d,
            &tab_id,
            &tab.provider,
            &command,
            &args,
            &extra_env,
            &extra_args,
            &worktree_path,
            integration_on,
            cli_agent_sock.clone(),
            order,
        )
    });
    let pty = match remote {
        Some(p) => p,
        None => crate::pty_raw::RawPtySession::spawn(
            app_handle,
            tab_id.clone(),
            &worktree_path,
            24,
            80,
            &command,
            &args,
            &extra_env,
            &extra_args,
            integration_on,
            cli_agent_sock,
        )
        .map_err(|e| format!("Failed to spawn PTY: {e}"))?,
    };

    tab.pty = Some(pty);
    tab.alive = true;

    let mut app = state.lock();
    if target_ws_idx < app.workspaces.len() {
        app.workspaces[target_ws_idx].tabs.push(tab);
        app.workspaces[target_ws_idx].active_tab = app.workspaces[target_ws_idx].tabs.len() - 1;
    }

    Ok(tab_id)
}

// ── Live agent rows (Agents sidebar panel) ─────────────

/// One row in the desktop's Agents panel: a (workspace, tab) pair running an
/// AI agent, across ALL workspaces. Mirrors the TUI's `App::agent_rows()`
/// filter: Custom-provider tabs always list; a built-in tab lists only when
/// its cli-agent channel reported (a `claude` run manually inside it).
#[derive(Serialize, Clone)]
pub struct AgentRow {
    pub workspace_idx: usize,
    pub workspace_name: String,
    /// Index into the workspace's tab list — feed to `setActiveTab` to jump.
    pub tab_idx: usize,
    pub tab_id: String,
    /// Display label: the provider name, or "Claude (<tab label>)" for a
    /// non-Custom tab that only lists because its cli-agent channel reported.
    pub label: String,
    pub alive: bool,
    /// Structured cli-agent status, when the channel has reported.
    pub status: Option<piki_core::cli_agent::CliAgentStatus>,
    /// Unseen news (permission / idle / done the user hasn't looked at).
    pub attention: bool,
    pub summary: Option<String>,
    /// Seconds since the current run began (session start / last prompt),
    /// `None` once it stopped. The panel formats it (`3m 12s`) and ticks it
    /// locally between refreshes.
    pub elapsed_secs: Option<u64>,
}

#[tauri::command]
pub fn list_agent_rows(state: State<'_, Mutex<DesktopApp>>) -> Vec<AgentRow> {
    let app = state.lock();
    let mut rows = Vec::new();
    for (wi, ws) in app.workspaces.iter().enumerate() {
        for (ti, tab) in ws.tabs.iter().enumerate() {
            let snapshot = tab.pty.as_ref().and_then(|p| p.shell()).and_then(|s| {
                let guard = s.lock();
                piki_core::cli_agent::cli_agent_of(&guard.state).map(|a| {
                    (
                        a.status,
                        a.last_attention_at.is_some(),
                        a.last_summary.clone(),
                        a.elapsed().map(|d| d.as_secs()),
                    )
                })
            });
            let is_custom = matches!(tab.provider, piki_core::AIProvider::Custom(_));
            if !is_custom && snapshot.is_none() {
                continue;
            }
            let has_custom = tab
                .custom_title
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty());
            let label = if has_custom {
                tab.display_label()
            } else if is_custom {
                tab.provider.label().to_string()
            } else {
                format!("Claude ({})", tab.provider.label())
            };
            let (status, attention, summary, elapsed_secs) = match snapshot {
                Some((s, a, sum, e)) => (Some(s), a, sum, e),
                None => (None, false, None, None),
            };
            rows.push(AgentRow {
                workspace_idx: wi,
                workspace_name: ws.info.name.clone(),
                tab_idx: ti,
                tab_id: tab.id.clone(),
                label,
                alive: tab.alive,
                status,
                attention,
                summary,
                elapsed_secs,
            });
        }
    }
    rows
}

// ── External agents (via /proc) ─────────────

#[derive(Serialize, Clone)]
pub struct ExternalAgentPayload {
    pub pid: u32,
    pub ppid: u32,
    pub cwd: Option<String>,
    pub cmd: String,
    pub provider: String,
    pub workspace_idx: Option<usize>,
    pub workspace_name: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct ExternalTreePayload {
    pub root: ExternalAgentPayload,
    pub children: Vec<ExternalAgentPayload>,
}

#[tauri::command]
pub fn list_external_agents(state: State<'_, Mutex<DesktopApp>>) -> Vec<ExternalTreePayload> {
    let infos: Vec<piki_core::WorkspaceInfo> = {
        let app = state.lock();
        app.workspaces.iter().map(|w| w.info.clone()).collect()
    };
    let trees = piki_core::external_agents::scan_external_agents(&infos);
    // Need workspace names for rendering without extra lookup
    let names: Vec<String> = infos.iter().map(|i| i.name.clone()).collect();
    trees
        .into_iter()
        .map(|t| {
            let root_ws_name = t.root.workspace_idx.and_then(|idx| names.get(idx).cloned());
            let root = ExternalAgentPayload {
                pid: t.root.pid,
                ppid: t.root.ppid,
                cwd: t.root.cwd.map(|p| p.to_string_lossy().to_string()),
                cmd: t.root.cmd,
                provider: t.root.provider,
                workspace_idx: t.root.workspace_idx,
                workspace_name: root_ws_name,
            };
            let children = t
                .children
                .into_iter()
                .map(|c| {
                    let ws_name = c.workspace_idx.and_then(|idx| names.get(idx).cloned());
                    ExternalAgentPayload {
                        pid: c.pid,
                        ppid: c.ppid,
                        cwd: c.cwd.map(|p| p.to_string_lossy().to_string()),
                        cmd: c.cmd,
                        provider: c.provider.clone(),
                        workspace_idx: c.workspace_idx,
                        workspace_name: ws_name,
                    }
                })
                .collect();
            ExternalTreePayload { root, children }
        })
        .collect()
}
