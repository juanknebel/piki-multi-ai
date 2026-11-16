use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, AppMode, ToastLevel};
use crate::dialog_state::DialogState;
use piki_core::AIProvider;
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::BroadcastInput {
            targets,
            text,
            append_enter,
        } => {
            let mut payload = text.into_bytes();
            if append_enter {
                payload.push(b'\n');
            }
            let mut done = 0;
            for (ws_idx, tab_idx) in targets {
                let wrote = app
                    .workspaces
                    .get_mut(ws_idx)
                    .and_then(|ws| ws.tabs.get_mut(tab_idx))
                    .and_then(|tab| tab.pty_session.as_mut())
                    .map(|pty| pty.write(&payload).is_ok())
                    .unwrap_or(false);
                if wrote {
                    done += 1;
                }
            }
            app.set_toast(format!("Broadcast to {done} agent(s)"), ToastLevel::Success);
        }
        Action::SaveAgent {
            source_repo,
            profile,
        } => {
            if let Some(ref storage) = app.storage.agent_profiles {
                if let Err(e) = storage.save_agent(&profile) {
                    app.set_toast(
                        format!("Save agent failed: {}", e),
                        crate::app::ToastLevel::Error,
                    );
                } else {
                    // Reload agents for this project
                    if let Ok(agents) = storage.load_agents(&source_repo) {
                        app.agent_profiles = agents;
                    }
                    app.set_toast(
                        format!("Agent saved: {}", profile.name),
                        ToastLevel::Success,
                    );
                }
            }
        }
        Action::DeleteAgent(id) => {
            let repo = app.current_workspace().map(|ws| ws.source_repo.clone());
            if let Some(ref storage) = app.storage.agent_profiles {
                if let Err(e) = storage.delete_agent(id) {
                    app.set_toast(
                        format!("Delete agent failed: {}", e),
                        crate::app::ToastLevel::Error,
                    );
                } else {
                    if let Some(ref repo) = repo
                        && let Ok(agents) = storage.load_agents(repo)
                    {
                        app.agent_profiles = agents;
                    }
                    app.set_toast("Agent deleted".to_string(), ToastLevel::Success);
                }
            }
        }
        Action::SyncAgentToRepo(id) => {
            let ws_info = app
                .current_workspace()
                .map(|ws| (ws.path.clone(), ws.source_repo.clone()));
            let agent_data = app
                .agent_profiles
                .iter()
                .find(|a| a.id == Some(id))
                .map(|a| (a.name.clone(), a.provider.clone(), a.role.clone()));

            if let Some((ws_path, repo)) = ws_info
                && let Some((name, provider_str, role)) = agent_data
            {
                let provider = AIProvider::from_label(&provider_str);
                match materialize_agent_config(
                    &ws_path,
                    &name,
                    &provider,
                    &role,
                    Some(&app.provider_manager),
                ) {
                    Ok(()) => {
                        if let Some(ref storage) = app.storage.agent_profiles {
                            let _ = storage.mark_synced(id);
                            if let Ok(agents) = storage.load_agents(&repo) {
                                app.agent_profiles = agents;
                            }
                        }
                        app.set_toast(format!("Agent synced: {}", name), ToastLevel::Success);
                    }
                    Err(e) => {
                        app.set_toast(format!("Sync failed: {}", e), crate::app::ToastLevel::Error);
                    }
                }
            }
        }
        Action::ScanRepoAgents => {
            if let Some(ws) = app.current_workspace() {
                // The checkout the user is in, not its `source_repo`: a
                // worktree is its own working tree, and an agent file written
                // (or not yet committed) there is absent from the parent's.
                // The profiles it is compared against stay keyed by the repo.
                let ws_path = ws.path.clone();

                // Directory list, provider attribution and the
                // already-imported check all live in core so the desktop
                // applies the identical rules (see core::agent_scan).
                let discovered: Vec<(String, String, String, bool)> =
                    piki_core::agent_scan::scan_repo_agents(
                        &ws_path,
                        &app.provider_manager,
                        &app.agent_profiles,
                    )
                    .into_iter()
                    .map(|a| (a.name, a.provider, a.role, a.exists))
                    .collect();

                if discovered.is_empty() {
                    app.set_toast("No agent files found in repo".to_string(), ToastLevel::Info);
                } else {
                    // Pre-select only new agents (not already in DB)
                    let selected: Vec<bool> =
                        discovered.iter().map(|(_, _, _, exists)| !exists).collect();
                    app.active_dialog = Some(DialogState::ImportAgents {
                        discovered,
                        selected,
                        cursor: 0,
                    });
                    app.mode = AppMode::ImportAgents;
                }
            }
        }
        Action::ImportAgents(agents_to_import) => {
            if let Some(ws) = app.current_workspace() {
                let source_repo = ws.source_repo.clone();
                if let Some(ref storage) = app.storage.agent_profiles {
                    let mut imported = 0;
                    for (name, provider_label, role) in &agents_to_import {
                        let profile = piki_core::storage::AgentProfile {
                            id: None,
                            source_repo: source_repo.to_string_lossy().to_string(),
                            name: name.clone(),
                            provider: provider_label.clone(),
                            role: role.clone(),
                            version: 0,
                            last_synced_at: None,
                        };
                        if storage.save_agent(&profile).is_ok() {
                            imported += 1;
                            // Mark as synced — the file already exists in repo
                            if let Ok(agents) = storage.load_agents(&source_repo)
                                && let Some(saved) = agents
                                    .iter()
                                    .find(|a| a.name == *name && a.provider == *provider_label)
                                && let Some(id) = saved.id
                            {
                                let _ = storage.mark_synced(id);
                            }
                        }
                    }
                    // Reload agents
                    if let Ok(agents) = storage.load_agents(&source_repo) {
                        app.agent_profiles = agents;
                    }
                    app.set_toast(
                        format!("Imported {} agent(s)", imported),
                        ToastLevel::Success,
                    );
                }
            }
            // Return to manage agents dialog
            app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
            app.mode = AppMode::ManageAgents;
        }
        other => unreachable!("non-agent action routed to action::agent: {other:?}"),
    }
    Ok(())
}

/// Write agent role/instructions to the provider's standard subagent config path in the worktree.
fn materialize_agent_config(
    worktree_path: &std::path::Path,
    agent_name: &str,
    provider: &AIProvider,
    role: &str,
    provider_manager: Option<&piki_core::providers::ProviderManager>,
) -> anyhow::Result<()> {
    let filename = format!("{}.md", agent_name);
    // `agent_dir_for` falls back to the CLI's conventional directory when the
    // provider has none configured — Codex, Muse and Antigravity all ship
    // that way, and used to write nothing at all here while still reporting
    // a successful sync.
    let dir = if let AIProvider::Custom(name) = provider
        && let Some(mgr) = provider_manager
        && let Some(config) = mgr.get(name)
        && let Some(agent_dir) = piki_core::agent_scan::agent_dir_for(config)
    {
        agent_dir
    } else {
        return Ok(());
    };
    let agent_dir = worktree_path.join(dir);
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join(filename), role)?;
    Ok(())
}
