use std::path::PathBuf;
use std::sync::Arc;

use ratatui::DefaultTerminal;

use crate::app::{self, ActivePane, App, AppMode, ToastLevel};
use crate::code_review::CodeReviewState;
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
    DeleteWorkspace(usize),
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
        Action::DeleteWorkspace(idx) => {
            if idx < app.workspaces.len() {
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
                crossterm::event::DisableMouseCapture
            )?;
            ratatui::restore();
            let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor_cmd).arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture,
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
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let source_repo = ws.source_repo.clone();
                let branch = ws.branch.clone();

                // Check workspace has no uncommitted changes
                let status_output = tokio::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&ws.path)
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

                        if merge.status.success() {
                            let stdout = String::from_utf8_lossy(&merge.stdout);
                            let first = stdout.lines().next().unwrap_or("Merged");
                            app.status_message = Some(format!(
                                "✓ Merged '{}' into {}: {}",
                                branch, main_branch, first
                            ));
                        } else {
                            let stderr = String::from_utf8_lossy(&merge.stderr);
                            // Abort the failed merge to leave repo clean
                            let _ = tokio::process::Command::new("git")
                                .args(["merge", "--abort"])
                                .current_dir(&source_repo)
                                .output()
                                .await;
                            app.status_message = Some(format!(
                                "Merge conflict: {} — resolve in Shell tab",
                                stderr.trim()
                            ));
                        }

                        // Restore previous branch
                        if prev != main_branch {
                            let _ = tokio::process::Command::new("git")
                                .args(["checkout", &prev])
                                .current_dir(&source_repo)
                                .output()
                                .await;
                        }

                        // Restore stash if we stashed
                        if src_dirty {
                            let _ = tokio::process::Command::new("git")
                                .args(["stash", "pop"])
                                .current_dir(&source_repo)
                                .output()
                                .await;
                        }
                    }
                    MergeStrategy::Rebase => {
                        // Rebase workspace branch onto main
                        let rebase = tokio::process::Command::new("git")
                            .args(["rebase", &main_branch])
                            .current_dir(&ws.path)
                            .output()
                            .await?;

                        if !rebase.status.success() {
                            let stderr = String::from_utf8_lossy(&rebase.stderr);
                            // Abort rebase on conflict
                            let _ = tokio::process::Command::new("git")
                                .args(["rebase", "--abort"])
                                .current_dir(&ws.path)
                                .output()
                                .await;
                            app.status_message = Some(format!(
                                "Rebase conflict: {} — resolve in Shell tab",
                                stderr.trim()
                            ));
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

                ws.dirty = true;
                let _ = ws.refresh_changed_files().await;
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
                        Box::new(flow::provider_jira::JiraProvider::from_env())
                            as Box<dyn flow::provider::Provider>
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
                                        dirs::home_dir()
                                            .unwrap_or_else(|| std::path::PathBuf::from("."))
                                            .join(".config/flow/boards/default")
                                    })
                            });

                        let expanded_path = if let Some(path_str) = default_path.to_str() {
                            if path_str.starts_with("~/") {
                                if let Some(home) = dirs::home_dir() {
                                    home.join(path_str.strip_prefix("~/").unwrap())
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

                        Box::new(flow::provider_local::LocalProvider::new(expanded_path))
                            as Box<dyn flow::provider::Provider>
                    };

                    let board = kanban_provider
                        .load_board()
                        .unwrap_or_else(|_e| flow::Board { columns: vec![] });
                    let mut kanban = flow::App::new(board);
                    if kanban.board.columns.is_empty() {
                        kanban.banner =
                            Some("Load failed or empty board. Check board.txt.".to_string());
                    }
                    ws.kanban_app = Some(kanban);
                    ws.kanban_provider = Some(kanban_provider);
                }

                let idx = spawn_tab(ws, provider, app.pty_rows, app.pty_cols).await;
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
                    ws.add_markdown_tab(label.clone(), content);
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
                crossterm::event::DisableMouseCapture
            )?;
            ratatui::restore();
            let status = std::process::Command::new("mdr").arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture,
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
    }
    Ok(())
}
