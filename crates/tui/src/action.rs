use std::path::PathBuf;
use std::sync::Arc;

use ratatui::DefaultTerminal;

use crate::app::{self, ActivePane, App, AppMode, ToastLevel};
use crate::code_review::CodeReviewState;
use crate::dialog_state::{ConflictFile, ConflictStrategy, DialogState, GitLogEntry};
use crate::helpers::spawn_tab;
use piki_core::workspace::{FileWatcher, WorkspaceManager};
use piki_core::{AIProvider, MergeStrategy, WorkspaceType};

/// Async actions triggered by key events
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
        Action::CreateWorkspace(name, description, prompt, kanban_path, dir, ws_type, group) => {
            let result = match ws_type {
                WorkspaceType::Simple => {
                    manager
                        .create_simple(&name, &description, &prompt, kanban_path, &dir)
                        .await
                }
                WorkspaceType::Worktree => {
                    manager
                        .create(&name, &description, &prompt, kanban_path, &dir)
                        .await
                }
                WorkspaceType::Project => {
                    manager
                        .create_project(&name, &description, &prompt, kanban_path, &dir)
                        .await
                }
            };
            match result {
                Ok(mut info) => {
                    info.group = group;
                    info.order = app
                        .workspaces
                        .iter()
                        .map(|w| w.info.order)
                        .max()
                        .map(|m| m + 1)
                        .unwrap_or(0);
                    app.workspaces.push(app::Workspace::from_info(info));
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    // Populate sub-directories for Project workspaces
                    if ws_type == WorkspaceType::Project {
                        app.workspaces[new_idx].refresh_sub_directories().await;
                    }

                    // Start file watcher
                    let ws = &mut app.workspaces[new_idx];
                    match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
                        Ok(watcher) => {
                            ws.watcher = Some(watcher);
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Watcher error: {}", e));
                        }
                    }

                    // Persist config async
                    {
                        let source = app.workspaces[new_idx].source_repo.clone();
                        let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                        let storage = Arc::clone(&app.storage);
                        tokio::spawn(async move {
                            let _ = storage.workspaces.save_workspaces(&source, &infos);
                        });
                    }
                }
                Err(e) => {
                    app.status_message = Some(format!("Error: {}", e));
                }
            }
        }
        Action::EditWorkspace(idx, kanban_path, prompt, group) => {
            if let Some(ws) = app.workspaces.get_mut(idx) {
                if ws.kanban_path != kanban_path {
                    ws.kanban_app = None;
                    ws.kanban_provider = None;
                }
                ws.kanban_path = kanban_path;
                ws.prompt = prompt;
                ws.info.group = group;
                {
                    let source = ws.source_repo.clone();
                    let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                    let storage = Arc::clone(&app.storage);
                    tokio::spawn(async move {
                        let _ = storage.workspaces.save_workspaces(&source, &infos);
                    });
                }
                app.set_toast("Workspace updated", ToastLevel::Success);
            }
        }
        Action::DeleteWorkspace(idx, target_column) => {
            if idx < app.workspaces.len() {
                // If this was a dispatched agent, move card to the chosen column
                let dispatch_info = app.workspaces[idx]
                    .info
                    .dispatch_card_id
                    .clone()
                    .zip(app.workspaces[idx].info.dispatch_source_kanban.clone());
                if let Some((card_id, kanban_path)) = dispatch_info
                    && let Some(target_col) = target_column
                {
                    let source_ws_idx = app.workspaces.iter().position(|w| {
                        w.kanban_path.as_deref() == Some(kanban_path.as_str())
                            && w.kanban_provider.is_some()
                    });
                    if let Some(src_idx) = source_ws_idx {
                        let src_ws = &mut app.workspaces[src_idx];
                        if let Some(ref mut kp) = src_ws.kanban_provider {
                            if let Ok(board) = kp.load_board() {
                                for col in &board.columns {
                                    if let Some(card) =
                                        col.cards.iter().find(|c| c.id == card_id)
                                    {
                                        let _ = kp.update_card(
                                            &card_id,
                                            &card.title,
                                            &card.description,
                                            card.priority,
                                            "",
                                            &card.project,
                                        );
                                        break;
                                    }
                                }
                            }
                            let _ = kp.move_card(&card_id, &target_col);
                            if let Ok(board) = kp.load_board()
                                && let Some(ref mut ka) = src_ws.kanban_app
                            {
                                ka.board = board;
                                ka.clamp();
                            }
                        }
                    }
                }

                let is_worktree =
                    app.workspaces[idx].info.workspace_type == WorkspaceType::Worktree;

                // Kill all PTY sessions before removing
                for tab in &mut app.workspaces[idx].tabs {
                    if let Some(ref mut pty) = tab.pty_session {
                        let _ = pty.kill();
                    }
                }
                // Drop watcher (stops watching)
                app.workspaces[idx].watcher = None;

                let source_repo = app.workspaces[idx].source_repo.clone();

                let removed = if !is_worktree {
                    // Simple workspaces: just remove from list
                    app.workspaces.remove(idx);
                    true
                } else {
                    let name = app.workspaces[idx].name.clone();
                    match manager.remove(&name, &source_repo).await {
                        Ok(()) => {
                            app.workspaces.remove(idx);
                            true
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                            false
                        }
                    }
                };

                if removed {
                    // Adjust indices
                    if app.workspaces.is_empty() {
                        app.active_workspace = 0;
                        app.selected_workspace = 0;
                    } else {
                        if app.active_workspace >= app.workspaces.len() {
                            app.active_workspace = app.workspaces.len() - 1;
                        }
                        if app.selected_workspace >= app.workspaces.len() {
                            app.selected_workspace = app.workspaces.len() - 1;
                        }
                    }

                    // Persist config
                    {
                        let source = source_repo.clone();
                        let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                        let storage = Arc::clone(&app.storage);
                        tokio::spawn(async move {
                            let _ = storage.workspaces.save_workspaces(&source, &infos);
                        });
                    }
                }
            }
        }
        Action::RemoveFromList(idx) => {
            if idx < app.workspaces.len() {
                // Kill all PTY sessions
                for tab in &mut app.workspaces[idx].tabs {
                    if let Some(ref mut pty) = tab.pty_session {
                        let _ = pty.kill();
                    }
                }
                app.workspaces[idx].watcher = None;

                let source_repo = app.workspaces[idx].source_repo.clone();
                app.workspaces.remove(idx);

                // Adjust indices
                if app.workspaces.is_empty() {
                    app.active_workspace = 0;
                    app.selected_workspace = 0;
                } else {
                    if app.active_workspace >= app.workspaces.len() {
                        app.active_workspace = app.workspaces.len() - 1;
                    }
                    if app.selected_workspace >= app.workspaces.len() {
                        app.selected_workspace = app.workspaces.len() - 1;
                    }
                }

                // Persist config
                {
                    let source = source_repo.clone();
                    let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                    let storage = Arc::clone(&app.storage);
                    tokio::spawn(async move {
                        let _ = storage.workspaces.save_workspaces(&source, &infos);
                    });
                }
            }
        }
        Action::OpenEditor(path) => {
            // Suspend TUI, open $EDITOR, restore TUI
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            )?;
            ratatui::restore();
            let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor_cmd).arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture,
                crossterm::event::EnableBracketedPaste,
                crossterm::event::PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            )?;
            match status {
                Ok(s) if s.success() => {
                    if let Some(ws) = app.current_workspace_mut() {
                        ws.dirty = true;
                    }
                    app.status_message = Some(format!("Edited: {}", path.display()));
                }
                Ok(s) => {
                    app.status_message = Some(format!("Editor exited with: {}", s));
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to run {}: {}", editor_cmd, e));
                }
            }
            // Close fuzzy search if it was open
            if app.mode == AppMode::FuzzySearch {
                app.fuzzy = None;
                app.mode = AppMode::Normal;
            }
        }
        Action::OpenDiff(file_idx) => {
            if let Some(ws) = app.workspaces.get(app.active_workspace)
                && let Some(file) = ws.changed_files.get(file_idx)
            {
                let file_path = file.path.clone();
                // Compute diff width from actual terminal size (matches diff overlay: 90% width minus borders)
                let term_size = terminal.size()?;
                let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
                let width = if overlay_inner_width > 10 {
                    overlay_inner_width
                } else {
                    120
                };
                let cache_key = format!("{}@{}", file_path, width);
                // Check cache first to avoid re-running git diff | delta
                if let Some(cached) = app.diff_cache.get(&cache_key) {
                    app.diff_content = Some(Arc::clone(cached));
                    app.diff_file_path = Some(file_path);
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                    app.interacting = true;
                } else {
                    let worktree_path = ws.path.clone();
                    let file_status = file.status.clone();
                    match piki_core::diff::runner::run_diff(
                        &worktree_path,
                        &file_path,
                        width,
                        &file_status,
                    )
                    .await
                    {
                        Ok(ansi_bytes) => {
                            use ansi_to_tui::IntoText;
                            match ansi_bytes.into_text() {
                                Ok(text) => {
                                    let text = Arc::new(text);
                                    app.insert_diff_cache(cache_key, Arc::clone(&text));
                                    app.diff_content = Some(text);
                                    app.diff_file_path = Some(file_path);
                                    app.diff_scroll = 0;
                                    app.mode = AppMode::Diff;
                                    app.active_pane = ActivePane::MainPanel;
                                    app.interacting = true;
                                }
                                Err(e) => {
                                    app.status_message =
                                        Some(format!("Failed to parse diff: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Diff error: {}", e));
                        }
                    }
                }
            }
        }
        Action::GitStage(file_idx) => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx)
                && let Some(file) = ws.changed_files.get(file_idx)
            {
                let file_path = file.path.clone();
                let worktree = ws.path.clone();
                let status_tx = app.status_tx.clone();
                let undo_tx = app.undo_tx.clone();
                app.status_message = Some(format!("Staging: {}", file_path));
                ws.dirty = true;
                ws.last_refresh = None;
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("git")
                        .args(["add", &file_path])
                        .current_dir(&worktree)
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            let _ = undo_tx.send(app::UndoEntry {
                                action: app::UndoAction::Stage,
                                workspace_idx: ws_idx,
                                file_path: file_path.clone(),
                            });
                            let _ = status_tx.send(format!("Staged: {} [C-z undo]", file_path));
                        }
                        Ok(o) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            let _ = status_tx.send(format!("Stage failed: {}", stderr.trim()));
                        }
                        Err(e) => {
                            let _ = status_tx.send(format!("Stage error: {}", e));
                        }
                    }
                });
            }
        }
        Action::GitUnstage(file_idx) => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx)
                && let Some(file) = ws.changed_files.get(file_idx)
            {
                let file_path = file.path.clone();
                let worktree = ws.path.clone();
                let status_tx = app.status_tx.clone();
                let undo_tx = app.undo_tx.clone();
                app.status_message = Some(format!("Unstaging: {}", file_path));
                ws.dirty = true;
                ws.last_refresh = None;
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("git")
                        .args(["reset", "HEAD", &file_path])
                        .current_dir(&worktree)
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            let _ = undo_tx.send(app::UndoEntry {
                                action: app::UndoAction::Unstage,
                                workspace_idx: ws_idx,
                                file_path: file_path.clone(),
                            });
                            let _ = status_tx.send(format!("Unstaged: {} [C-z undo]", file_path));
                        }
                        Ok(o) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            let _ = status_tx.send(format!("Unstage failed: {}", stderr.trim()));
                        }
                        Err(e) => {
                            let _ = status_tx.send(format!("Unstage error: {}", e));
                        }
                    }
                });
            }
        }
        Action::GitStageSelected => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx) {
                let paths: Vec<String> = app.selected_files.iter().cloned().collect();
                if !paths.is_empty() {
                    let worktree = ws.path.clone();
                    let status_tx = app.status_tx.clone();
                    let undo_tx = app.undo_tx.clone();
                    let count = paths.len();
                    app.status_message = Some(format!("Staging {} files...", count));
                    ws.dirty = true;
                    ws.last_refresh = None;
                    app.selected_files.clear();
                    tokio::spawn(async move {
                        let mut args = vec!["add".to_string()];
                        args.extend(paths.iter().cloned());
                        let output = tokio::process::Command::new("git")
                            .args(&args)
                            .current_dir(&worktree)
                            .output()
                            .await;
                        match output {
                            Ok(o) if o.status.success() => {
                                for p in &paths {
                                    let _ = undo_tx.send(app::UndoEntry {
                                        action: app::UndoAction::Stage,
                                        workspace_idx: ws_idx,
                                        file_path: p.clone(),
                                    });
                                }
                                let _ =
                                    status_tx.send(format!("Staged {} files [C-z undo]", count));
                            }
                            Ok(o) => {
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                let _ =
                                    status_tx.send(format!("Stage failed: {}", stderr.trim()));
                            }
                            Err(e) => {
                                let _ = status_tx.send(format!("Stage error: {}", e));
                            }
                        }
                    });
                }
            }
        }
        Action::GitUnstageSelected => {
            let ws_idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(ws_idx) {
                let paths: Vec<String> = app.selected_files.iter().cloned().collect();
                if !paths.is_empty() {
                    let worktree = ws.path.clone();
                    let status_tx = app.status_tx.clone();
                    let undo_tx = app.undo_tx.clone();
                    let count = paths.len();
                    app.status_message = Some(format!("Unstaging {} files...", count));
                    ws.dirty = true;
                    ws.last_refresh = None;
                    app.selected_files.clear();
                    tokio::spawn(async move {
                        let mut args = vec!["reset".to_string(), "HEAD".to_string()];
                        args.extend(paths.iter().cloned());
                        let output = tokio::process::Command::new("git")
                            .args(&args)
                            .current_dir(&worktree)
                            .output()
                            .await;
                        match output {
                            Ok(o) if o.status.success() => {
                                for p in &paths {
                                    let _ = undo_tx.send(app::UndoEntry {
                                        action: app::UndoAction::Unstage,
                                        workspace_idx: ws_idx,
                                        file_path: p.clone(),
                                    });
                                }
                                let _ = status_tx
                                    .send(format!("Unstaged {} files [C-z undo]", count));
                            }
                            Ok(o) => {
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                let _ =
                                    status_tx.send(format!("Unstage failed: {}", stderr.trim()));
                            }
                            Err(e) => {
                                let _ = status_tx.send(format!("Unstage error: {}", e));
                            }
                        }
                    });
                }
            }
        }
        Action::GitCommit(message) => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let worktree = ws.path.clone();
                let output = tokio::process::Command::new("git")
                    .args(["commit", "-m", &message])
                    .current_dir(&worktree)
                    .output()
                    .await?;
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let first_line = stdout.lines().next().unwrap_or("Committed");
                    app.status_message = Some(format!("✓ {}", first_line));
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    app.status_message = Some(format!("Commit failed: {}", stderr.trim()));
                }
                ws.dirty = true;
                ws.last_refresh = None;
            }
        }
        Action::GitPush => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let worktree = ws.path.clone();
                let output = tokio::process::Command::new("git")
                    .args(["push"])
                    .current_dir(&worktree)
                    .output()
                    .await?;
                if output.status.success() {
                    app.status_message = Some("✓ Pushed successfully".into());
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    app.status_message = Some(format!("Push failed: {}", stderr.trim()));
                }
            }
        }
        Action::GitMerge(strategy) => {
            // Extract data we need and drop the mutable borrow on workspaces
            let merge_data = app.workspaces.get(app.active_workspace).map(|ws| {
                (
                    ws.source_repo.clone(),
                    ws.branch.clone(),
                    ws.path.clone(),
                )
            });
            if let Some((source_repo, branch, ws_path)) = merge_data {
                // Check workspace has no uncommitted changes
                let status_output = tokio::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&ws_path)
                    .output()
                    .await?;
                let status_str = String::from_utf8_lossy(&status_output.stdout);
                if !status_str.trim().is_empty() {
                    app.status_message =
                        Some("Merge aborted: workspace has uncommitted changes".into());
                    return Ok(());
                }

                // Detect main branch
                let main_branch = WorkspaceManager::detect_main_branch(&source_repo).await;

                match strategy {
                    MergeStrategy::Merge => {
                        // Stash source repo if dirty
                        let src_status = tokio::process::Command::new("git")
                            .args(["status", "--porcelain"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let src_dirty = !String::from_utf8_lossy(&src_status.stdout)
                            .trim()
                            .is_empty();
                        if src_dirty {
                            tokio::process::Command::new("git")
                                .args(["stash", "push", "-m", "piki-multi-merge-temp"])
                                .current_dir(&source_repo)
                                .output()
                                .await?;
                        }

                        // Save current branch to restore later
                        let prev_branch = tokio::process::Command::new("git")
                            .args(["rev-parse", "--abbrev-ref", "HEAD"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let prev = String::from_utf8_lossy(&prev_branch.stdout)
                            .trim()
                            .to_string();

                        // Checkout main
                        let checkout = tokio::process::Command::new("git")
                            .args(["checkout", &main_branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        if !checkout.status.success() {
                            let stderr = String::from_utf8_lossy(&checkout.stderr);
                            app.status_message = Some(format!(
                                "Checkout {} failed: {}",
                                main_branch,
                                stderr.trim()
                            ));
                            if src_dirty {
                                let _ = tokio::process::Command::new("git")
                                    .args(["stash", "pop"])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                            }
                            return Ok(());
                        }

                        // Merge
                        let merge = tokio::process::Command::new("git")
                            .args(["merge", &branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;

                        let mut has_conflicts = false;
                        if merge.status.success() {
                            let stdout = String::from_utf8_lossy(&merge.stdout);
                            let first = stdout.lines().next().unwrap_or("Merged");
                            app.status_message = Some(format!(
                                "✓ Merged '{}' into {}: {}",
                                branch, main_branch, first
                            ));
                        } else {
                            // Check for conflict markers in git status
                            let conflict_check = tokio::process::Command::new("git")
                                .args(["status", "--porcelain=v1"])
                                .current_dir(&source_repo)
                                .output()
                                .await?;
                            let conflict_stdout =
                                String::from_utf8_lossy(&conflict_check.stdout);
                            let conflict_files: Vec<ConflictFile> =
                                piki_core::git::parse_porcelain_status(&conflict_stdout)
                                    .into_iter()
                                    .filter(|f| {
                                        matches!(f.status, piki_core::FileStatus::Conflicted)
                                    })
                                    .map(|f| ConflictFile {
                                        path: f.path.clone(),
                                        status: format!("{:?}", f.status),
                                    })
                                    .collect();
                            if !conflict_files.is_empty() {
                                has_conflicts = true;
                                // Open conflict resolution overlay — stay on main branch
                                // so user can resolve conflicts in the source repo
                                app.active_dialog = Some(DialogState::ConflictResolution {
                                    files: conflict_files,
                                    selected: 0,
                                    repo_path: source_repo.clone(),
                                });
                                app.mode = AppMode::ConflictResolution;
                                app.set_toast(
                                    "Merge conflicts detected — resolve below",
                                    ToastLevel::Error,
                                );
                            } else {
                                // Some other merge error, abort
                                let _ = tokio::process::Command::new("git")
                                    .args(["merge", "--abort"])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                                let stderr = String::from_utf8_lossy(&merge.stderr);
                                app.status_message = Some(format!(
                                    "Merge failed: {}",
                                    stderr.trim()
                                ));
                            }
                        }

                        // Only restore branch and stash if no conflicts
                        // (conflicts need to be resolved on the current branch)
                        if !has_conflicts {
                            if prev != main_branch {
                                let _ = tokio::process::Command::new("git")
                                    .args(["checkout", &prev])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                            }
                            if src_dirty {
                                let _ = tokio::process::Command::new("git")
                                    .args(["stash", "pop"])
                                    .current_dir(&source_repo)
                                    .output()
                                    .await;
                            }
                        }
                    }
                    MergeStrategy::Rebase => {
                        // Rebase workspace branch onto main
                        let rebase = tokio::process::Command::new("git")
                            .args(["rebase", &main_branch])
                            .current_dir(&ws_path)
                            .output()
                            .await?;

                        if !rebase.status.success() {
                            // Check for conflict markers in git status
                            let conflict_check = tokio::process::Command::new("git")
                                .args(["status", "--porcelain=v1"])
                                .current_dir(&ws_path)
                                .output()
                                .await?;
                            let conflict_stdout =
                                String::from_utf8_lossy(&conflict_check.stdout);
                            let conflict_files: Vec<ConflictFile> =
                                piki_core::git::parse_porcelain_status(&conflict_stdout)
                                    .into_iter()
                                    .filter(|f| {
                                        matches!(f.status, piki_core::FileStatus::Conflicted)
                                    })
                                    .map(|f| ConflictFile {
                                        path: f.path.clone(),
                                        status: format!("{:?}", f.status),
                                    })
                                    .collect();
                            if !conflict_files.is_empty() {
                                app.active_dialog = Some(DialogState::ConflictResolution {
                                    files: conflict_files,
                                    selected: 0,
                                    repo_path: ws_path.clone(),
                                });
                                app.mode = AppMode::ConflictResolution;
                                app.set_toast(
                                    "Rebase conflicts detected — resolve below",
                                    ToastLevel::Error,
                                );
                            } else {
                                let stderr = String::from_utf8_lossy(&rebase.stderr);
                                let _ = tokio::process::Command::new("git")
                                    .args(["rebase", "--abort"])
                                    .current_dir(&ws_path)
                                    .output()
                                    .await;
                                app.status_message = Some(format!(
                                    "Rebase failed: {}",
                                    stderr.trim()
                                ));
                            }
                            return Ok(());
                        }

                        // Now fast-forward merge in source repo
                        let prev_branch = tokio::process::Command::new("git")
                            .args(["rev-parse", "--abbrev-ref", "HEAD"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let prev = String::from_utf8_lossy(&prev_branch.stdout)
                            .trim()
                            .to_string();

                        let _ = tokio::process::Command::new("git")
                            .args(["checkout", &main_branch])
                            .current_dir(&source_repo)
                            .output()
                            .await;

                        let ff = tokio::process::Command::new("git")
                            .args(["merge", "--ff-only", &branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;

                        if ff.status.success() {
                            app.status_message = Some(format!(
                                "✓ Rebased and merged '{}' into {}",
                                branch, main_branch
                            ));
                        } else {
                            let stderr = String::from_utf8_lossy(&ff.stderr);
                            app.status_message =
                                Some(format!("Fast-forward failed: {}", stderr.trim()));
                        }

                        // Restore previous branch
                        if prev != main_branch {
                            let _ = tokio::process::Command::new("git")
                                .args(["checkout", &prev])
                                .current_dir(&source_repo)
                                .output()
                                .await;
                        }
                    }
                }

                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    let _ = ws.refresh_changed_files().await;
                }
            }
        }
        Action::LoadPrReview => {
            let worktree_path = app
                .workspaces
                .get(app.active_workspace)
                .map(|ws| ws.path.clone());
            if let Some(worktree_path) = worktree_path {
                match piki_core::github::get_pr_for_branch(&worktree_path).await {
                    Ok(Some(pr_info)) => {
                        match piki_core::github::get_pr_files(&worktree_path).await {
                            Ok(files) => {
                                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                                    ws.code_review = Some(CodeReviewState::new(pr_info, files));
                                }
                                app.set_toast("PR loaded", ToastLevel::Success);
                            }
                            Err(e) => {
                                app.set_toast(
                                    format!("Failed to load PR files: {}", e),
                                    ToastLevel::Error,
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                        {
                            ws.close_tab(ws.active_tab);
                        }
                        app.set_toast("No open PR for this branch", ToastLevel::Error);
                    }
                    Err(e) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                        {
                            ws.close_tab(ws.active_tab);
                        }
                        app.set_toast(format!("gh error: {}", e), ToastLevel::Error);
                    }
                }
            }
        }
        Action::LoadPrFileDiff(file_idx) => {
            // Extract what we need before the async call
            let diff_data = app.workspaces.get_mut(app.active_workspace).and_then(|ws| {
                let cr = ws.code_review.as_mut()?;
                let file = cr.files.get(file_idx)?;
                let file_path = file.path.clone();
                if cr.file_diffs.contains_key(&file_path) {
                    return None; // Already cached
                }
                cr.loading = true;
                let base_ref = cr.pr_info.base_ref_name.clone();
                Some((ws.path.clone(), file_path, base_ref))
            });
            if let Some((worktree_path, file_path, base_ref)) = diff_data {
                match piki_core::github::get_pr_file_diff_raw(&worktree_path, &file_path, &base_ref)
                    .await
                {
                    Ok(parsed) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(ref mut cr) = ws.code_review
                        {
                            cr.file_diffs.insert(file_path, parsed);
                            cr.diff_scroll = 0;
                            cr.cursor_line = 0;
                            cr.loading = false;
                        }
                    }
                    Err(e) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(ref mut cr) = ws.code_review
                        {
                            cr.loading = false;
                        }
                        app.set_toast(format!("Diff error: {}", e), ToastLevel::Error);
                    }
                }
            }
        }
        Action::SubmitPrReview => {
            let submit_data = if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if let Some(cr) = ws.code_review.as_mut() {
                    let verdict = cr.draft.verdict;
                    let body = cr.draft.body.clone();
                    let comments = cr.draft.comments.clone();
                    let pr_number = cr.pr_info.number;
                    cr.show_submit = false;
                    Some((ws.info.path.clone(), verdict, body, comments, pr_number))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((worktree_path, verdict, body, comments, pr_number)) = submit_data {
                let result = if comments.is_empty() {
                    piki_core::github::submit_review(&worktree_path, verdict, &body).await
                } else {
                    piki_core::github::submit_review_with_comments(
                        &worktree_path,
                        pr_number,
                        verdict,
                        &body,
                        &comments,
                    )
                    .await
                };
                match result {
                    Ok(_) => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                            ws.code_review = None;
                            if ws
                                .current_tab()
                                .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                        }
                        app.mode = AppMode::Normal;
                        app.interacting = false;
                        app.active_dialog = None;
                        app.set_toast(
                            format!("Review submitted: {}", verdict.label()),
                            ToastLevel::Success,
                        );
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let user_msg = if msg.contains("Can not request changes on your own")
                            || msg.contains("Can not approve your own")
                        {
                            "Cannot approve/request-changes on your own PR — use Comment"
                                .to_string()
                        } else if msg.contains("Unprocessable Entity") {
                            format!("GitHub rejected: {}", msg)
                        } else {
                            format!("Submit failed: {}", msg)
                        };
                        // Show error inside the submit overlay so user can see it and retry
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(cr) = ws.code_review.as_mut()
                        {
                            cr.show_submit = true;
                            cr.submit_error = Some(user_msg);
                        }
                    }
                }
            }
        }
        Action::SpawnTab(provider) => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if provider == AIProvider::Kanban && ws.kanban_app.is_none() {
                    // Initialize Kanban app if it doesn't exist yet for this workspace
                    let kanban_path_opt = ws.kanban_path.clone();

                    let mut kanban_provider = if app.config.kanban.provider == "jira" {
                        Box::new(flow_core::provider_jira::JiraProvider::from_env())
                            as Box<dyn flow_core::provider::Provider>
                    } else {
                        let default_path = kanban_path_opt
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                app.config
                                    .kanban
                                    .path
                                    .clone()
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_else(|| {
                                        piki_core::xdg::home_dir()
                                            .join(".config/flow/boards/default")
                                    })
                            });

                        let expanded_path = if let Some(path_str) = default_path.to_str() {
                            if path_str.starts_with("~/") {
                                if let Ok(home) = std::env::var("HOME") {
                                    std::path::PathBuf::from(home)
                                        .join(path_str.strip_prefix("~/").unwrap())
                                } else {
                                    default_path
                                }
                            } else {
                                default_path
                            }
                        } else {
                            default_path
                        };

                        // Initialize if board.txt doesn't exist
                        let board_txt = expanded_path.join("board.txt");
                        if !board_txt.exists() {
                            if let Err(e) = std::fs::create_dir_all(&expanded_path) {
                                app.status_message =
                                    Some(format!("Failed to create kanban dir: {}", e));
                            } else {
                                let board_content = "col todo \"TO DO\"\ncol in_progress \"IN PROGRESS\"\ncol in_review \"IN REVIEW\"\ncol done \"DONE\"\n";
                                if let Err(e) = std::fs::write(&board_txt, board_content) {
                                    app.status_message =
                                        Some(format!("Failed to write board.txt: {}", e));
                                } else {
                                    for col in &["todo", "in_progress", "in_review", "done"] {
                                        let col_dir = expanded_path.join("cols").join(col);
                                        let _ = std::fs::create_dir_all(&col_dir);
                                        let _ = std::fs::write(col_dir.join("order.txt"), "");
                                    }
                                }
                            }
                        }

                        Box::new(flow_core::provider_local::LocalProvider::new(expanded_path))
                            as Box<dyn flow_core::provider::Provider>
                    };

                    let board = kanban_provider
                        .load_board()
                        .unwrap_or_else(|_e| flow_core::Board { columns: vec![] });
                    let mut kanban = flow_tui::App::new(board);
                    if kanban.board.columns.is_empty() {
                        kanban.banner =
                            Some("Load failed or empty board. Check board.txt.".to_string());
                    }
                    ws.kanban_app = Some(kanban);
                    ws.kanban_provider = Some(kanban_provider);
                }

                let idx = spawn_tab(ws, &provider, app.pty_rows, app.pty_cols, None, Some(&app.provider_manager)).await;
                ws.active_tab = idx;
                app.status_message = Some(format!("Opened {} tab", provider.label()));
            }

            // Code Review: check gh availability (lazy, cached) then load PR data
            if provider == AIProvider::CodeReview {
                // Lazy gh CLI check — run once, cache forever
                if app.gh_available.is_none() {
                    let gh_ok = tokio::process::Command::new("gh")
                        .arg("--version")
                        .output()
                        .await
                        .is_ok_and(|o| o.status.success());
                    let auth_ok = if gh_ok {
                        tokio::process::Command::new("gh")
                            .args(["auth", "status"])
                            .output()
                            .await
                            .is_ok_and(|o| o.status.success())
                    } else {
                        false
                    };
                    app.gh_available = Some(gh_ok && auth_ok);
                    if !gh_ok {
                        app.set_toast(
                            "gh CLI not found — install from https://cli.github.com/",
                            ToastLevel::Error,
                        );
                    } else if !auth_ok {
                        app.set_toast(
                            "gh not authenticated — run `gh auth login`",
                            ToastLevel::Error,
                        );
                    }
                }
                if app.gh_available != Some(true) {
                    // Remove the tab we just created
                    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                        && ws
                            .current_tab()
                            .is_some_and(|t| t.provider == AIProvider::CodeReview)
                    {
                        ws.close_tab(ws.active_tab);
                    }
                    return Ok(());
                }
                let worktree_path = app
                    .workspaces
                    .get(app.active_workspace)
                    .map(|ws| ws.info.path.clone());
                if let Some(worktree_path) = worktree_path {
                    match piki_core::github::get_pr_for_branch(&worktree_path).await {
                        Ok(Some(pr_info)) => {
                            match piki_core::github::get_pr_files(&worktree_path).await {
                                Ok(files) => {
                                    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                                        ws.code_review = Some(CodeReviewState::new(pr_info, files));
                                    }
                                    app.set_toast("PR loaded", ToastLevel::Success);
                                }
                                Err(e) => {
                                    app.set_toast(
                                        format!("Failed to load PR files: {}", e),
                                        ToastLevel::Error,
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                                && ws
                                    .current_tab()
                                    .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                            app.set_toast("No open PR for this branch", ToastLevel::Error);
                        }
                        Err(e) => {
                            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                                && ws
                                    .current_tab()
                                    .is_some_and(|t| t.provider == AIProvider::CodeReview)
                            {
                                ws.close_tab(ws.active_tab);
                            }
                            app.set_toast(format!("gh error: {}", e), ToastLevel::Error);
                        }
                    }
                }
            }
        }
        Action::OpenMarkdown(path) => match std::fs::read_to_string(&path) {
            Ok(content) => {
                let label = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "markdown".to_string());
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.add_markdown_tab(label.clone(), content, Some(&app.syntax));
                    app.status_message = Some(format!("Opened {}", label));
                }
            }
            Err(e) => {
                app.status_message = Some(format!("Failed to read file: {}", e));
            }
        },
        Action::OpenMdr(path) => {
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            )?;
            ratatui::restore();
            let status = std::process::Command::new("mdr").arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture,
                crossterm::event::EnableBracketedPaste,
                crossterm::event::PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            )?;
            match status {
                Ok(s) if s.success() => {
                    app.status_message = Some(format!("mdr: {}", path.display()));
                }
                Ok(s) => {
                    app.status_message = Some(format!("mdr exited with: {}", s));
                }
                Err(_) => {
                    app.status_message =
                        Some("mdr not found. Install: cargo install markdown-reader".to_string());
                }
            }
            if app.mode == AppMode::FuzzySearch {
                app.fuzzy = None;
                app.mode = AppMode::Normal;
            }
        }
        Action::SendApiRequest(text) => {
            // Parse the Hurl text (supports multiple requests)
            let parsed_requests = match piki_api_client::parse_hurl_multi(&text) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "API Explorer: failed to parse request");
                    app.set_toast(format!("Parse error: {}", e), ToastLevel::Error);
                    return Ok(());
                }
            };

            // Set loading state
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(tab) = ws.current_tab_mut()
                && let Some(ref mut api) = tab.api_state
            {
                api.loading = true;
                api.responses.clear();
                let slot = Arc::clone(&api.pending_responses);
                let storage = Arc::clone(&app.storage);
                let source_repo = ws.source_repo.to_string_lossy().to_string();

                tokio::spawn(async move {
                    let mut results = Vec::with_capacity(parsed_requests.len());

                    for parsed in parsed_requests {
                        let url = if !parsed.url.contains("://") {
                            format!("https://{}", parsed.url)
                        } else {
                            parsed.url.clone()
                        };

                        let method_str = match parsed.method {
                            piki_api_client::Method::Get => "GET",
                            piki_api_client::Method::Post => "POST",
                            piki_api_client::Method::Put => "PUT",
                            piki_api_client::Method::Delete => "DELETE",
                            piki_api_client::Method::Patch => "PATCH",
                        };

                        let mut request = match parsed.method {
                            piki_api_client::Method::Get => piki_api_client::ApiRequest::get(""),
                            piki_api_client::Method::Post => piki_api_client::ApiRequest::post(""),
                            piki_api_client::Method::Put => piki_api_client::ApiRequest::put(""),
                            piki_api_client::Method::Delete => {
                                piki_api_client::ApiRequest::delete("")
                            }
                            piki_api_client::Method::Patch => {
                                piki_api_client::ApiRequest::patch("")
                            }
                        };
                        request.body = parsed.body.clone();
                        for (k, v) in &parsed.headers {
                            request.headers.insert(k.clone(), v.clone());
                        }

                        // Build request text for history
                        let request_text = {
                            let mut text = format!("{} {}", method_str, url);
                            for (k, v) in &parsed.headers {
                                text.push_str(&format!("\n{}: {}", k, v));
                            }
                            if let Some(ref body) = parsed.body {
                                let body_str = String::from_utf8_lossy(body);
                                text.push_str(&format!("\n\n{}", body_str));
                            }
                            text
                        };

                        let config = piki_api_client::ClientConfig::new(&url);
                        let client = match piki_api_client::HttpClient::new(config) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::error!(error = %e, "API Explorer: failed to create HTTP client");
                                results.push(app::ApiResponseDisplay {
                                    status: 0,
                                    elapsed_ms: 0,
                                    body: format!("Client error: {}", e),
                                    headers: String::new(),
                                });
                                continue;
                            }
                        };

                        let start = std::time::Instant::now();
                        let result =
                            <piki_api_client::HttpClient as piki_api_client::ApiClient>::execute(
                                &client, request,
                            )
                            .await;
                        let elapsed = start.elapsed().as_millis();

                        let display = match result {
                            Ok(resp) => {
                                let body_text = String::from_utf8_lossy(&resp.body).to_string();
                                let body = if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&body_text)
                                {
                                    serde_json::to_string_pretty(&json).unwrap_or(body_text)
                                } else {
                                    body_text
                                };
                                let headers = resp
                                    .headers
                                    .iter()
                                    .map(|(k, v)| format!("{}: {}", k, v))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                tracing::info!(status = resp.status, elapsed_ms = elapsed, url = %url, "API Explorer: request completed");
                                app::ApiResponseDisplay {
                                    status: resp.status,
                                    elapsed_ms: elapsed,
                                    body,
                                    headers,
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, url = %url, "API Explorer: request failed");
                                app::ApiResponseDisplay {
                                    status: 0,
                                    elapsed_ms: elapsed,
                                    body: format!("Error: {}", e),
                                    headers: String::new(),
                                }
                            }
                        };

                        // Persist to API history storage if available
                        if let Some(ref api_storage) = storage.api_history {
                            let entry = piki_core::storage::ApiHistoryEntry {
                                id: None,
                                source_repo: source_repo.clone(),
                                created_at: String::new(),
                                request_text,
                                method: method_str.to_string(),
                                url: url.clone(),
                                status: display.status,
                                elapsed_ms: display.elapsed_ms,
                                response_body: display.body.clone(),
                                response_headers: display.headers.clone(),
                            };
                            if let Err(e) = api_storage.save_api_entry(&entry) {
                                tracing::warn!(error = %e, "Failed to persist API history entry");
                            }
                        }

                        results.push(display);
                    }

                    let mut guard = slot.lock();
                    *guard = Some(results);
                });
            }
        }
        Action::Undo => {
            if let Some(entry) = app.undo_stack.pop_back() {
                // Validate workspace still exists
                if let Some(ws) = app.workspaces.get_mut(entry.workspace_idx) {
                    let worktree = ws.path.clone();
                    let file_path = entry.file_path.clone();
                    let status_tx = app.status_tx.clone();
                    ws.dirty = true;
                    ws.last_refresh = None;
                    match entry.action {
                        app::UndoAction::Stage => {
                            // Undo a stage = unstage
                            tokio::spawn(async move {
                                let output = tokio::process::Command::new("git")
                                    .args(["reset", "HEAD", &file_path])
                                    .current_dir(&worktree)
                                    .output()
                                    .await;
                                match output {
                                    Ok(o) if o.status.success() => {
                                        let _ =
                                            status_tx.send(format!("✓ Undo stage: {}", file_path));
                                    }
                                    Ok(o) => {
                                        let stderr = String::from_utf8_lossy(&o.stderr);
                                        let _ = status_tx
                                            .send(format!("Undo failed: {}", stderr.trim()));
                                    }
                                    Err(e) => {
                                        let _ = status_tx.send(format!("Undo error: {}", e));
                                    }
                                }
                            });
                        }
                        app::UndoAction::Unstage => {
                            // Undo an unstage = re-stage
                            tokio::spawn(async move {
                                let output = tokio::process::Command::new("git")
                                    .args(["add", &file_path])
                                    .current_dir(&worktree)
                                    .output()
                                    .await;
                                match output {
                                    Ok(o) if o.status.success() => {
                                        let _ = status_tx
                                            .send(format!("✓ Undo unstage: {}", file_path));
                                    }
                                    Ok(o) => {
                                        let stderr = String::from_utf8_lossy(&o.stderr);
                                        let _ = status_tx
                                            .send(format!("Undo failed: {}", stderr.trim()));
                                    }
                                    Err(e) => {
                                        let _ = status_tx.send(format!("Undo error: {}", e));
                                    }
                                }
                            });
                        }
                    }
                } else {
                    app.set_toast(
                        "Undo failed: workspace no longer exists".to_string(),
                        ToastLevel::Error,
                    );
                }
            } else {
                app.set_toast("Nothing to undo".to_string(), ToastLevel::Info);
            }
        }
        Action::LoadGitLog => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let output = tokio::process::Command::new("git")
                .args(["log", "--oneline", "--graph", "--decorate", "--all", "-50"])
                .current_dir(&worktree)
                .output()
                .await?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<GitLogEntry> = stdout
                .lines()
                .map(|line| {
                    // Extract SHA: skip graph chars (*, |, /, \, space), then look for hex
                    let trimmed = line.trim_start_matches(|c: char| "*|/\\ ".contains(c));
                    let sha = trimmed
                        .split_whitespace()
                        .next()
                        .filter(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit()))
                        .map(String::from);
                    GitLogEntry {
                        raw_line: line.to_string(),
                        sha,
                    }
                })
                .collect();
            app.active_dialog = Some(DialogState::GitLog {
                lines,
                selected: 0,
                scroll: 0,
            });
            app.mode = AppMode::GitLog;
        }
        Action::ViewCommitDiff(sha) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            // Compute diff width from terminal size (matches diff overlay: 90% width minus borders)
            let term_size = terminal.size()?;
            let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
            let width = if overlay_inner_width > 10 {
                overlay_inner_width
            } else {
                120
            };
            // Try delta first, fall back to plain git show
            let ansi_bytes = match run_commit_diff_with_delta(&worktree, &sha, width).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    // Fallback: plain git show with color
                    let output = tokio::process::Command::new("git")
                        .args(["show", "--color=always", "--stat", "-p", &sha])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    output.stdout
                }
            };
            use ansi_to_tui::IntoText;
            match ansi_bytes.into_text() {
                Ok(text) => {
                    let text = Arc::new(text);
                    app.diff_content = Some(text);
                    app.diff_file_path = Some(format!("commit {}", sha));
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                    app.interacting = true;
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to parse commit diff: {}", e));
                }
            }
        }
        Action::GitStashList => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let entries = parse_stash_list(&worktree).await;
            app.active_dialog = Some(DialogState::GitStash {
                entries,
                selected: 0,
                scroll: 0,
                input_mode: false,
                input_buffer: String::new(),
                input_cursor: 0,
            });
            app.mode = AppMode::GitStash;
        }
        Action::GitStashSave(message) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let output = tokio::process::Command::new("git")
                .args(["stash", "push", "-m", &message])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Stashed: {}", message), ToastLevel::Success);
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    ws.last_refresh = None;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(
                    format!("Stash failed: {}", stderr.trim()),
                    ToastLevel::Error,
                );
            }
            // Refresh stash list in the overlay
            let entries = parse_stash_list(&worktree).await;
            if let Some(DialogState::GitStash {
                entries: ref mut e,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                *e = entries;
                if *selected >= e.len() {
                    *selected = e.len().saturating_sub(1);
                }
            }
        }
        Action::GitStashPop(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let output = tokio::process::Command::new("git")
                .args(["stash", "pop", &stash_ref])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Popped {}", stash_ref), ToastLevel::Success);
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    ws.last_refresh = None;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(format!("Pop failed: {}", stderr.trim()), ToastLevel::Error);
            }
            let entries = parse_stash_list(&worktree).await;
            if let Some(DialogState::GitStash {
                entries: ref mut e,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                *e = entries;
                if *selected >= e.len() {
                    *selected = e.len().saturating_sub(1);
                }
            }
        }
        Action::GitStashApply(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let output = tokio::process::Command::new("git")
                .args(["stash", "apply", &stash_ref])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Applied {}", stash_ref), ToastLevel::Success);
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.dirty = true;
                    ws.last_refresh = None;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(
                    format!("Apply failed: {}", stderr.trim()),
                    ToastLevel::Error,
                );
            }
        }
        Action::GitStashDrop(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let output = tokio::process::Command::new("git")
                .args(["stash", "drop", &stash_ref])
                .current_dir(&worktree)
                .output()
                .await?;
            if output.status.success() {
                app.set_toast(format!("Dropped {}", stash_ref), ToastLevel::Success);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                app.set_toast(format!("Drop failed: {}", stderr.trim()), ToastLevel::Error);
            }
            let entries = parse_stash_list(&worktree).await;
            if let Some(DialogState::GitStash {
                entries: ref mut e,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                *e = entries;
                if *selected >= e.len() {
                    *selected = e.len().saturating_sub(1);
                }
            }
        }
        Action::GitStashShow(idx) => {
            let worktree = match app.current_workspace() {
                Some(ws) => ws.path.clone(),
                None => return Ok(()),
            };
            let stash_ref = format!("stash@{{{}}}", idx);
            let term_size = terminal.size()?;
            let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
            let width = if overlay_inner_width > 10 {
                overlay_inner_width
            } else {
                120
            };
            // Try delta first, fall back to plain git stash show
            let ansi_bytes = match run_stash_diff_with_delta(&worktree, &stash_ref, width).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    let output = tokio::process::Command::new("git")
                        .args(["stash", "show", "-p", "--color=always", &stash_ref])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    output.stdout
                }
            };
            use ansi_to_tui::IntoText;
            match ansi_bytes.into_text() {
                Ok(text) => {
                    let text = Arc::new(text);
                    app.diff_content = Some(text);
                    app.diff_file_path = Some(format!("stash: {}", stash_ref));
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                    app.interacting = true;
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to parse stash diff: {}", e));
                }
            }
        }
        Action::ViewConflictDiff(file) => {
            // Use repo_path from the conflict dialog (conflicts may be in source_repo, not worktree)
            let worktree = match &app.active_dialog {
                Some(DialogState::ConflictResolution { repo_path, .. }) => repo_path.clone(),
                _ => match app.current_workspace() {
                    Some(ws) => ws.path.clone(),
                    None => return Ok(()),
                },
            };
            let term_size = terminal.size()?;
            let overlay_inner_width = (term_size.width * 90 / 100).saturating_sub(2);
            let width = if overlay_inner_width > 10 {
                overlay_inner_width
            } else {
                120
            };
            // Try delta first, fall back to plain git diff
            let ansi_bytes =
                match run_conflict_diff_with_delta(&worktree, &file, width).await {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        let output = tokio::process::Command::new("git")
                            .args(["diff", "--color=always", "--", &file])
                            .current_dir(&worktree)
                            .output()
                            .await?;
                        output.stdout
                    }
                };
            use ansi_to_tui::IntoText;
            match ansi_bytes.into_text() {
                Ok(text) => {
                    let text = Arc::new(text);
                    app.diff_content = Some(text);
                    app.diff_file_path = Some(format!("conflict: {}", file));
                    app.diff_scroll = 0;
                    app.mode = AppMode::Diff;
                    app.active_pane = ActivePane::MainPanel;
                    app.interacting = true;
                }
                Err(e) => {
                    app.status_message = Some(format!("Failed to parse diff: {}", e));
                }
            }
        }
        Action::ResolveConflict { file, strategy } => {
            // Use repo_path from the conflict dialog (conflicts may be in source_repo)
            let worktree = match &app.active_dialog {
                Some(DialogState::ConflictResolution { repo_path, .. }) => repo_path.clone(),
                _ => match app.current_workspace() {
                    Some(ws) => ws.path.clone(),
                    None => return Ok(()),
                },
            };
            match strategy {
                ConflictStrategy::Ours => {
                    tokio::process::Command::new("git")
                        .args(["checkout", "--ours", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    tokio::process::Command::new("git")
                        .args(["add", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                }
                ConflictStrategy::Theirs => {
                    tokio::process::Command::new("git")
                        .args(["checkout", "--theirs", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    tokio::process::Command::new("git")
                        .args(["add", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                }
                ConflictStrategy::MarkResolved => {
                    tokio::process::Command::new("git")
                        .args(["add", &file])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                }
            }
            app.set_toast(format!("Resolved: {}", file), ToastLevel::Success);
            // Remove from conflict list
            if let Some(DialogState::ConflictResolution {
                ref mut files,
                ref mut selected,
                ..
            }) = app.active_dialog
            {
                files.retain(|f| f.path != file);
                if files.is_empty() {
                    // Auto-commit to complete the merge
                    let merge_commit = tokio::process::Command::new("git")
                        .args(["commit", "--no-edit"])
                        .current_dir(&worktree)
                        .output()
                        .await;
                    let msg = match merge_commit {
                        Ok(out) if out.status.success() => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let first = stdout.lines().next().unwrap_or("Merge committed");
                            format!("✓ All conflicts resolved and merge committed: {}", first)
                        }
                        _ => "All conflicts resolved — commit to complete merge".to_string(),
                    };
                    app.active_dialog = None;
                    app.mode = AppMode::Normal;
                    app.interacting = false;
                    app.diff_content = None;
                    app.diff_file_path = None;
                    app.set_toast(msg, ToastLevel::Success);
                } else if *selected >= files.len() {
                    *selected = files.len() - 1;
                }
            }
            // Refresh workspace
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                ws.dirty = true;
                ws.last_refresh = None;
            }
        }
        Action::AbortMerge => {
            let worktree = match &app.active_dialog {
                Some(DialogState::ConflictResolution { repo_path, .. }) => repo_path.clone(),
                _ => match app.current_workspace() {
                    Some(ws) => ws.path.clone(),
                    None => return Ok(()),
                },
            };
            // Try merge --abort first, fall back to rebase --abort
            let merge_result = tokio::process::Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(&worktree)
                .output()
                .await?;
            if !merge_result.status.success() {
                tokio::process::Command::new("git")
                    .args(["rebase", "--abort"])
                    .current_dir(&worktree)
                    .output()
                    .await?;
            }
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.interacting = false;
            app.diff_content = None;
            app.diff_file_path = None;
            app.set_toast("Merge aborted", ToastLevel::Info);
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                ws.dirty = true;
                ws.last_refresh = None;
            }
        }
        Action::DetectConflicts => {
            let (ws_path, source_repo) = match app.current_workspace() {
                Some(ws) => (ws.path.clone(), ws.source_repo.clone()),
                None => return Ok(()),
            };
            // Check both worktree and source_repo for conflicts
            for repo_path in [&ws_path, &source_repo] {
                let output = tokio::process::Command::new("git")
                    .args(["status", "--porcelain=v1"])
                    .current_dir(repo_path)
                    .output()
                    .await?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let conflicts: Vec<ConflictFile> =
                    piki_core::git::parse_porcelain_status(&stdout)
                        .into_iter()
                        .filter(|f| matches!(f.status, piki_core::FileStatus::Conflicted))
                        .map(|f| ConflictFile {
                            path: f.path.clone(),
                            status: format!("{:?}", f.status),
                        })
                        .collect();
                if !conflicts.is_empty() {
                    app.open_conflict_resolution_with(conflicts, repo_path.clone());
                    return Ok(());
                }
            }
            app.set_toast("No conflicts detected", ToastLevel::Info);
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
                    spawn_tab(ws, &provider, app.pty_rows, app.pty_cols, Some(&task_prompt), Some(&app.provider_manager)).await;
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

                // Scan provider agent directories for .md files
                // Start with built-in providers, then add custom providers
                let mut provider_dirs: Vec<(String, String)> = vec![
                    (".claude/agents".into(), "Claude Code".into()),
                    (".gemini/agents".into(), "Gemini".into()),
                    (".opencode/agents".into(), "OpenCode".into()),
                    (".kilo/agents".into(), "Kilo".into()),
                    (".codex/agents".into(), "Codex".into()),
                ];
                for config in app.provider_manager.all() {
                    if let Some(ref agent_dir) = config.agent_dir {
                        let already = provider_dirs.iter().any(|(d, _)| d == agent_dir);
                        if !already {
                            provider_dirs.push((agent_dir.clone(), config.name.clone()));
                        }
                    }
                }

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

        Action::ChatSendMessage => {
            let input = std::mem::take(&mut app.chat_panel.input);
            let input = input.trim().to_string();
            if input.is_empty() || app.chat_panel.streaming || app.chat_panel.config.model.is_empty()
            {
                if app.chat_panel.config.model.is_empty() {
                    app.set_toast("No model selected. Press Tab to pick one.", ToastLevel::Error);
                }
                return Ok(());
            }

            // Append user message
            app.chat_panel.messages.push(piki_core::chat::ChatMessage {
                role: piki_core::chat::ChatRole::User,
                content: input,
            });
            app.chat_panel.input_cursor = 0;
            app.chat_panel.streaming = true;
            app.chat_panel.current_response.clear();

            // Build Ollama messages
            let mut ollama_msgs: Vec<piki_api_client::OllamaMessage> = Vec::new();
            if let Some(ref sys) = app.chat_panel.config.system_prompt
                && !sys.is_empty()
            {
                ollama_msgs.push(piki_api_client::OllamaMessage {
                    role: "system".to_string(),
                    content: sys.clone(),
                });
            }
            for msg in &app.chat_panel.messages {
                ollama_msgs.push(piki_api_client::OllamaMessage {
                    role: match msg.role {
                        piki_core::chat::ChatRole::System => "system",
                        piki_core::chat::ChatRole::User => "user",
                        piki_core::chat::ChatRole::Assistant => "assistant",
                    }
                    .to_string(),
                    content: msg.content.clone(),
                });
            }

            tracing::info!(
                model = %app.chat_panel.config.model,
                base_url = %app.chat_panel.config.base_url,
                msg_count = ollama_msgs.len(),
                "TUI: sending chat message"
            );

            let client =
                piki_api_client::OllamaClient::new(&app.chat_panel.config.base_url);
            let model = app.chat_panel.config.model.clone();
            let tx = app.chat_token_tx.clone();

            tokio::spawn(async move {
                if let Err(e) = client.chat_stream(&model, &ollama_msgs, tx).await {
                    tracing::error!(error = %e, "Ollama chat_stream error");
                }
            });
        }

        Action::ChatLoadModels => {
            tracing::debug!(base_url = %app.chat_panel.config.base_url, "TUI: loading Ollama models");
            let base_url = app.chat_panel.config.base_url.clone();
            let status_tx = app.status_tx.clone();
            let chat_tx = app.chat_token_tx.clone();

            tokio::spawn(async move {
                let client = piki_api_client::OllamaClient::new(&base_url);
                match client.list_models().await {
                    Ok(models) => {
                        let names: Vec<String> = models.into_iter().map(|m| m.name).collect();
                        // Use a special "models loaded" event via the chat channel
                        // We pack model names as a Done event with a special prefix
                        let payload = format!("__MODELS__{}", names.join("\n"));
                        let _ = chat_tx.send(piki_api_client::ChatStreamEvent::Done(payload));
                    }
                    Err(e) => {
                        let msg = format!("{e}. Is Ollama running? (ollama serve)");
                        let _ = status_tx.send(msg);
                    }
                }
            });
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
    let dir = match provider.builtin_agent_dir() {
        Some(d) => d.to_string(),
        None => {
            // Check custom provider for agent_dir
            if let AIProvider::Custom(name) = provider
                && let Some(mgr) = provider_manager
                && let Some(config) = mgr.get(name)
                && let Some(agent_dir) = &config.agent_dir
            {
                agent_dir.clone()
            } else {
                return Ok(());
            }
        }
    };
    let agent_dir = worktree_path.join(dir);
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join(filename), role)?;
    Ok(())
}

/// Run `git show <sha>` piped through delta for formatted output.
async fn run_commit_diff_with_delta(
    worktree: &std::path::Path,
    sha: &str,
    width: u16,
) -> anyhow::Result<Vec<u8>> {
    let git_show = tokio::process::Command::new("git")
        .args(["show", "--color=always", sha])
        .current_dir(worktree)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let git_stdout = git_show
        .stdout
        .ok_or_else(|| anyhow::anyhow!("failed to capture git show stdout"))?;

    let git_stdout_std: std::process::Stdio = git_stdout.try_into()?;

    let delta_output = tokio::process::Command::new("delta")
        .args([
            "--side-by-side",
            &format!("--width={}", width),
            "--paging=never",
            "--true-color=always",
            "--line-fill-method=ansi",
        ])
        .stdin(git_stdout_std)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await?;

    if !delta_output.status.success() {
        anyhow::bail!("delta exited with non-zero status");
    }

    Ok(delta_output.stdout)
}

/// Run `git stash show -p <ref>` piped through delta for formatted output.
async fn run_stash_diff_with_delta(
    worktree: &std::path::Path,
    stash_ref: &str,
    width: u16,
) -> anyhow::Result<Vec<u8>> {
    let git_stash = tokio::process::Command::new("git")
        .args(["stash", "show", "-p", "--color=always", stash_ref])
        .current_dir(worktree)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let git_stdout = git_stash
        .stdout
        .ok_or_else(|| anyhow::anyhow!("failed to capture git stash show stdout"))?;

    let git_stdout_std: std::process::Stdio = git_stdout.try_into()?;

    let delta_output = tokio::process::Command::new("delta")
        .args([
            "--side-by-side",
            &format!("--width={}", width),
            "--paging=never",
            "--true-color=always",
            "--line-fill-method=ansi",
        ])
        .stdin(git_stdout_std)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await?;

    if !delta_output.status.success() {
        anyhow::bail!("delta exited with non-zero status");
    }

    Ok(delta_output.stdout)
}

/// Run `git diff -- <file>` piped through delta for conflict viewing.
async fn run_conflict_diff_with_delta(
    worktree: &std::path::Path,
    file: &str,
    width: u16,
) -> anyhow::Result<Vec<u8>> {
    let git_diff = tokio::process::Command::new("git")
        .args(["diff", "--color=always", "--", file])
        .current_dir(worktree)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let git_stdout = git_diff
        .stdout
        .ok_or_else(|| anyhow::anyhow!("failed to capture git diff stdout"))?;

    let git_stdout_std: std::process::Stdio = git_stdout.try_into()?;

    let delta_output = tokio::process::Command::new("delta")
        .args([
            "--side-by-side",
            &format!("--width={}", width),
            "--paging=never",
            "--true-color=always",
            "--line-fill-method=ansi",
        ])
        .stdin(git_stdout_std)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await?;

    if !delta_output.status.success() {
        anyhow::bail!("delta exited with non-zero status");
    }

    Ok(delta_output.stdout)
}

/// Parse `git stash list` output into (ref, message) pairs.
async fn parse_stash_list(worktree: &std::path::Path) -> Vec<(String, String)> {
    let output = tokio::process::Command::new("git")
        .args(["stash", "list"])
        .current_dir(worktree)
        .output()
        .await
        .ok();
    match output {
        Some(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .filter_map(|line| {
                    // Format: "stash@{0}: On main: my message" or "stash@{0}: WIP on main: ..."
                    let colon_pos = line.find(':')?;
                    let stash_ref = line[..colon_pos].trim().to_string();
                    let message = line[colon_pos + 1..].trim().to_string();
                    Some((stash_ref, message))
                })
                .collect()
        }
        _ => Vec::new(),
    }
}
