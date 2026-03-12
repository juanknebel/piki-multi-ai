use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ratatui::DefaultTerminal;

use crate::app::{self, ActivePane, App, AppMode, ToastLevel};
use crate::helpers::{spawn_initial_shell, spawn_tab};
use piki_core::workspace::config as ws_config;
use piki_core::workspace::{FileWatcher, WorkspaceManager};
use piki_core::{AIProvider, MergeStrategy};

/// Async actions triggered by key events
pub(crate) enum Action {
    CreateWorkspace(String, String, String, Option<String>, PathBuf),
    EditWorkspace(usize, Option<String>, String),
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
}

pub(crate) async fn execute_action(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(name, description, prompt, kanban_path, dir) => {
            match manager
                .create(&name, &description, &prompt, kanban_path, &dir)
                .await
            {
                Ok(info) => {
                    app.workspaces.push(app::Workspace::from_info(info));
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    // Spawn initial Shell tab
                    spawn_initial_shell(&mut app.workspaces[new_idx], app.pty_rows, app.pty_cols)
                        .await;

                    // Auto-send prompt to active tab PTY if non-empty
                    if !prompt.is_empty() {
                        let ws = &mut app.workspaces[new_idx];
                        if let Some(tab) = ws.current_tab_mut()
                            && let Some(ref mut pty) = tab.pty_session {
                                // Small delay to let the PTY initialize
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                let prompt_with_newline = format!("{}\n", prompt);
                                let _ = pty.write(prompt_with_newline.as_bytes());
                            }
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
                        tokio::spawn(async move {
                            let _ = ws_config::save(&source, &infos);
                        });
                    }
                }
                Err(e) => {
                    app.status_message = Some(format!("Error: {}", e));
                }
            }
        }
        Action::EditWorkspace(idx, kanban_path, prompt) => {
            if let Some(ws) = app.workspaces.get_mut(idx) {
                if ws.kanban_path != kanban_path {
                    ws.kanban_app = None;
                    ws.kanban_provider = None;
                }
                ws.kanban_path = kanban_path;
                ws.prompt = prompt;
                {
                    let source = ws.source_repo.clone();
                    let infos: Vec<_> = app.workspaces.iter().map(|w| w.info.clone()).collect();
                    tokio::spawn(async move {
                        let _ = ws_config::save(&source, &infos);
                    });
                }
                app.set_toast("Workspace updated", ToastLevel::Success);
            }
        }
        Action::DeleteWorkspace(idx) => {
            if idx < app.workspaces.len() {
                // Kill all PTY sessions before removing
                for tab in &mut app.workspaces[idx].tabs {
                    if let Some(ref mut pty) = tab.pty_session {
                        let _ = pty.kill();
                    }
                }
                // Drop watcher (stops watching)
                app.workspaces[idx].watcher = None;

                let name = app.workspaces[idx].name.clone();
                let source_repo = app.workspaces[idx].source_repo.clone();

                match manager.remove(&name, &source_repo).await {
                    Ok(()) => {
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
                            let infos: Vec<_> =
                                app.workspaces.iter().map(|w| w.info.clone()).collect();
                            tokio::spawn(async move {
                                let _ = ws_config::save(&source, &infos);
                            });
                        }
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Error: {}", e));
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
                    tokio::spawn(async move {
                        let _ = ws_config::save(&source, &infos);
                    });
                }
            }
        }
        Action::OpenEditor(path) => {
            // Suspend TUI, open $EDITOR, restore TUI
            crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture)?;
            ratatui::restore();
            let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor_cmd).arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture)?;
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
                && let Some(file) = ws.changed_files.get(file_idx) {
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
                && let Some(file) = ws.changed_files.get(file_idx) {
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
                && let Some(file) = ws.changed_files.get(file_idx) {
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
            crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture)?;
            ratatui::restore();
            let status = std::process::Command::new("mdr").arg(&path).status();
            *terminal = ratatui::init();
            crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture)?;
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
                                        let _ = status_tx
                                            .send(format!("✓ Undo stage: {}", file_path));
                                    }
                                    Ok(o) => {
                                        let stderr = String::from_utf8_lossy(&o.stderr);
                                        let _ = status_tx.send(format!(
                                            "Undo failed: {}",
                                            stderr.trim()
                                        ));
                                    }
                                    Err(e) => {
                                        let _ =
                                            status_tx.send(format!("Undo error: {}", e));
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
                                        let _ = status_tx.send(format!(
                                            "Undo failed: {}",
                                            stderr.trim()
                                        ));
                                    }
                                    Err(e) => {
                                        let _ =
                                            status_tx.send(format!("Undo error: {}", e));
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
