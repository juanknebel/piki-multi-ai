use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::State;

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

    // Load existing agents for "exists" check
    let existing: Vec<String> = match &storage.agent_profiles {
        Some(s) => s
            .load_agents(&source_repo)
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.name)
            .collect(),
        None => Vec::new(),
    };

    let provider_dirs: &[(&str, &str)] = &[
        (".claude/agents", "Claude Code"),
        (".gemini/agents", "Gemini"),
        (".opencode/agents", "OpenCode"),
        (".kilo/agents", "Kilo"),
        (".codex/agents", "Codex"),
    ];

    let mut discovered = Vec::new();

    for &(dir, provider_label) in provider_dirs {
        let agent_dir = source_repo.join(dir);
        if !agent_dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&agent_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let role = std::fs::read_to_string(&path).unwrap_or_default();
                    let exists = existing.contains(&name);
                    discovered.push(ScannedAgent {
                        name,
                        provider: provider_label.to_string(),
                        role,
                        exists,
                    });
                }
            }
        }
    }

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

#[tauri::command]
pub async fn dispatch_agent(
    state: State<'_, Mutex<DesktopApp>>,
    app_handle: tauri::AppHandle,
    workspace_idx: usize,
    provider: String,
    prompt: String,
    create_worktree: bool,
    ws_name: Option<String>,
    group: Option<String>,
    dispatch_card_id: Option<String>,
    dispatch_source_kanban: Option<String>,
    dispatch_agent_name: Option<String>,
    dispatch_card_title: Option<String>,
) -> Result<String, String> {
    let ai_provider = match provider.as_str() {
        "Claude Code" | "Claude" => piki_core::AIProvider::Claude,
        "Gemini" => piki_core::AIProvider::Gemini,
        "OpenCode" => piki_core::AIProvider::OpenCode,
        "Kilo" => piki_core::AIProvider::Kilo,
        "Codex" => piki_core::AIProvider::Codex,
        _ => piki_core::AIProvider::Claude,
    };

    let command = ai_provider.resolved_command();
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

        let description = dispatch_card_title
            .as_deref()
            .unwrap_or("Agent dispatch");

        let name = ws_name.unwrap_or_else(|| format!("agent-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("x")));
        let info = manager
            .create(&name, &description, &prompt, source_kanban_path, &source_dir)
            .await
            .map_err(|e| format!("Failed to create workspace: {e}"))?;

        let watcher =
            piki_core::workspace::watcher::FileWatcher::new(info.path.clone(), info.name.clone())
                .ok();

        let mut app = state.lock();
        let order = app.workspaces.iter().map(|ws| ws.info.order).max().unwrap_or(0) + 1;
        let mut ws_info = info;
        ws_info.order = order;
        if let Some(ref g) = group {
            ws_info.group = Some(g.clone());
        }
        ws_info.dispatch_card_id = dispatch_card_id;
        ws_info.dispatch_source_kanban = dispatch_source_kanban;
        ws_info.dispatch_agent_name = dispatch_agent_name;

        app.workspaces.push(crate::state::DesktopWorkspace {
            info: ws_info,
            status: piki_core::WorkspaceStatus::Idle,
            changed_files: Vec::new(),
            ahead_behind: None,
            tabs: Vec::new(),
            active_tab: 0,
            watcher,
        });

        let idx = app.workspaces.len() - 1;
        app.active_workspace = idx;
        idx
    } else {
        workspace_idx
    };

    // Spawn the AI tab with prompt
    let worktree_path = {
        let app = state.lock();
        if target_ws_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        app.workspaces[target_ws_idx].info.path.clone()
    };

    let args = ai_provider.prompt_args(&prompt);
    let mut tab = crate::state::DesktopTab::new(ai_provider);
    let tab_id = tab.id.clone();

    let pty = crate::pty_raw::RawPtySession::spawn(
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
    if target_ws_idx < app.workspaces.len() {
        app.workspaces[target_ws_idx].tabs.push(tab);
        app.workspaces[target_ws_idx].active_tab =
            app.workspaces[target_ws_idx].tabs.len() - 1;
    }

    Ok(tab_id)
}
