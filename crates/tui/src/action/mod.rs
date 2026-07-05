use std::path::PathBuf;
use std::sync::Arc;

use ratatui::DefaultTerminal;

use crate::app::{self, App, AppMode, ToastLevel};
use crate::dialog_state::{ConflictStrategy, DialogState};
use crate::helpers::spawn_tab;
use piki_core::workspace::{FileWatcher, WorkspaceManager};
use piki_core::{AIProvider, MergeStrategy, WorkspaceType};

mod files;
mod git_stash;
mod git;
mod git_merge;
mod workspace;
mod tabs;
mod review;
mod api;
mod chat;

/// Async actions triggered by key events
#[derive(Debug)]
pub(crate) enum Action {
    CreateWorkspace(
        String,
        String,
        String,
        Option<String>,
        PathBuf,
        WorkspaceType,
        Option<String>,
    ),
    /// Clone a GitHub URL into a user-chosen destination directory and
    /// register as Simple. Args:
    /// (name, description, prompt, kanban_path, github_url, destination_dir, group)
    CreateGithubWorkspace(
        String,
        String,
        String,
        Option<String>,
        String,
        std::path::PathBuf,
        Option<String>,
    ),
    EditWorkspace(usize, Option<String>, String, Option<String>),
    /// Second field: optional target kanban column for dispatched cards
    DeleteWorkspace(usize, Option<String>),
    /// Remove workspace from app list but keep worktree on disk
    RemoveFromList(usize),
    /// Open diff for the file at the given index in the active workspace
    OpenDiff(usize),
    /// Open $EDITOR for a file path
    OpenEditor(PathBuf),
    /// Git: stage a file at the given index
    GitStage(usize),
    /// Git: unstage a file at the given index
    GitUnstage(usize),
    /// Git: stage all multi-selected files
    GitStageSelected,
    /// Git: unstage all multi-selected files
    GitUnstageSelected,
    /// Git: commit with message
    GitCommit(String),
    /// Git: push current branch
    GitPush,
    /// Spawn a new tab with the given provider
    SpawnTab(AIProvider),
    /// Open a markdown file in a new tab
    OpenMarkdown(PathBuf),
    /// Open a markdown file in external mdr viewer
    OpenMdr(PathBuf),
    /// Git: merge workspace branch into main
    GitMerge(MergeStrategy),
    /// Undo last stage/unstage action
    Undo,
    /// Load PR review data (info + files) for the active workspace
    LoadPrReview,
    /// Load diff for a specific file in the PR review
    LoadPrFileDiff(usize),
    /// Submit the PR review using the draft state
    SubmitPrReview,
    /// Send an API request (raw Hurl text)
    SendApiRequest(String),
    /// Load git log for the active workspace
    LoadGitLog,
    /// View diff for a specific commit by SHA
    ViewCommitDiff(String),
    /// Git stash: list all stash entries
    GitStashList,
    /// Git stash: save with message
    GitStashSave(String),
    /// Git stash: pop entry at index
    GitStashPop(usize),
    /// Git stash: apply entry at index
    GitStashApply(usize),
    /// Git stash: drop entry at index
    GitStashDrop(usize),
    /// Git stash: show diff for entry at index
    GitStashShow(usize),
    /// View the conflict diff for a file (shows ours vs theirs)
    ViewConflictDiff(String),
    /// Resolve a merge conflict on a single file using the given strategy
    ResolveConflict {
        file: String,
        strategy: ConflictStrategy,
    },
    /// Abort the current merge or rebase
    AbortMerge,
    /// Scan for conflicts in worktree and source_repo, open resolution overlay
    DetectConflicts,
    /// Dispatch an agent to work on a kanban card
    DispatchAgent {
        source_ws: usize,
        card_id: String,
        card_title: String,
        card_description: String,
        card_priority: flow_core::Priority,
        card_project: String,
        provider: AIProvider,
        agent_name: Option<String>,
        agent_role: Option<String>,
        additional_prompt: String,
        use_current_ws: bool,
    },
    /// Save an agent profile to storage
    SaveAgent {
        source_repo: std::path::PathBuf,
        profile: piki_core::storage::AgentProfile,
    },
    /// Delete an agent profile by ID
    DeleteAgent(i64),
    /// Persist agent config file to the repo (Simple workspace only)
    SyncAgentToRepo(i64),
    /// Scan repo for agent files and open import dialog
    ScanRepoAgents,
    /// Import selected agents from repo files into storage: Vec<(name, provider_label, role)>
    ImportAgents(Vec<(String, String, String)>),
    /// Send the current chat input to Ollama and stream the response
    ChatSendMessage,
    /// Load available Ollama models into chat_panel.models
    ChatLoadModels,
}

pub(crate) async fn execute_action(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(..)
        | Action::CreateGithubWorkspace(..)
        | Action::EditWorkspace(..)
        | Action::DeleteWorkspace(..)
        | Action::RemoveFromList(..) => {
            workspace::handle(app, manager, action, terminal).await?
        }
        Action::OpenEditor(..) | Action::OpenDiff(..) => {
            files::handle(app, manager, action, terminal).await?
        }
        Action::GitStage(..)
        | Action::GitUnstage(..)
        | Action::GitStageSelected
        | Action::GitUnstageSelected
        | Action::GitCommit(..)
        | Action::GitPush
        | Action::Undo
        | Action::LoadGitLog
        | Action::ViewCommitDiff(..) => {
            git::handle(app, manager, action, terminal).await?
        }
        Action::GitMerge(..)
        | Action::ViewConflictDiff(..)
        | Action::ResolveConflict { .. }
        | Action::AbortMerge
        | Action::DetectConflicts => {
            git_merge::handle(app, manager, action, terminal).await?
        }
        Action::LoadPrReview | Action::LoadPrFileDiff(..) | Action::SubmitPrReview => {
            review::handle(app, manager, action, terminal).await?
        }
        Action::SpawnTab(..) | Action::OpenMarkdown(..) | Action::OpenMdr(..) => {
            tabs::handle(app, manager, action, terminal).await?
        }
        Action::SendApiRequest(..) => {
            api::handle(app, manager, action, terminal).await?
        }
        Action::GitStashList
        | Action::GitStashSave(..)
        | Action::GitStashPop(..)
        | Action::GitStashApply(..)
        | Action::GitStashDrop(..)
        | Action::GitStashShow(..) => {
            git_stash::handle(app, manager, action, terminal).await?
        }
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
            let (source_dir, source_ws_name) = match app.workspaces.get(source_ws) {
                Some(ws) => (ws.source_repo.clone(), ws.name.clone()),
                None => {
                    app.status_message = Some("Source workspace not found".into());
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
                if let Some(src_ws) = app.workspaces.get_mut(source_ws)
                    && let Some(ref mut kp) = src_ws.kanban_provider
                {
                    let _ = kp.update_card(
                        &card_id,
                        &card_title,
                        &card_description,
                        card_priority,
                        assignee_label,
                        &card_project,
                    );
                    let _ = kp.move_card(&card_id, "in_progress");
                    if let Some(ref mut ka) = src_ws.kanban_app
                        && let Ok(board) = kp.load_board()
                    {
                        ka.board = board;
                        ka.clamp();
                    }
                }

                // Spawn tab in current workspace
                let ws = &mut app.workspaces[source_ws];
                let idx =
                    spawn_tab(ws, &provider, app.pty_rows, app.pty_cols, Some(&task_prompt), Some(&app.provider_manager), &app.paths).await;
                ws.active_tab = idx;

                app.set_toast(
                    format!("Task started: {} via {}", card_title, provider.label()),
                    ToastLevel::Success,
                );
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
                let group_name = format!("{}-AGENTS", source_ws_name);

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
                        info.group = Some(group_name);
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
                            let _ = materialize_agent_config(&info.path, name, &provider, role, Some(&app.provider_manager));
                        }

                        // Update kanban card: set assignee and move to IN PROGRESS
                        let assignee_label =
                            agent_name.as_deref().unwrap_or(provider.label());
                        if let Some(src_ws) = app.workspaces.get_mut(source_ws)
                            && let Some(ref mut kp) = src_ws.kanban_provider
                        {
                            let _ = kp.update_card(
                                &card_id,
                                &card_title,
                                &card_description,
                                card_priority,
                                assignee_label,
                                &card_project,
                            );
                            let _ = kp.move_card(&card_id, "in_progress");
                            if let Some(ref mut ka) = src_ws.kanban_app
                                && let Ok(board) = kp.load_board()
                            {
                                ka.board = board;
                                ka.clamp();
                            }
                        }

                        // Create workspace and switch to it
                        app.workspaces.push(app::Workspace::from_info(info));
                        let new_idx = app.workspaces.len() - 1;
                        app.switch_workspace(new_idx);

                        // Start file watcher
                        let ws = &mut app.workspaces[new_idx];
                        match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
                            Ok(watcher) => ws.watcher = Some(watcher),
                            Err(e) => {
                                app.status_message = Some(format!("Watcher error: {}", e));
                            }
                        }

                        // Spawn AI provider tab with task prompt
                        let ws = &mut app.workspaces[new_idx];
                        let idx = spawn_tab(
                            ws,
                            &provider,
                            app.pty_rows,
                            app.pty_cols,
                            Some(&task_prompt),
                            Some(&app.provider_manager),
                            &app.paths,
                        )
                        .await;
                        ws.active_tab = idx;

                        // Persist config async
                        {
                            let source = app.workspaces[new_idx].source_repo.clone();
                            let infos: Vec<_> =
                                app.workspaces.iter().map(|w| w.info.clone()).collect();
                            let storage = Arc::clone(&app.storage);
                            tokio::spawn(async move {
                                let _ = storage.workspaces.save_workspaces(&source, &infos);
                            });
                        }

                        app.set_toast(
                            format!(
                                "Agent dispatched: {} via {}",
                                card_title,
                                provider.label()
                            ),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Dispatch failed: {}", e));
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
                    app.status_message = Some(format!("Save agent failed: {}", e));
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
                    app.status_message = Some(format!("Delete agent failed: {}", e));
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
            let ws_info = app.current_workspace().map(|ws| {
                (ws.path.clone(), ws.source_repo.clone())
            });
            let agent_data = app
                .agent_profiles
                .iter()
                .find(|a| a.id == Some(id))
                .map(|a| (a.name.clone(), a.provider.clone(), a.role.clone()));

            if let Some((ws_path, repo)) = ws_info
                && let Some((name, provider_str, role)) = agent_data
            {
                let provider = AIProvider::from_label(&provider_str);
                match materialize_agent_config(&ws_path, &name, &provider, &role, Some(&app.provider_manager)) {
                    Ok(()) => {
                        if let Some(ref storage) = app.storage.agent_profiles {
                            let _ = storage.mark_synced(id);
                            if let Ok(agents) = storage.load_agents(&repo) {
                                app.agent_profiles = agents;
                            }
                        }
                        app.set_toast(
                            format!("Agent synced: {}", name),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Sync failed: {}", e));
                    }
                }
            }
        }
        Action::ScanRepoAgents => {
            if let Some(ws) = app.current_workspace() {
                let source_repo = ws.source_repo.clone();

                // Scan provider agent directories for .md files — all come from ProviderManager.
                let provider_dirs: Vec<(String, String)> = app
                    .provider_manager
                    .all()
                    .iter()
                    .filter_map(|config| {
                        config
                            .agent_dir
                            .as_ref()
                            .map(|d| (d.clone(), config.name.clone()))
                    })
                    .collect();

                let mut discovered: Vec<(String, String, String, bool)> = Vec::new();

                for (dir, provider_label) in &provider_dirs {
                    let agent_dir = source_repo.join(dir);
                    if let Ok(entries) = std::fs::read_dir(&agent_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().is_some_and(|e| e == "md")
                                && let Some(stem) = path.file_stem()
                            {
                                let name = stem.to_string_lossy().to_string();
                                let role =
                                    std::fs::read_to_string(&path).unwrap_or_default();
                                let exists = app.agent_profiles.iter().any(|a| {
                                    a.name == name && a.provider == *provider_label
                                });
                                discovered.push((
                                    name,
                                    provider_label.clone(),
                                    role,
                                    exists,
                                ));
                            }
                        }
                    }
                }

                if discovered.is_empty() {
                    app.set_toast(
                        "No agent files found in repo".to_string(),
                        ToastLevel::Info,
                    );
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

        Action::ChatSendMessage | Action::ChatLoadModels => {
            chat::handle(app, manager, action, terminal).await?
        }
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

