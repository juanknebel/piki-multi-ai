mod app;
mod clipboard;
mod config;
mod diff;
mod pty;
mod theme;
mod ui;
mod sysinfo;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use app::{AIProvider, ActivePane, App, AppMode, DialogField, MergeStrategy};
use clap::{Parser, Subcommand};
use pty::PtySession;
use workspace::{FileWatcher, WorkspaceManager, config as ws_config};

#[derive(Parser)]
#[command(name = "piki-multi-ai")]
#[command(version, about = "Terminal UI for orchestrating multiple AI assistants in parallel", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generates the default configuration file to stdout
    GenerateConfig,
    /// Shows version and author information (same as About in-app)
    Version,
}

const TICK_RATE: Duration = Duration::from_millis(50);
const DEBOUNCE: Duration = Duration::from_millis(500);
const PERIODIC_REFRESH: Duration = Duration::from_secs(3);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        match command {
            Commands::GenerateConfig => {
                println!("{}", config::Config::generate_default_toml());
                return Ok(());
            }
            Commands::Version => {
                let version = env!("CARGO_PKG_VERSION");
                println!("");
                println!("██████╗ ██╗██╗  ██╗██╗");
                println!("██╔══██╗██║██║ ██╔╝██║");
                println!("██████╔╝██║█████╔╝ ██║");
                println!("██╔═══╝ ██║██╔═██╗ ██║");
                println!("██║     ██║██║  ██╗██║");
                println!("╚═╝     ╚═╝╚═╝  ╚═╝╚═╝");
                println!("");
                println!("piki-multi-ai v{version}");
                println!("");
                println!("Author: Juan Knebel");
                println!("Contact: juanknebel@gmail.com");
                println!("Web: github.com/juanknebel/piki-multi-ai");
                println!("License: GPL-2.0");
                println!("");
                return Ok(());
            }
        }
    }

    // Install panic hook that restores terminal before printing panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture);
        let _ = ratatui::restore();
        original_hook(panic_info);
    }));

    let terminal = ratatui::init();
    crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture)?;
    let result = run(terminal).await;
    crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    result
}

async fn run(mut terminal: DefaultTerminal) -> anyhow::Result<()> {
    let manager = WorkspaceManager::new();
    let mut app = App::new();
    app.sysinfo = sysinfo::spawn_sysinfo_poller();
    app.theme = theme::load();

    // Compute real terminal dimensions for PTY spawning
    let term_size = terminal.size()?;
    let pty_area =
        ui::layout::compute_terminal_area_with(Rect::new(0, 0, term_size.width, term_size.height), app.sidebar_pct);
    app.pty_rows = pty_area.height;
    app.pty_cols = pty_area.width;

    // Restore persisted workspaces from all project configs
    let entries = ws_config::load_all();
    for entry in entries {
        let mut ws = app::Workspace::new(
            entry.name,
            entry.description,
            entry.prompt.clone(),
            entry.kanban_path,
            entry.branch,
            entry.worktree_path,
            entry.source_repo,
        );

        // Spawn initial Shell tab
        spawn_initial_shell(&mut ws, app.pty_rows, app.pty_cols).await;

        // Start file watcher
        match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
            Ok(watcher) => {
                ws.watcher = Some(watcher);
            }
            Err(e) => {
                app.status_message = Some(format!("Watcher error: {}", e));
            }
        }

        // Initial file status refresh so pre-existing changes show up
        let _ = ws.refresh_changed_files().await;

        app.workspaces.push(ws);
    }
    if !app.workspaces.is_empty() {
        app.switch_workspace(0);
    }

    loop {
        // Render
        terminal.draw(|frame| {
            ui::layout::render(frame, &mut app);
        })?;

        // Poll for events with timeout (non-blocking for async tasks)
        let poll_result = event::poll(TICK_RATE);
        if matches!(poll_result, Ok(true)) {
            let read_result = event::read();
            match read_result {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    if let Some(action) = handle_key_event(&mut app, key) {
                        execute_action(&mut app, &manager, action, &mut terminal).await?;
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    if let Some(action) = handle_mouse_event(&mut app, mouse, &mut terminal) {
                        execute_action(&mut app, &manager, action, &mut terminal).await?;
                    }
                },
                Ok(Event::Resize(cols, rows)) => {
                    let new_area = ui::layout::compute_terminal_area_with(Rect::new(0, 0, cols, rows), app.sidebar_pct);
                    app.pty_rows = new_area.height;
                    app.pty_cols = new_area.width;
                    // Resize all PTY sessions in all workspaces
                    for ws in &mut app.workspaces {
                        for tab in &mut ws.tabs {
                            if let Some(ref mut pty) = tab.pty_session {
                                let _ = pty.resize(new_area.height, new_area.width);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => continue, // Transient crossterm error, skip this tick
            }
        }

        // Poll file watcher events — mark workspaces as dirty when files change
        let now = Instant::now();
        for ws in &mut app.workspaces {
            if let Some(ref mut watcher) = ws.watcher {
                if watcher.try_recv().is_some() {
                    watcher.drain();
                    ws.dirty = true;
                }
            }
            // Check if active tab PTY process has exited
            let mut pty_done = false;
            if let Some(tab) = ws.current_tab_mut() {
                if let Some(ref mut pty) = tab.pty_session {
                    if !pty.is_alive() {
                        pty_done = true;
                    }
                    let current_bytes = pty.bytes_processed();
                    if current_bytes != tab.last_bytes_processed {
                        tab.last_bytes_processed = current_bytes;
                    }
                }
            }
            if pty_done {
                ws.status = app::WorkspaceStatus::Done;
            }
            // Debounced refresh of changed files list via git diff
            // Refresh when dirty (debounced) or periodically to catch commits/rebases
            let since_last = ws.last_refresh.map(|t| now.duration_since(t));
            let should_refresh = if ws.dirty {
                since_last.map(|d| d >= DEBOUNCE).unwrap_or(true)
            } else {
                since_last.map(|d| d >= PERIODIC_REFRESH).unwrap_or(true)
            };
            if should_refresh {
                let _ = ws.refresh_changed_files().await;
                ws.last_refresh = Some(now);
            }
        }

        if app.should_quit {
            shutdown(&mut app);
            break;
        }
    }

    Ok(())
}

/// Async actions triggered by key events
enum Action {
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
}

async fn execute_action(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(name, description, prompt, kanban_path, dir) => {
            match manager.create(&name, &description, &prompt, kanban_path, &dir).await {
                Ok(ws) => {
                    app.workspaces.push(ws);
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    // Spawn initial Shell tab
                    spawn_initial_shell(&mut app.workspaces[new_idx], app.pty_rows, app.pty_cols)
                        .await;

                    // Auto-send prompt to active tab PTY if non-empty
                    if !prompt.is_empty() {
                        let ws = &mut app.workspaces[new_idx];
                        if let Some(tab) = ws.current_tab_mut() {
                            if let Some(ref mut pty) = tab.pty_session {
                                // Small delay to let the PTY initialize
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                let prompt_with_newline = format!("{}\n", prompt);
                                let _ = pty.write(prompt_with_newline.as_bytes());
                            }
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

                    // Persist config
                    let source = app.workspaces[new_idx].source_repo.clone();
                    let _ = ws_config::save(&source, &app.workspaces);
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
                let source = ws.source_repo.clone();
                let _ = ws_config::save(&source, &app.workspaces);
                app.status_message = Some("Workspace updated".into());
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
                        let _ = ws_config::save(&source_repo, &app.workspaces);
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
                let _ = ws_config::save(&source_repo, &app.workspaces);
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
            if let Some(ws) = app.workspaces.get(app.active_workspace) {
                if let Some(file) = ws.changed_files.get(file_idx) {
                    let worktree_path = ws.path.clone();
                    let file_path = file.path.clone();
                    let file_status = file.status.clone();
                    // Use a reasonable width; TODO: pass actual panel width
                    let width = 120;
                    match diff::runner::run_diff(&worktree_path, &file_path, width, &file_status)
                        .await
                    {
                        Ok(ansi_bytes) => {
                            use ansi_to_tui::IntoText;
                            match ansi_bytes.into_text() {
                                Ok(text) => {
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
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if let Some(file) = ws.changed_files.get(file_idx) {
                    let file_path = file.path.clone();
                    let worktree = ws.path.clone();
                    let output = tokio::process::Command::new("git")
                        .args(["add", &file_path])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    if output.status.success() {
                        app.status_message = Some(format!("Staged: {}", file_path));
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        app.status_message = Some(format!("Stage failed: {}", stderr.trim()));
                    }
                    ws.dirty = true;
                    let _ = ws.refresh_changed_files().await;
                }
            }
        }
        Action::GitUnstage(file_idx) => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                if let Some(file) = ws.changed_files.get(file_idx) {
                    let file_path = file.path.clone();
                    let worktree = ws.path.clone();
                    let output = tokio::process::Command::new("git")
                        .args(["reset", "HEAD", &file_path])
                        .current_dir(&worktree)
                        .output()
                        .await?;
                    if output.status.success() {
                        app.status_message = Some(format!("Unstaged: {}", file_path));
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        app.status_message = Some(format!("Unstage failed: {}", stderr.trim()));
                    }
                    ws.dirty = true;
                    let _ = ws.refresh_changed_files().await;
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
                let _ = ws.refresh_changed_files().await;
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
                let main_branch =
                    WorkspaceManager::detect_main_branch(&source_repo).await;

                match strategy {
                    MergeStrategy::Merge => {
                        // Stash source repo if dirty
                        let src_status = tokio::process::Command::new("git")
                            .args(["status", "--porcelain"])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        let src_dirty =
                            !String::from_utf8_lossy(&src_status.stdout).trim().is_empty();
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
                        let prev =
                            String::from_utf8_lossy(&prev_branch.stdout).trim().to_string();

                        // Checkout main
                        let checkout = tokio::process::Command::new("git")
                            .args(["checkout", &main_branch])
                            .current_dir(&source_repo)
                            .output()
                            .await?;
                        if !checkout.status.success() {
                            let stderr = String::from_utf8_lossy(&checkout.stderr);
                            app.status_message =
                                Some(format!("Checkout {} failed: {}", main_branch, stderr.trim()));
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
                            app.status_message =
                                Some(format!("✓ Merged '{}' into {}: {}", branch, main_branch, first));
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
                        let prev =
                            String::from_utf8_lossy(&prev_branch.stdout).trim().to_string();

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
                        Box::new(flow::provider_jira::JiraProvider::from_env()) as Box<dyn flow::provider::Provider>
                    } else {
                        let default_path = kanban_path_opt.map(std::path::PathBuf::from).unwrap_or_else(|| {
                            app.config.kanban.path.clone().map(std::path::PathBuf::from).unwrap_or_else(|| {
                                dirs::home_dir()
                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                                    .join(".config/flow/boards/default")
                            })
                        });
                        
                        let expanded_path = if let Some(path_str) = default_path.to_str() {
                            if path_str.starts_with("~/") {
                                if let Some(home) = dirs::home_dir() {
                                    home.join(&path_str[2..])
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
                                app.status_message = Some(format!("Failed to create kanban dir: {}", e));
                            } else {
                                let board_content = "col todo \"TO DO\"\ncol in_progress \"IN PROGRESS\"\ncol in_review \"IN REVIEW\"\ncol done \"DONE\"\n";
                                if let Err(e) = std::fs::write(&board_txt, board_content) {
                                    app.status_message = Some(format!("Failed to write board.txt: {}", e));
                                } else {
                                    for col in &["todo", "in_progress", "in_review", "done"] {
                                        let col_dir = expanded_path.join("cols").join(col);
                                        let _ = std::fs::create_dir_all(&col_dir);
                                        let _ = std::fs::write(col_dir.join("order.txt"), "");
                                    }
                                }
                            }
                        }

                        Box::new(flow::provider_local::LocalProvider::new(expanded_path)) as Box<dyn flow::provider::Provider>
                    };

                    let board = kanban_provider.load_board().unwrap_or_else(|_e| {
                        flow::Board { columns: vec![] }
                    });
                    let mut kanban = flow::App::new(board);
                    if kanban.board.columns.is_empty() {
                        kanban.banner = Some("Load failed or empty board. Check board.txt.".to_string());
                    }
                    ws.kanban_app = Some(kanban);
                    ws.kanban_provider = Some(kanban_provider);
                }

                let idx = spawn_tab(ws, provider, app.pty_rows, app.pty_cols).await;
                ws.active_tab = idx;
                app.status_message = Some(format!("Opened {} tab", provider.label()));
            }
        }
        Action::OpenMarkdown(path) => {
            match std::fs::read_to_string(&path) {
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
            }
        }
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
                    app.status_message = Some(
                        "mdr not found. Install: cargo install markdown-reader".to_string(),
                    );
                }
            }
            if app.mode == AppMode::FuzzySearch {
                app.fuzzy = None;
                app.mode = AppMode::Normal;
            }
        }
    }
    Ok(())
}

/// Kill all PTY sessions and drop watchers for a clean exit.
fn shutdown(app: &mut App) {
    for ws in &mut app.workspaces {
        for tab in &mut ws.tabs {
            if let Some(ref mut pty) = tab.pty_session {
                let _ = pty.kill();
            }
        }
        ws.tabs.clear();
        ws.watcher = None;
    }
}

/// Spawn an initial Shell tab for a workspace.
async fn spawn_initial_shell(ws: &mut app::Workspace, rows: u16, cols: u16) {
    let idx = ws.add_tab(AIProvider::Shell, false); // first shell is not closable
    let cmd = AIProvider::Shell.resolved_command();
    match PtySession::spawn(&ws.path, rows, cols, &cmd).await {
        Ok(session) => {
            ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
            ws.tabs[idx].pty_session = Some(session);
            ws.status = app::WorkspaceStatus::Busy;
        }
        Err(_) => {}
    }
}

/// Spawn a new tab with the given provider in a workspace.
async fn spawn_tab(ws: &mut app::Workspace, provider: AIProvider, rows: u16, cols: u16) -> usize {
    let idx = ws.add_tab(provider, true);
    if provider == AIProvider::Kanban {
        return idx;
    }
    let cmd = provider.resolved_command();
    match PtySession::spawn(&ws.path, rows, cols, &cmd).await {
        Ok(session) => {
            ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
            ws.tabs[idx].pty_session = Some(session);
        }
        Err(_) => {}
    }
    idx
}

/// Probe the actual scrollback buffer size by setting a large offset and reading back.
/// `scrollback()` returns the current offset (which is always 0 after render reset),
/// so we temporarily set it to MAX, read the clamped value, then restore to 0.
fn scrollback_max(parser: &Arc<std::sync::Mutex<vt100::Parser>>) -> usize {
    let mut guard = parser.lock().unwrap();
    guard.screen_mut().set_scrollback(usize::MAX);
    let max = guard.screen().scrollback();
    guard.screen_mut().set_scrollback(0);
    max
}

fn copy_visible_terminal(app: &mut App) {
    if let Some(ws) = app.workspaces.get(app.active_workspace)
        && let Some(tab) = ws.current_tab()
        && let Some(ref parser) = tab.pty_parser
    {
        let mut guard = parser.lock().unwrap();
        guard.screen_mut().set_scrollback(tab.term_scroll);
        let text = guard.screen().contents();
        guard.screen_mut().set_scrollback(0);
        drop(guard);
        match clipboard::copy_to_clipboard(&text) {
            Ok(()) => {
                app.status_message = Some("Terminal content copied".into());
            }
            Err(e) => {
                app.status_message = Some(format!("Copy failed: {}", e));
            }
        }
    }
}

/// Recompute PTY dimensions after sidebar resize and resize all PTY sessions
fn resize_all_ptys(app: &mut App) {
    // We need the current terminal size; use cached pty dimensions as proxy
    // The actual resize will happen on next render via compute_terminal_area_with
    // For now, just mark that a resize is needed — the Event::Resize path handles actual PTY resize
    // But we can estimate from the stored terminal_inner_area
    if let Some(inner) = app.terminal_inner_area {
        // Rough estimate: total width = inner.width / old_main_pct * 100
        let total_width = inner.x + inner.width + 2; // approximate
        let total_height = inner.y + inner.height + 4; // approximate (tabs+subtabs+status+footer+borders)
        let new_area = ui::layout::compute_terminal_area_with(
            Rect::new(0, 0, total_width, total_height),
            app.sidebar_pct,
        );
        app.pty_rows = new_area.height;
        app.pty_cols = new_area.width;
        for ws in &mut app.workspaces {
            for tab in &mut ws.tabs {
                if let Some(ref mut pty) = tab.pty_session {
                    let _ = pty.resize(new_area.height, new_area.width);
                }
            }
        }
    }
}

/// Check if a point (col, row) is inside a Rect
fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Calculate which workspace tab index was clicked based on column position
fn tab_index_at(app: &App, col: u16, area: Rect) -> Option<usize> {
    let mut x = area.x;
    for (i, ws) in app.workspaces.iter().enumerate() {
        let tab_width = ws.name.len() as u16 + 2; // " name "
        if col >= x && col < x + tab_width {
            return Some(i);
        }
        x += tab_width + 1; // +1 for "|" divider
    }
    None
}

/// Calculate which sub-tab index was clicked and whether the close button was hit
fn subtab_index_at(app: &App, col: u16, area: Rect) -> Option<(usize, bool)> {
    let ws = app.current_workspace()?;
    let mut x = area.x;
    for (i, tab) in ws.tabs.iter().enumerate() {
        let label = tab.markdown_label.as_deref().unwrap_or(tab.provider.label());
        // Matches subtabs.rs: format!(" {}{} ", label, close_marker) where close_marker = " ×" or ""
        // Display widths: " " (1) + label (ascii len) + " ×" (2 display cols) + " " (1) = label.len() + 4
        // Without close: " " (1) + label + " " (1) = label.len() + 2
        let tab_display_width = if tab.closable {
            label.len() as u16 + 4 // " label × "
        } else {
            label.len() as u16 + 2 // " label "
        };
        if col >= x && col < x + tab_display_width {
            // Close button is the last 2 display columns before trailing space: "× "
            let on_close = tab.closable && col >= x + tab_display_width - 3;
            return Some((i, on_close));
        }
        x += tab_display_width + 1; // +1 for "|" divider
    }
    None
}

/// Handle all mouse events. Returns an Action if one needs async execution.
fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent, terminal: &mut DefaultTerminal) -> Option<Action> {
    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            match app.mode {
                AppMode::Help => { app.help_scroll = app.help_scroll.saturating_sub(3); }
                AppMode::Diff => { app.diff_scroll = app.diff_scroll.saturating_sub(3); }
                AppMode::FuzzySearch => {
                    if let Some(ref mut state) = app.fuzzy {
                        state.selected = state.selected.saturating_sub(1);
                    }
                }
                AppMode::Normal | AppMode::InlineEdit => {
                    if rect_contains(app.ws_list_area, col, row) {
                        app.select_prev_workspace();
                    } else if rect_contains(app.file_list_area, col, row) {
                        app.prev_file();
                    } else if rect_contains(app.main_content_area, col, row) {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(tab) = ws.current_tab_mut()
                        {
                            if tab.markdown_content.is_some() {
                                tab.markdown_scroll = tab.markdown_scroll.saturating_sub(3);
                            } else if let Some(ref parser) = tab.pty_parser {
                                let max = scrollback_max(parser);
                                tab.term_scroll = (tab.term_scroll + 3).min(max);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        MouseEventKind::ScrollDown => {
            match app.mode {
                AppMode::Help => { app.help_scroll = app.help_scroll.saturating_add(3); }
                AppMode::Diff => { app.diff_scroll = app.diff_scroll.saturating_add(3); }
                AppMode::FuzzySearch => {
                    if let Some(ref mut state) = app.fuzzy {
                        if !state.results.is_empty() {
                            state.selected = (state.selected + 1).min(state.results.len() - 1);
                        }
                    }
                }
                AppMode::Normal | AppMode::InlineEdit => {
                    if rect_contains(app.ws_list_area, col, row) {
                        app.select_next_workspace();
                    } else if rect_contains(app.file_list_area, col, row) {
                        app.next_file();
                    } else if rect_contains(app.main_content_area, col, row) {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(tab) = ws.current_tab_mut()
                        {
                            if tab.markdown_content.is_some() {
                                tab.markdown_scroll = tab.markdown_scroll.saturating_add(3);
                            } else {
                                tab.term_scroll = tab.term_scroll.saturating_sub(3);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Detect double-click
            let now = Instant::now();
            let is_double_click = app
                .last_click
                .is_some_and(|(t, c, r)| now.duration_since(t).as_millis() < 400 && c == col && r == row);
            app.last_click = Some((now, col, row));

            // Dismiss overlays on click
            match app.mode {
                AppMode::Help => { app.mode = AppMode::Normal; return None; }
                AppMode::About => { app.mode = AppMode::Normal; return None; }
                AppMode::WorkspaceInfo => { app.mode = AppMode::Normal; return None; }
                _ => {}
            }

            // Check resize borders first
            let on_sidebar_border = col >= app.sidebar_x.saturating_sub(1)
                && col <= app.sidebar_x + 1
                && row < app.left_area_rect.y + app.left_area_rect.height + app.left_area_rect.y;
            let on_left_split_border = row >= app.left_split_y.saturating_sub(1)
                && row <= app.left_split_y
                && col < app.sidebar_x;

            if on_sidebar_border {
                app.resize_drag = Some(app::ResizeDrag::Sidebar);
            } else if on_left_split_border {
                app.resize_drag = Some(app::ResizeDrag::LeftSplit);
            } else if app.mode == AppMode::Normal {
                // Click on workspace tabs
                if rect_contains(app.tabs_area, col, row) {
                    if let Some(idx) = tab_index_at(app, col, app.tabs_area) {
                        app.switch_workspace(idx);
                    }
                }
                // Click on sub-tabs
                else if rect_contains(app.subtabs_area, col, row) {
                    let subtabs_area = app.subtabs_area;
                    if let Some((idx, on_close)) = subtab_index_at(app, col, subtabs_area) {
                        if on_close {
                            if let Some(ws) = app.current_workspace() {
                                if ws.tabs.get(idx).is_some_and(|t| t.closable) {
                                    app.close_tab_target = Some(idx);
                                    app.mode = AppMode::ConfirmCloseTab;
                                }
                            }
                        } else if let Some(ws) = app.current_workspace_mut() {
                            ws.active_tab = idx;
                        }
                    }
                }
                // Click on workspace list
                else if rect_contains(app.ws_list_area, col, row) {
                    app.active_pane = ActivePane::WorkspaceList;
                    let inner_y = app.ws_list_area.y + 1;
                    if row >= inner_y {
                        let relative_row = (row - inner_y) as usize;
                        let item_height = 3;
                        let clicked_idx = relative_row / item_height;
                        if clicked_idx < app.workspaces.len() {
                            app.selected_workspace = clicked_idx;
                            app.switch_workspace(clicked_idx);
                        }
                    }
                }
                // Click on file list
                else if rect_contains(app.file_list_area, col, row) {
                    app.active_pane = ActivePane::GitStatus;
                    let inner_y = app.file_list_area.y + 1;
                    if row >= inner_y {
                        let relative_row = (row - inner_y) as usize;
                        if let Some(ws) = app.current_workspace() {
                            if relative_row < ws.changed_files.len() {
                                app.selected_file = relative_row;
                                // Double-click opens diff
                                if is_double_click {
                                    return Some(Action::OpenDiff(relative_row));
                                }
                            }
                        }
                    }
                }
                // Click on main panel — start text selection
                else if rect_contains(app.main_content_area, col, row) {
                    app.active_pane = ActivePane::MainPanel;
                    app.interacting = true;
                    if let Some(inner) = app.terminal_inner_area {
                        if rect_contains(inner, col, row) {
                            let cell_row = row - inner.y;
                            let cell_col = col - inner.x;
                            app.selection = Some(app::Selection::new(cell_row, cell_col));
                        }
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = app.resize_drag {
                let total = terminal.size().unwrap_or_default();
                match drag {
                    app::ResizeDrag::Sidebar => {
                        let pct = ((col as u32) * 100 / total.width.max(1) as u32) as u16;
                        app.sidebar_pct = pct.clamp(10, 90);
                        resize_all_ptys(app);
                    }
                    app::ResizeDrag::LeftSplit => {
                        let left_top = app.left_area_rect.y;
                        let left_height = app.left_area_rect.height;
                        if left_height > 0 {
                            let rel = row.saturating_sub(left_top) as u32;
                            let pct = (rel * 100 / left_height as u32) as u16;
                            app.left_split_pct = pct.clamp(10, 90);
                        }
                    }
                }
            } else if let Some(ref mut sel) = app.selection {
                if let Some(inner) = app.terminal_inner_area {
                    let cell_row = row.saturating_sub(inner.y).min(inner.height.saturating_sub(1));
                    let cell_col = col.saturating_sub(inner.x).min(inner.width.saturating_sub(1));
                    sel.end_row = cell_row;
                    sel.end_col = cell_col;
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.resize_drag.is_some() {
                app.resize_drag = None;
            } else if let Some(ref mut sel) = app.selection {
                sel.active = false;
                let (sr, sc, er, ec) = sel.normalized();
                if sr != er || sc != ec {
                    if let Some(ws) = app.workspaces.get(app.active_workspace)
                        && let Some(tab) = ws.current_tab()
                        && let Some(ref parser) = tab.pty_parser
                    {
                        let mut guard = parser.lock().unwrap();
                        guard.screen_mut().set_scrollback(tab.term_scroll);
                        let text = guard.screen().contents_between(sr, sc, er, ec + 1);
                        guard.screen_mut().set_scrollback(0);
                        if let Err(e) = clipboard::copy_to_clipboard(&text) {
                            app.status_message = Some(format!("Copy failed: {}", e));
                        } else {
                            app.status_message = Some("Selection copied".into());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    None
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Workspace info overlay — h/l or arrows for horizontal scroll, Esc/i to close
    if app.mode == AppMode::WorkspaceInfo {
        if app.config.matches_workspace_info(key, "right") || app.config.matches_workspace_info(key, "right_alt") {
            app.info_hscroll = app.info_hscroll.saturating_add(4);
        } else if app.config.matches_workspace_info(key, "left") || app.config.matches_workspace_info(key, "left_alt") {
            app.info_hscroll = app.info_hscroll.saturating_sub(4);
        } else if app.config.matches_workspace_info(key, "exit") || app.config.matches_workspace_info(key, "exit_info") {
            app.info_hscroll = 0;
            app.mode = AppMode::Normal;
            // Re-enable mouse capture
            let _ = crossterm::execute!(
                std::io::stderr(),
                crossterm::event::EnableMouseCapture
            );
        }
        return None;
    }

    // About overlay — close with Esc
    if app.mode == AppMode::About {
        if app.config.matches_about(key, "exit") {
            app.mode = AppMode::Normal;
        }
        return None;
    }

    // Help overlay — scroll with j/k/arrows, close with Esc/q/?
    if app.mode == AppMode::Help {
        if app.config.matches_help(key, "down") || app.config.matches_help(key, "down_alt") {
            app.help_scroll = app.help_scroll.saturating_add(1);
        } else if app.config.matches_help(key, "up") || app.config.matches_help(key, "up_alt") {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        } else if app.config.matches_help(key, "page_down") {
            app.help_scroll = app.help_scroll.saturating_add(10);
        } else if app.config.matches_help(key, "page_up") {
            app.help_scroll = app.help_scroll.saturating_sub(10);
        } else if app.config.matches_help(key, "scroll_top") {
            app.help_scroll = 0;
        } else if app.config.matches_help(key, "scroll_bottom") {
            app.help_scroll = u16::MAX; // clamped during render
        } else if app.config.matches_help(key, "exit") || app.config.matches_help(key, "exit_alt") || app.config.matches_help(key, "exit_help") {
            app.help_scroll = 0;
            app.mode = AppMode::Normal;
        }
        return None;
    }

    // Fuzzy search overlay captures all input
    if app.mode == AppMode::FuzzySearch {
        return handle_fuzzy_search_input(app, key);
    }

    // Inline editor captures all input
    if app.mode == AppMode::InlineEdit {
        return handle_inline_edit_input(app, key);
    }

    // New workspace dialog captures all input
    if app.mode == AppMode::NewWorkspace {
        return handle_new_workspace_input(app, key);
    }

    // Edit workspace dialog captures all input
    if app.mode == AppMode::EditWorkspace {
        return handle_edit_workspace_input(app, key);
    }

    // Commit message dialog captures all input
    if app.mode == AppMode::CommitMessage {
        return handle_commit_message_input(app, key);
    }

    // Confirm merge dialog captures all input
    if app.mode == AppMode::ConfirmMerge {
        return handle_confirm_merge_input(app, key);
    }

    // New tab provider selection dialog
    if app.mode == AppMode::NewTab {
        return handle_new_tab_input(app, key);
    }

    // Confirm close tab dialog captures all input
    if app.mode == AppMode::ConfirmCloseTab {
        return handle_confirm_close_tab_input(app, key);
    }

    // Confirm quit dialog captures all input
    if app.mode == AppMode::ConfirmQuit {
        return handle_confirm_quit_input(app, key);
    }

    // Confirm delete dialog captures all input
    if app.mode == AppMode::ConfirmDelete {
        return handle_confirm_delete_input(app, key);
    }

    // Clear status message and selection on any key
    app.status_message = None;
    app.selection = None;

    if app.interacting {
        handle_interaction_mode(app, key)
    } else {
        handle_navigation_mode(app, key)
    }
}

// ── Navigation mode: hjkl between panes, Enter to interact, global shortcuts ──

fn handle_navigation_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Pane navigation
    if app.config.matches_navigation(key, "left") || app.config.matches_navigation(key, "left_alt") {
        if app.active_pane == ActivePane::MainPanel {
            app.active_pane = ActivePane::GitStatus;
        }
    } else if app.config.matches_navigation(key, "right") || app.config.matches_navigation(key, "right_alt") {
        if matches!(
            app.active_pane,
            ActivePane::WorkspaceList | ActivePane::GitStatus
        ) {
            app.active_pane = ActivePane::MainPanel;
        }
    } else if app.config.matches_navigation(key, "down") || app.config.matches_navigation(key, "down_alt") {
        if app.active_pane == ActivePane::WorkspaceList {
            app.active_pane = ActivePane::GitStatus;
        }
    } else if app.config.matches_navigation(key, "up") || app.config.matches_navigation(key, "up_alt") {
        if app.active_pane == ActivePane::GitStatus {
            app.active_pane = ActivePane::WorkspaceList;
        }
    } else if app.config.matches_navigation(key, "enter_pane") {
        app.interacting = true;
    } else if app.config.matches_navigation(key, "quit") {
        app.mode = AppMode::ConfirmQuit;
    } else if app.config.matches_navigation(key, "help") {
        app.mode = AppMode::Help;
    } else if app.config.matches_navigation(key, "about") {
        app.mode = AppMode::About;
    } else if app.config.matches_navigation(key, "workspace_info") {
        if !app.workspaces.is_empty() {
            app.mode = AppMode::WorkspaceInfo;
            app.info_hscroll = 0;
            let _ = crossterm::execute!(
                std::io::stderr(),
                crossterm::event::DisableMouseCapture
            );
        }
    } else if app.config.matches_navigation(key, "edit_workspace") {
        if !app.workspaces.is_empty() {
            let ws = &app.workspaces[app.selected_workspace];
            let k_path = ws.kanban_path.clone().unwrap_or_default();
            let prompt = ws.prompt.clone();
            app.kanban_input_buffer = k_path;
            app.prompt_input_buffer = prompt;
            app.kanban_input_cursor = app.kanban_input_buffer.chars().count();
            app.prompt_input_cursor = app.prompt_input_buffer.chars().count();
            app.active_dialog_field = DialogField::KanbanPath;
            app.edit_target = Some(app.selected_workspace);
            app.mode = AppMode::EditWorkspace;
        }
    } else if app.config.matches_navigation(key, "clone_workspace") {
        if !app.workspaces.is_empty() {
            let ws = &app.workspaces[app.selected_workspace];
            let dir = ws.source_repo.display().to_string();
            let kanban = ws.kanban_path.clone().unwrap_or_default();
            let prompt = ws.prompt.clone();
            app.mode = AppMode::NewWorkspace;
            app.input_buffer.clear();
            app.input_cursor = 0;
            app.dir_input_buffer = dir;
            app.dir_input_cursor = app.dir_input_buffer.chars().count();
            app.desc_input_buffer.clear();
            app.desc_input_cursor = 0;
            app.prompt_input_buffer = prompt;
            app.prompt_input_cursor = app.prompt_input_buffer.chars().count();
            app.kanban_input_buffer = kanban;
            app.kanban_input_cursor = app.kanban_input_buffer.chars().count();
            app.active_dialog_field = DialogField::Name;
        }
    } else if app.config.matches_navigation(key, "new_workspace") {
        app.mode = AppMode::NewWorkspace;
        app.input_buffer.clear();
        app.dir_input_buffer.clear();
        app.desc_input_buffer.clear();
        app.prompt_input_buffer.clear();
        app.kanban_input_buffer.clear();
        app.input_cursor = 0;
        app.dir_input_cursor = 0;
        app.desc_input_cursor = 0;
        app.prompt_input_cursor = 0;
        app.kanban_input_cursor = 0;
        app.active_dialog_field = DialogField::Name;
    } else if app.config.matches_navigation(key, "delete_workspace") {
        if !app.workspaces.is_empty() {
            app.delete_target = Some(app.selected_workspace);
            app.mode = AppMode::ConfirmDelete;
        }
    } else if app.config.matches_navigation(key, "commit") {
        if app.current_workspace().is_some() {
            app.commit_msg_buffer.clear();
            app.mode = AppMode::CommitMessage;
        }
    } else if app.config.matches_navigation(key, "merge") {
        if app.current_workspace().is_some() {
            app.mode = AppMode::ConfirmMerge;
        }
    } else if app.config.matches_navigation(key, "push") {
        if app.current_workspace().is_some() {
            return Some(Action::GitPush);
        }
    } else if app.config.matches_navigation(key, "next_workspace") {
        app.next_workspace();
    } else if app.config.matches_navigation(key, "prev_workspace") {
        app.prev_workspace();
    } else if app.config.matches_navigation(key, "scroll_up") {
        if app.active_pane == ActivePane::MainPanel
            && app.mode == AppMode::Normal
            && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
            && let Some(ref parser) = tab.pty_parser
        {
            let max = scrollback_max(parser);
            tab.term_scroll = (tab.term_scroll + 3).min(max);
        }
    } else if app.config.matches_navigation(key, "scroll_down") {
        if app.active_pane == ActivePane::MainPanel
            && app.mode == AppMode::Normal
            && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
        {
            tab.term_scroll = tab.term_scroll.saturating_sub(3);
        }
    } else if app.config.matches_navigation(key, "page_up") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
            && let Some(ref parser) = tab.pty_parser
        {
            let screen_height = app.pty_rows as usize;
            let max = scrollback_max(parser);
            tab.term_scroll = (tab.term_scroll + screen_height).min(max);
        }
    } else if app.config.matches_navigation(key, "page_down") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
        {
            let screen_height = app.pty_rows as usize;
            tab.term_scroll = tab.term_scroll.saturating_sub(screen_height);
        }
    } else if app.config.matches_navigation(key, "copy") {
        copy_visible_terminal(app);
    } else if app.config.matches_navigation(key, "fuzzy_search") || app.config.matches_navigation(key, "fuzzy_search_alt") {
        app.open_fuzzy_search();
    } else if app.config.matches_navigation(key, "sidebar_shrink") || app.config.matches_navigation(key, "sidebar_shrink_alt") {
        app.sidebar_pct = app.sidebar_pct.saturating_sub(5).max(10);
        resize_all_ptys(app);
    } else if app.config.matches_navigation(key, "sidebar_grow") || app.config.matches_navigation(key, "sidebar_grow_alt") {
        app.sidebar_pct = (app.sidebar_pct + 5).min(90);
        resize_all_ptys(app);
    } else if app.config.matches_navigation(key, "split_up") || app.config.matches_navigation(key, "split_up_alt") {
        app.left_split_pct = (app.left_split_pct + 10).min(90);
    } else if app.config.matches_navigation(key, "split_down") {
        app.left_split_pct = app.left_split_pct.saturating_sub(10).max(10);
    } else if app.config.matches_navigation(key, "next_tab") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
            if !ws.tabs.is_empty() {
                ws.active_tab = (ws.active_tab + 1) % ws.tabs.len();
            }
        }
    } else if app.config.matches_navigation(key, "prev_tab") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
            if !ws.tabs.is_empty() {
                ws.active_tab = (ws.active_tab + ws.tabs.len() - 1) % ws.tabs.len();
            }
        }
    } else if app.config.matches_navigation(key, "new_tab") {
        if app.current_workspace().is_some() {
            app.mode = AppMode::NewTab;
        }
    } else if app.config.matches_navigation(key, "close_tab") {
        if let Some(ws) = app.workspaces.get(app.active_workspace) {
            if ws.current_tab().is_some_and(|t| t.closable) {
                app.close_tab_target = Some(ws.active_tab);
                app.mode = AppMode::ConfirmCloseTab;
            } else {
                app.status_message = Some("Cannot close the initial shell tab".into());
            }
        }
    } else if let KeyCode::Char(c @ '1'..='9') = key.code {
        let idx = (c as usize) - ('1' as usize);
        app.switch_workspace(idx);
    }
    None
}

// ── Interaction mode: Esc to leave, keys go to the active pane ──

fn handle_interaction_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    match app.active_pane {
        ActivePane::MainPanel => {
            if app.mode == AppMode::Diff {
                handle_diff_interaction(app, key)
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.provider == AIProvider::Kanban)
            {
                handle_kanban_interaction(app, key)
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.markdown_content.is_some())
            {
                handle_markdown_interaction(app, key)
            } else {
                handle_terminal_interaction(app, key)
            }
        }
        ActivePane::WorkspaceList => handle_workspace_interaction(app, key),
        ActivePane::GitStatus => handle_filelist_interaction(app, key),
    }
}

fn handle_kanban_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_interaction(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }

    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let (kanban_app, kanban_provider) = match (&mut ws.kanban_app, &mut ws.kanban_provider) {
        (Some(a), Some(p)) => (a, p),
        _ => return None,
    };

    // Helper to get selected card ID
    let selected_card_id = |a: &flow::App| -> Option<String> {
        a.board
            .columns
            .get(a.col)
            .and_then(|col| col.cards.get(a.row))
            .map(|card| card.id.clone())
    };

    if let Some(edit) = kanban_app.edit_state.as_mut() {
        match key.code {
            KeyCode::Esc => {
                kanban_app.edit_state = None;
            }
            KeyCode::Tab => {
                edit.focus_description = !edit.focus_description;
                edit.cursor_pos = if edit.focus_description {
                    edit.description.len()
                } else {
                    edit.title.len()
                };
            }
            KeyCode::Enter => {
                let card_id = edit.card_id.clone();
                let title = edit.title.clone();
                let description = edit.description.clone();
                if let Err(e) = kanban_provider.update_card(&card_id, &title, &description) {
                    kanban_app.banner = Some(format!("Save failed: {}", e));
                } else {
                    match kanban_provider.load_board() {
                        Ok(b) => {
                            kanban_app.board = b;
                            kanban_app.clamp();
                            // Optional: focus_card_by_id(&mut kanban_app, &card_id);
                            kanban_app.banner = Some("Card saved".to_string());
                        }
                        Err(e) => kanban_app.banner = Some(format!("Reload failed: {}", e)),
                    }
                }
                kanban_app.edit_state = None;
            }
            KeyCode::Char(c) => {
                if edit.focus_description {
                    edit.description.push(c);
                } else {
                    edit.title.push(c);
                    edit.cursor_pos = edit.title.len();
                }
            }
            KeyCode::Backspace => {
                if edit.focus_description {
                    edit.description.pop();
                } else {
                    edit.title.pop();
                    edit.cursor_pos = edit.title.len();
                }
            }
            _ => {}
        }
        return None;
    }

    if kanban_app.confirm_delete {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(card_id) = selected_card_id(kanban_app) {
                    if let Err(e) = kanban_provider.delete_card(&card_id) {
                        kanban_app.banner = Some(format!("Delete failed: {}", e));
                    } else {
                        match kanban_provider.load_board() {
                            Ok(b) => {
                                kanban_app.board = b;
                                kanban_app.clamp();
                                kanban_app.banner = Some(format!("Card {} deleted", card_id));
                            }
                            Err(e) => kanban_app.banner = Some(format!("Reload failed: {}", e)),
                        }
                    }
                }
                kanban_app.confirm_delete = false;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                kanban_app.confirm_delete = false;
            }
            _ => {}
        }
        return None;
    }

    let action = match key.code {
        KeyCode::Char('q') => Some(flow::Action::Quit),
        KeyCode::Esc => Some(flow::Action::CloseOrQuit),
        KeyCode::Char('h') | KeyCode::Left => Some(flow::Action::FocusLeft),
        KeyCode::Char('l') | KeyCode::Right => Some(flow::Action::FocusRight),
        KeyCode::Char('j') | KeyCode::Down => Some(flow::Action::SelectDown),
        KeyCode::Char('k') | KeyCode::Up => Some(flow::Action::SelectUp),
        KeyCode::Char('H') => Some(flow::Action::MoveLeft),
        KeyCode::Char('L') => Some(flow::Action::MoveRight),
        KeyCode::Enter => Some(flow::Action::ToggleDetail),
        KeyCode::Char('r') => Some(flow::Action::Refresh),
        KeyCode::Char('d') => Some(flow::Action::Delete),
        KeyCode::Char('a') | KeyCode::Char('n') => Some(flow::Action::Add),
        KeyCode::Char('e') => Some(flow::Action::Edit),
        _ => None,
    };

    if let Some(a) = action {
        match a {
            flow::Action::Add => {
                let Some(col) = kanban_app.board.columns.get(kanban_app.col) else {
                    kanban_app.banner = Some("Create failed: no column selected".to_string());
                    return None;
                };
                match kanban_provider.create_card(&col.id) {
                    Ok(id) => {
                        kanban_app.edit_state = Some(flow::app::EditState {
                            card_id: id,
                            title: "New card".to_string(),
                            description: "".to_string(),
                            cursor_pos: 8,
                            focus_description: false,
                        });
                    }
                    Err(e) => {
                        kanban_app.banner = Some(format!("Create failed: {}", e));
                    }
                }
            }
            flow::Action::Edit => {
                let Some(col) = kanban_app.board.columns.get(kanban_app.col) else { return None; };
                let Some(card) = col.cards.get(kanban_app.row) else {
                    kanban_app.banner = Some("Edit failed: no card selected".to_string());
                    return None;
                };
                kanban_app.edit_state = Some(flow::app::EditState {
                    card_id: card.id.clone(),
                    title: card.title.clone(),
                    description: card.description.clone(),
                    cursor_pos: card.title.len(),
                    focus_description: false,
                });
            }
            flow::Action::MoveLeft => {
                if let Some((card_id, dst)) = kanban_app.optimistic_move(-1) {
                    if let Err(e) = kanban_provider.move_card(&card_id, &dst) {
                        kanban_app.banner = Some(format!("Move failed: {}", e));
                        // Revert optimistic move by reloading
                        if let Ok(b) = kanban_provider.load_board() {
                            kanban_app.board = b;
                        }
                    } else {
                        kanban_app.banner = Some("Moved".to_string());
                    }
                }
            }
            flow::Action::MoveRight => {
                if let Some((card_id, dst)) = kanban_app.optimistic_move(1) {
                    if let Err(e) = kanban_provider.move_card(&card_id, &dst) {
                        kanban_app.banner = Some(format!("Move failed: {}", e));
                        // Revert optimistic move by reloading
                        if let Ok(b) = kanban_provider.load_board() {
                            kanban_app.board = b;
                        }
                    } else {
                        kanban_app.banner = Some("Moved".to_string());
                    }
                }
            }
            flow::Action::Refresh => {
                match kanban_provider.load_board() {
                    Ok(b) => {
                        kanban_app.board = b;
                        kanban_app.clamp();
                        kanban_app.banner = Some("Refreshed".to_string());
                    }
                    Err(e) => {
                        kanban_app.banner = Some(format!("Refresh failed: {}", e));
                    }
                }
            }
            _ => {
                let should_quit = kanban_app.apply(a);
                if should_quit {
                    app.interacting = false;
                }
            }
        }
    }
    None
}

fn handle_terminal_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_interaction(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    // Ctrl+Shift+V: paste from clipboard
    if app.config.matches_interaction(key, "paste") {
        match clipboard::paste_from_clipboard() {
            Ok(text) => {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    if let Some(tab) = ws.current_tab_mut() {
                        let bracketed = tab.pty_parser
                            .as_ref()
                            .map(|p| p.lock().unwrap().screen().bracketed_paste())
                            .unwrap_or(false);
                        let data = if bracketed {
                            format!("\x1b[200~{}\x1b[201~", text)
                        } else {
                            text
                        };
                        if let Some(ref mut pty) = tab.pty_session {
                            let _ = pty.write(data.as_bytes());
                        }
                    }
                }
            }
            Err(e) => {
                app.status_message = Some(format!("Paste failed: {}", e));
            }
        }
        return None;
    }
    // Ctrl+Shift+C: copy visible terminal content
    if app.config.matches_interaction(key, "copy") {
        copy_visible_terminal(app);
        return None;
    }
    // Forward all other keys to the active tab's PTY
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
        if let Some(tab) = ws.current_tab_mut() {
            if let Some(ref mut pty) = tab.pty_session {
                if let Some(bytes) = pty::input::key_to_bytes(key) {
                    let _ = pty.write(&bytes);
                }
            }
        }
    }
    None
}

fn handle_markdown_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_markdown(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
        if let Some(tab) = ws.current_tab_mut() {
            if app.config.matches_markdown(key, "down") || app.config.matches_markdown(key, "down_alt") {
                tab.markdown_scroll = tab.markdown_scroll.saturating_add(1);
            } else if app.config.matches_markdown(key, "up") || app.config.matches_markdown(key, "up_alt") {
                tab.markdown_scroll = tab.markdown_scroll.saturating_sub(1);
            } else if app.config.matches_markdown(key, "page_down") {
                tab.markdown_scroll = tab.markdown_scroll.saturating_add(20);
            } else if app.config.matches_markdown(key, "page_up") {
                tab.markdown_scroll = tab.markdown_scroll.saturating_sub(20);
            } else if app.config.matches_markdown(key, "scroll_top") {
                tab.markdown_scroll = 0;
            } else if app.config.matches_markdown(key, "scroll_bottom") {
                tab.markdown_scroll = u16::MAX;
            }
        }
    }
    None
}

fn handle_diff_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_diff(key, "exit") {
        app.mode = AppMode::Normal;
        app.diff_content = None;
        app.diff_file_path = None;
        app.interacting = false;
        app.active_pane = ActivePane::GitStatus;
        return None;
    }

    if app.config.matches_diff(key, "down") || app.config.matches_diff(key, "down_alt") {
        app.diff_scroll = app.diff_scroll.saturating_add(1);
    } else if app.config.matches_diff(key, "up") || app.config.matches_diff(key, "up_alt") {
        app.diff_scroll = app.diff_scroll.saturating_sub(1);
    } else if app.config.matches_diff(key, "page_down") {
        app.diff_scroll = app.diff_scroll.saturating_add(20);
    } else if app.config.matches_diff(key, "page_up") {
        app.diff_scroll = app.diff_scroll.saturating_sub(20);
    } else if app.config.matches_diff(key, "scroll_top") {
        app.diff_scroll = 0;
    } else if app.config.matches_diff(key, "scroll_bottom") {
        app.diff_scroll = u16::MAX;
    } else if app.config.matches_diff(key, "next_file") {
        app.next_file();
        return Some(Action::OpenDiff(app.selected_file));
    } else if app.config.matches_diff(key, "prev_file") {
        app.prev_file();
        return Some(Action::OpenDiff(app.selected_file));
    }
    None
}

fn handle_workspace_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_workspace_list(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    if app.config.matches_workspace_list(key, "down") || app.config.matches_workspace_list(key, "down_alt") {
        app.select_next_workspace();
    } else if app.config.matches_workspace_list(key, "up") || app.config.matches_workspace_list(key, "up_alt") {
        app.select_prev_workspace();
    } else if app.config.matches_workspace_list(key, "select") {
        app.switch_workspace(app.selected_workspace);
    } else if app.config.matches_workspace_list(key, "delete") {
        if !app.workspaces.is_empty() {
            app.delete_target = Some(app.selected_workspace);
            app.mode = AppMode::ConfirmDelete;
        }
    } else if app.config.matches_navigation(key, "edit_workspace") {
        if let Some(ws) = app.workspaces.get(app.selected_workspace) {
            let k_path = ws.kanban_path.clone().unwrap_or_default();
            let prompt = ws.prompt.clone();
            app.kanban_input_buffer = k_path;
            app.prompt_input_buffer = prompt;
            app.kanban_input_cursor = app.kanban_input_buffer.chars().count();
            app.prompt_input_cursor = app.prompt_input_buffer.chars().count();
            app.active_dialog_field = DialogField::KanbanPath;
            app.edit_target = Some(app.selected_workspace);
            app.mode = AppMode::EditWorkspace;
            app.interacting = false;
        }
    }
    None
}

fn handle_filelist_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_file_list(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    if app.config.matches_file_list(key, "down") || app.config.matches_file_list(key, "down_alt") {
        app.next_file();
    } else if app.config.matches_file_list(key, "up") || app.config.matches_file_list(key, "up_alt") {
        app.prev_file();
    } else if app.config.matches_file_list(key, "diff") {
        if let Some(ws) = app.current_workspace() {
            if !ws.changed_files.is_empty() {
                return Some(Action::OpenDiff(app.selected_file));
            }
        }
    } else if app.config.matches_file_list(key, "edit_external") {
        if let Some(ws) = app.current_workspace()
            && let Some(file) = ws.changed_files.get(app.selected_file)
        {
            let full_path = ws.path.join(&file.path);
            return Some(Action::OpenEditor(full_path));
        }
    } else if app.config.matches_file_list(key, "edit_inline") {
        if let Some(ws) = app.current_workspace()
            && let Some(file) = ws.changed_files.get(app.selected_file)
        {
            let full_path = ws.path.join(&file.path);
            app.open_inline_editor(full_path);
        }
    } else if app.config.matches_file_list(key, "stage") {
        if let Some(ws) = app.current_workspace() {
            if !ws.changed_files.is_empty() {
                return Some(Action::GitStage(app.selected_file));
            }
        }
    } else if app.config.matches_file_list(key, "unstage") {
        if let Some(ws) = app.current_workspace() {
            if !ws.changed_files.is_empty() {
                return Some(Action::GitUnstage(app.selected_file));
            }
        }
    }
    None
}

fn handle_fuzzy_search_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.fuzzy = None;
            app.mode = AppMode::Normal;
        }
        KeyCode::Up => {
            if let Some(ref mut state) = app.fuzzy
                && state.selected > 0
            {
                state.selected -= 1;
            }
        }
        KeyCode::Down => {
            if let Some(ref mut state) = app.fuzzy
                && !state.results.is_empty()
                && state.selected + 1 < state.results.len()
            {
                state.selected += 1;
            }
        }
        KeyCode::Enter => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.results.get(s.selected))
                .map(|m| m.path.clone());

            if let Some(path) = selected_path {
                // Check if file is in changed_files list; if so, open its diff
                if let Some(ws) = app.current_workspace() {
                    if let Some(idx) = ws.changed_files.iter().position(|f| f.path == path) {
                        app.fuzzy = None;
                        app.mode = AppMode::Normal;
                        app.selected_file = idx;
                        return Some(Action::OpenDiff(idx));
                    } else {
                        app.status_message = Some(format!("{} has no changes to diff", path));
                    }
                }
            }
        }
        // Ctrl+O: open markdown file in a new tab
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.results.get(s.selected))
                .map(|m| m.path.clone());

            if let (Some(rel_path), Some(ws)) = (selected_path, app.current_workspace()) {
                if rel_path.ends_with(".md") || rel_path.ends_with(".markdown") {
                    let full_path = ws.path.join(&rel_path);
                    app.fuzzy = None;
                    app.mode = AppMode::Normal;
                    return Some(Action::OpenMarkdown(full_path));
                } else {
                    app.status_message = Some("Not a markdown file".to_string());
                }
            }
        }
        // Alt+M: open markdown file in external mdr viewer
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::ALT) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.results.get(s.selected))
                .map(|m| m.path.clone());

            if let (Some(rel_path), Some(ws)) = (selected_path, app.current_workspace()) {
                if rel_path.ends_with(".md") || rel_path.ends_with(".markdown") {
                    let full_path = ws.path.join(&rel_path);
                    return Some(Action::OpenMdr(full_path));
                } else {
                    app.status_message = Some("Not a markdown file".to_string());
                }
            }
        }
        // Ctrl+E: open in $EDITOR
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.results.get(s.selected))
                .map(|m| m.path.clone());

            if let (Some(rel_path), Some(ws)) = (selected_path, app.current_workspace()) {
                let full_path = ws.path.join(&rel_path);
                return Some(Action::OpenEditor(full_path));
            }
        }
        // Ctrl+V: open inline editor
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let selected_path = app
                .fuzzy
                .as_ref()
                .and_then(|s| s.results.get(s.selected))
                .map(|m| m.path.clone());

            if let Some(rel_path) = selected_path
                && let Some(ws) = app.current_workspace()
            {
                let full_path = ws.path.join(&rel_path);
                app.fuzzy = None;
                app.open_inline_editor(full_path);
            }
        }
        KeyCode::Backspace => {
            if let Some(ref mut state) = app.fuzzy {
                state.query.pop();
            }
            app.update_fuzzy_filter();
        }
        KeyCode::Char(c) => {
            if let Some(ref mut state) = app.fuzzy {
                state.query.push(c);
            }
            app.update_fuzzy_filter();
        }
        _ => {}
    }
    None
}

fn handle_inline_edit_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.editor = None;
            app.editing_file = None;
            app.mode = AppMode::Normal;
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Save file
            if let (Some(editor), Some(path)) = (&app.editor, &app.editing_file) {
                let content = editor.contents();
                match std::fs::write(path, &content) {
                    Ok(()) => {
                        app.status_message = Some(format!("Saved: {}", path.display()));
                        if let Some(ws) = app.current_workspace_mut() {
                            ws.dirty = true;
                        }
                    }
                    Err(e) => {
                        app.status_message = Some(format!("Save error: {}", e));
                    }
                }
            }
        }
        KeyCode::Up => {
            if let Some(ref mut editor) = app.editor {
                editor.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(ref mut editor) = app.editor {
                editor.move_down();
            }
        }
        KeyCode::Left => {
            if let Some(ref mut editor) = app.editor {
                editor.move_left();
            }
        }
        KeyCode::Right => {
            if let Some(ref mut editor) = app.editor {
                editor.move_right();
            }
        }
        KeyCode::Enter => {
            if let Some(ref mut editor) = app.editor {
                editor.enter();
            }
        }
        KeyCode::Backspace => {
            if let Some(ref mut editor) = app.editor {
                editor.backspace();
            }
        }
        KeyCode::Char(c) => {
            if let Some(ref mut editor) = app.editor {
                editor.insert_char(c);
            }
        }
        KeyCode::Tab => {
            if let Some(ref mut editor) = app.editor {
                // Insert 4 spaces
                for _ in 0..4 {
                    editor.insert_char(' ');
                }
            }
        }
        _ => {}
    }
    // Keep cursor visible after any edit
    if let Some(ref mut editor) = app.editor {
        editor.adjust_scroll(app.pty_rows.saturating_sub(4) as usize);
    }
    None
}

fn dialog_buf_and_cursor(app: &mut App) -> (&mut String, &mut usize) {
    match app.active_dialog_field {
        DialogField::Name => (&mut app.input_buffer, &mut app.input_cursor),
        DialogField::Directory => (&mut app.dir_input_buffer, &mut app.dir_input_cursor),
        DialogField::Description => (&mut app.desc_input_buffer, &mut app.desc_input_cursor),
        DialogField::Prompt => (&mut app.prompt_input_buffer, &mut app.prompt_input_cursor),
        DialogField::KanbanPath => (&mut app.kanban_input_buffer, &mut app.kanban_input_cursor),
    }
}

fn handle_edit_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.active_dialog_field = match app.active_dialog_field {
                DialogField::KanbanPath => DialogField::Prompt,
                _ => DialogField::KanbanPath,
            };
        }
        KeyCode::Char(c) => {
            if !c.is_control() {
                let (buf, cursor) = dialog_buf_and_cursor(app);
                let byte_idx = buf.char_indices().nth(*cursor).map_or(buf.len(), |(i, _)| i);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
        }
        KeyCode::Backspace => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            if *cursor > 0 {
                *cursor -= 1;
                let byte_idx = buf.char_indices().nth(*cursor).map_or(buf.len(), |(i, _)| i);
                buf.remove(byte_idx);
            }
        }
        KeyCode::Delete => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            let len = buf.chars().count();
            if *cursor < len {
                let byte_idx = buf.char_indices().nth(*cursor).map_or(buf.len(), |(i, _)| i);
                buf.remove(byte_idx);
            }
        }
        KeyCode::Left => {
            let (_, cursor) = dialog_buf_and_cursor(app);
            if *cursor > 0 {
                *cursor -= 1;
            }
        }
        KeyCode::Right => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            let len = buf.chars().count();
            if *cursor < len {
                *cursor += 1;
            }
        }
        KeyCode::Home => {
            let (_, cursor) = dialog_buf_and_cursor(app);
            *cursor = 0;
        }
        KeyCode::End => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            *cursor = buf.chars().count();
        }
        KeyCode::Enter => {
            let kanban_path_raw = app.kanban_input_buffer.trim();
            let kanban_path = if kanban_path_raw.is_empty() {
                None
            } else {
                Some(kanban_path_raw.to_string())
            };
            let prompt = app.prompt_input_buffer.clone();
            let idx = app.edit_target.take().unwrap_or(app.active_workspace);

            app.kanban_input_buffer.clear();
            app.prompt_input_buffer.clear();
            app.kanban_input_cursor = 0;
            app.prompt_input_cursor = 0;
            app.mode = AppMode::Normal;
            return Some(Action::EditWorkspace(idx, kanban_path, prompt));
        }
        _ if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('g')
                && key.modifiers.contains(KeyModifiers::CONTROL)) =>
        {
            app.edit_target = None;
            app.kanban_input_buffer.clear();
            app.prompt_input_buffer.clear();
            app.kanban_input_cursor = 0;
            app.prompt_input_cursor = 0;
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
    None
}

fn handle_new_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.active_dialog_field = match app.active_dialog_field {
                DialogField::Name => DialogField::Directory,
                DialogField::Directory => DialogField::Description,
                DialogField::Description => DialogField::Prompt,
                DialogField::Prompt => DialogField::KanbanPath,
                DialogField::KanbanPath => DialogField::Name,
            };
        }
        KeyCode::Char(c) => {
            let valid = match app.active_dialog_field {
                DialogField::Name => {
                    c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/'
                }
                _ => !c.is_control(),
            };
            if valid {
                let (buf, cursor) = dialog_buf_and_cursor(app);
                let byte_idx = buf.char_indices().nth(*cursor).map_or(buf.len(), |(i, _)| i);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
        }
        KeyCode::Backspace => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            if *cursor > 0 {
                let byte_idx =
                    buf.char_indices().nth(*cursor - 1).map_or(buf.len(), |(i, _)| i);
                buf.remove(byte_idx);
                *cursor -= 1;
            }
        }
        KeyCode::Delete => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            if let Some((byte_idx, _)) = buf.char_indices().nth(*cursor) {
                buf.remove(byte_idx);
            }
        }
        KeyCode::Left => {
            let (_, cursor) = dialog_buf_and_cursor(app);
            if *cursor > 0 {
                *cursor -= 1;
            }
        }
        KeyCode::Right => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            let len = buf.chars().count();
            if *cursor < len {
                *cursor += 1;
            }
        }
        KeyCode::Home => {
            let (_, cursor) = dialog_buf_and_cursor(app);
            *cursor = 0;
        }
        KeyCode::End => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            *cursor = buf.chars().count();
        }
        KeyCode::Enter => {
            let name = app.input_buffer.clone();
            let dir_raw = app.dir_input_buffer.clone();
            let description = app.desc_input_buffer.clone();
            let prompt = app.prompt_input_buffer.clone();
            let kanban_path_raw = app.kanban_input_buffer.trim();
            let kanban_path = if kanban_path_raw.is_empty() {
                None
            } else {
                Some(kanban_path_raw.to_string())
            };

            if name.is_empty() || dir_raw.is_empty() {
                app.status_message = Some("Name and directory are required".into());
                return None;
            }

            // Resolve ~ to home directory
            let dir_str = if dir_raw.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    dir_raw.replacen('~', &home.to_string_lossy(), 1)
                } else {
                    dir_raw.clone()
                }
            } else {
                dir_raw.clone()
            };

            let dir = PathBuf::from(&dir_str);
            if !dir.exists() {
                app.status_message = Some(format!("Directory does not exist: {}", dir_str));
                return None;
            }

            app.input_buffer.clear();
            app.dir_input_buffer.clear();
            app.desc_input_buffer.clear();
            app.prompt_input_buffer.clear();
            app.kanban_input_buffer.clear();
            app.input_cursor = 0;
            app.dir_input_cursor = 0;
            app.desc_input_cursor = 0;
            app.prompt_input_cursor = 0;
            app.kanban_input_cursor = 0;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            return Some(Action::CreateWorkspace(name, description, prompt, kanban_path, dir));
        }
        _ if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('g')
                && key.modifiers.contains(KeyModifiers::CONTROL)) =>
        {
            app.input_buffer.clear();
            app.dir_input_buffer.clear();
            app.desc_input_buffer.clear();
            app.prompt_input_buffer.clear();
            app.kanban_input_buffer.clear();
            app.input_cursor = 0;
            app.dir_input_cursor = 0;
            app.desc_input_cursor = 0;
            app.prompt_input_cursor = 0;
            app.kanban_input_cursor = 0;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
        }
        _ => {}
    }
    None
}

fn handle_confirm_close_tab_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(idx) = app.close_tab_target.take() {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    ws.close_tab(idx);
                }
            }
            app.mode = AppMode::Normal;
            None
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.close_tab_target = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

fn handle_confirm_quit_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            app.should_quit = true;
            None
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

fn handle_confirm_delete_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let target = app.delete_target.take();
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            target.map(Action::DeleteWorkspace)
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let target = app.delete_target.take();
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            target.map(Action::RemoveFromList)
        }
        KeyCode::Esc => {
            app.delete_target = None;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            None
        }
        _ => None,
    }
}

fn handle_commit_message_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char(c) => {
            if !c.is_control() {
                app.commit_msg_buffer.push(c);
            }
        }
        KeyCode::Backspace => {
            app.commit_msg_buffer.pop();
        }
        KeyCode::Enter => {
            let message = app.commit_msg_buffer.clone();
            if message.is_empty() {
                app.status_message = Some("Commit message cannot be empty".into());
                return None;
            }
            app.commit_msg_buffer.clear();
            app.mode = AppMode::Normal;
            return Some(Action::GitCommit(message));
        }
        KeyCode::Esc => {
            app.commit_msg_buffer.clear();
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
    None
}

fn handle_confirm_merge_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('m') => {
            app.mode = AppMode::Normal;
            Some(Action::GitMerge(MergeStrategy::Merge))
        }
        KeyCode::Char('r') => {
            app.mode = AppMode::Normal;
            Some(Action::GitMerge(MergeStrategy::Rebase))
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

fn handle_new_tab_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('1') => {
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Claude))
        }
        KeyCode::Char('2') => {
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Gemini))
        }
        KeyCode::Char('3') => {
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Codex))
        }
        KeyCode::Char('4') => {
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Shell))
        }
        KeyCode::Char('5') => {
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Kanban))
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

