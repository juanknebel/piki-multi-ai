use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{self, App, AppMode, ToastLevel};
use crate::dialog_state::DialogState;
use crate::helpers::spawn_tab;
use piki_core::AIProvider;
use piki_core::workspace::{FileWatcher, WorkspaceManager};

pub(super) async fn handle(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::DispatchAgent {
            source_ws,
            card_id,
            card_title,
            card_description,
            card_priority,
            card_project,
            provider,
            agent_name,
            agent_role,
            additional_prompt,
            use_current_ws,
        } => {
            // 1. Extract source workspace data
            let source_dir = match app.workspaces.get(source_ws) {
                Some(ws) => ws.source_repo.clone(),
                None => {
                    app.set_toast("Source workspace not found", crate::app::ToastLevel::Info);
                    return Ok(());
                }
            };
            let kanban_path = app
                .workspaces
                .get(source_ws)
                .and_then(|ws| ws.kanban_path.clone());

            // 2. Compose task prompt: always include card title so the agent starts working
            let task_prompt = if let Some(ref name) = agent_name {
                let mut parts = vec![format!(
                    "Use the {} agent to plan and then implement the task: {}",
                    name, card_title
                )];
                if !card_description.is_empty() {
                    parts.push(card_description.clone());
                }
                if !additional_prompt.trim().is_empty() {
                    parts.push(additional_prompt.trim().to_string());
                }
                parts.join("\n\n")
            } else {
                let mut parts = vec![card_title.clone()];
                if !card_description.is_empty() {
                    parts.push(card_description.clone());
                }
                if !additional_prompt.trim().is_empty() {
                    parts.push(additional_prompt.trim().to_string());
                }
                parts.join("\n\n")
            };

            if use_current_ws {
                // Use current workspace — just spawn a new tab, no worktree
                // Update kanban card
                let assignee_label = agent_name.as_deref().unwrap_or(provider.label());
                let mut kanban_err: Option<String> = None;
                if let Some(src_ws) = app.workspaces.get_mut(source_ws)
                    && let Some(ref mut kp) = src_ws.kanban_provider
                {
                    if let Err(e) = kp.update_card(
                        &card_id,
                        &card_title,
                        &card_description,
                        card_priority,
                        assignee_label,
                        &card_project,
                    ) {
                        kanban_err = Some(e.to_string());
                    }
                    if let Err(e) = kp.move_card(&card_id, "in_progress") {
                        kanban_err = Some(e.to_string());
                    }
                    if let Some(ref mut ka) = src_ws.kanban_app
                        && let Ok(board) = kp.load_board()
                    {
                        ka.board = board;
                        ka.clamp();
                    }
                }
                if let Some(e) = kanban_err {
                    app.set_toast(
                        format!("Kanban card sync failed: {e}"),
                        crate::app::ToastLevel::Error,
                    );
                }

                // Spawn tab in current workspace
                let ws = &mut app.workspaces[source_ws];
                let (idx, spawn_error) = spawn_tab(
                    ws,
                    &provider,
                    app.pty_rows,
                    app.pty_cols,
                    Some(&task_prompt),
                    Some(&app.provider_manager),
                    &app.paths,
                    app.pty_output.clone(),
                )
                .await;
                ws.active_tab = idx;
                app.active_pane = crate::app::ActivePane::MainPanel;

                match spawn_error {
                    Some(err) => app.set_toast(err, ToastLevel::Error),
                    None => app.set_toast(
                        format!("Task started: {} via {}", card_title, provider.label()),
                        ToastLevel::Success,
                    ),
                }
            } else {
                // Create new worktree workspace (original flow)
                // Build branch name: <type>/<sanitized_card_id>
                let type_prefix = match card_priority {
                    flow_core::Priority::Bug => "bug",
                    flow_core::Priority::Wishlist => "spike",
                    _ => "feature",
                };
                let sanitized_id: String = card_id
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                let ws_name = format!("{}/{}", type_prefix, sanitized_id);

                let result = manager
                    .create(
                        &ws_name,
                        &card_title,
                        &task_prompt,
                        kanban_path.clone(),
                        &source_dir,
                    )
                    .await;

                match result {
                    Ok(mut info) => {
                        info.dispatch_card_id = Some(card_id.clone());
                        info.dispatch_source_kanban = kanban_path;
                        info.dispatch_agent_name = agent_name.clone();
                        info.order = app
                            .workspaces
                            .iter()
                            .map(|w| w.info.order)
                            .max()
                            .map(|m| m + 1)
                            .unwrap_or(0);

                        // Materialize agent config files in worktree
                        if let (Some(name), Some(role)) = (&agent_name, &agent_role) {
                            let _ = materialize_agent_config(
                                &info.path,
                                name,
                                &provider,
                                role,
                                Some(&app.provider_manager),
                            );
                        }

                        // Update kanban card: set assignee and move to IN PROGRESS
                        let assignee_label = agent_name.as_deref().unwrap_or(provider.label());
                        let mut kanban_err: Option<String> = None;
                        if let Some(src_ws) = app.workspaces.get_mut(source_ws)
                            && let Some(ref mut kp) = src_ws.kanban_provider
                        {
                            if let Err(e) = kp.update_card(
                                &card_id,
                                &card_title,
                                &card_description,
                                card_priority,
                                assignee_label,
                                &card_project,
                            ) {
                                kanban_err = Some(e.to_string());
                            }
                            if let Err(e) = kp.move_card(&card_id, "in_progress") {
                                kanban_err = Some(e.to_string());
                            }
                            if let Some(ref mut ka) = src_ws.kanban_app
                                && let Ok(board) = kp.load_board()
                            {
                                ka.board = board;
                                ka.clamp();
                            }
                        }
                        if let Some(e) = kanban_err {
                            app.set_toast(
                                format!("Kanban card sync failed: {e}"),
                                crate::app::ToastLevel::Error,
                            );
                        }

                        // Create workspace and switch to it
                        app.workspaces.push(app::Workspace::from_info(info));
                        let new_idx = app.workspaces.len() - 1;
                        app.switch_workspace_and_focus(new_idx);

                        // Start file watcher
                        let ws = &mut app.workspaces[new_idx];
                        match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
                            Ok(watcher) => ws.watcher = Some(watcher),
                            Err(e) => {
                                app.set_toast(
                                    format!("Watcher error: {}", e),
                                    crate::app::ToastLevel::Error,
                                );
                            }
                        }

                        // Spawn AI provider tab with task prompt
                        let ws = &mut app.workspaces[new_idx];
                        let (idx, spawn_error) = spawn_tab(
                            ws,
                            &provider,
                            app.pty_rows,
                            app.pty_cols,
                            Some(&task_prompt),
                            Some(&app.provider_manager),
                            &app.paths,
                            app.pty_output.clone(),
                        )
                        .await;
                        ws.active_tab = idx;

                        // Persist config async
                        {
                            let source = app.workspaces[new_idx].source_repo.clone();
                            crate::helpers::persist_workspaces(app, source);
                        }

                        match spawn_error {
                            Some(err) => app.set_toast(err, ToastLevel::Error),
                            None => app.set_toast(
                                format!(
                                    "Agent dispatched: {} via {}",
                                    card_title,
                                    provider.label()
                                ),
                                ToastLevel::Success,
                            ),
                        }
                    }
                    Err(e) => {
                        app.set_toast(
                            format!("Dispatch failed: {}", e),
                            crate::app::ToastLevel::Error,
                        );
                    }
                }
            }
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
                let source_repo = ws.source_repo.clone();

                // Directory list, provider attribution and the
                // already-imported check all live in core so the desktop
                // applies the identical rules (see core::agent_scan).
                let discovered: Vec<(String, String, String, bool)> =
                    piki_core::agent_scan::scan_repo_agents(
                        &source_repo,
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
    let dir = if let AIProvider::Custom(name) = provider
        && let Some(mgr) = provider_manager
        && let Some(config) = mgr.get(name)
        && let Some(agent_dir) = &config.agent_dir
    {
        agent_dir.clone()
    } else {
        return Ok(());
    };
    let agent_dir = worktree_path.join(dir);
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join(filename), role)?;
    Ok(())
}
