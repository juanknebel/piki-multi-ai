mod app;
mod diff;
mod pty;
mod ui;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;

use app::{AIProvider, ActivePane, App, AppMode, DialogField};
use pty::PtySession;
use workspace::{config as ws_config, FileWatcher, WorkspaceManager};

const TICK_RATE: Duration = Duration::from_millis(50);
const DEBOUNCE: Duration = Duration::from_millis(500);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install panic hook that restores terminal before printing panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = ratatui::restore();
        original_hook(panic_info);
    }));

    let terminal = ratatui::init();
    let result = run(terminal).await;
    ratatui::restore();
    result
}

async fn run(mut terminal: DefaultTerminal) -> anyhow::Result<()> {
    let manager = WorkspaceManager::new();
    let mut app = App::new();

    // Restore persisted workspaces from all project configs
    let entries = ws_config::load_all();
    for entry in entries {
        let mut ws = app::Workspace::new(
            entry.name,
            entry.branch,
            entry.worktree_path,
            entry.source_repo,
        );

        // Spawn PTY for each AI provider
        spawn_all_providers(&mut ws).await;

        // Start file watcher
        match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
            Ok(watcher) => {
                ws.watcher = Some(watcher);
            }
            Err(e) => {
                app.status_message = Some(format!("Watcher error: {}", e));
            }
        }

        app.workspaces.push(ws);
    }
    if !app.workspaces.is_empty() {
        app.switch_workspace(0);
    }

    loop {
        // Render
        terminal.draw(|frame| {
            ui::layout::render(frame, &app);
        })?;

        // Poll for events with timeout (non-blocking for async tasks)
        if event::poll(TICK_RATE)? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = handle_key_event(&mut app, key) {
                        execute_action(&mut app, &manager, action).await?;
                    }
                }
                Event::Resize(_, _) => {} // ratatui handles resize automatically
                _ => {}
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
            // Check if active provider PTY process has exited
            if let Some(pty) = ws.pty_sessions.get_mut(&ws.active_provider) {
                if !pty.is_alive() {
                    ws.status = app::WorkspaceStatus::Done;
                }
            }
            // Debounced refresh of changed files list via git diff
            if ws.dirty {
                let should_refresh = ws
                    .last_refresh
                    .map(|t| now.duration_since(t) >= DEBOUNCE)
                    .unwrap_or(true);
                if should_refresh {
                    let _ = ws.refresh_changed_files().await;
                    ws.last_refresh = Some(now);
                }
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
    CreateWorkspace(String, PathBuf),
    DeleteWorkspace(usize),
    /// Open diff for the file at the given index in the active workspace
    OpenDiff(usize),
}

async fn execute_action(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(name, dir) => {
            match manager.create(&name, &dir).await {
                Ok(ws) => {
                    app.workspaces.push(ws);
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    // Spawn PTY for each AI provider
                    spawn_all_providers(&mut app.workspaces[new_idx]).await;

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
        Action::DeleteWorkspace(idx) => {
            if idx < app.workspaces.len() {
                // Kill all PTY sessions before removing
                for (_, pty) in app.workspaces[idx].pty_sessions.iter_mut() {
                    let _ = pty.kill();
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
        Action::OpenDiff(file_idx) => {
            if let Some(ws) = app.workspaces.get(app.active_workspace) {
                if let Some(file) = ws.changed_files.get(file_idx) {
                    let worktree_path = ws.path.clone();
                    let file_path = file.path.clone();
                    let file_status = file.status.clone();
                    // Use a reasonable width; TODO: pass actual panel width
                    let width = 120;
                    match diff::runner::run_diff(&worktree_path, &file_path, width, &file_status).await {
                        Ok(ansi_bytes) => {
                            use ansi_to_tui::IntoText;
                            match ansi_bytes.into_text() {
                                Ok(text) => {
                                    app.diff_content = Some(text);
                                    app.diff_file_path = Some(file_path);
                                    app.diff_scroll = 0;
                                    app.mode = AppMode::Diff;
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
    }
    Ok(())
}

/// Kill all PTY sessions and drop watchers for a clean exit.
fn shutdown(app: &mut App) {
    for ws in &mut app.workspaces {
        // Kill all provider PTY processes
        for (_, pty) in ws.pty_sessions.iter_mut() {
            let _ = pty.kill();
        }
        ws.pty_sessions.clear();
        // Drop watcher
        ws.watcher = None;
    }
}

/// Spawn a PTY session for each AI provider in a workspace.
async fn spawn_all_providers(ws: &mut app::Workspace) {
    for provider in AIProvider::all() {
        match PtySession::spawn(&ws.path, 24, 80, provider.command()).await {
            Ok(session) => {
                ws.pty_parsers.insert(*provider, Arc::clone(session.parser()));
                ws.pty_sessions.insert(*provider, session);
            }
            Err(_) => {
                // Provider not installed or failed to spawn — skip silently
            }
        }
    }
    if !ws.pty_sessions.is_empty() {
        ws.status = app::WorkspaceStatus::Busy;
    }
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Help overlay — any key closes it
    if app.mode == AppMode::Help {
        app.mode = AppMode::Normal;
        return None;
    }

    // New workspace dialog captures all input
    if app.mode == AppMode::NewWorkspace {
        return handle_new_workspace_input(app, key);
    }

    // Clear status message on any key
    app.status_message = None;

    if app.interacting {
        handle_interaction_mode(app, key)
    } else {
        handle_navigation_mode(app, key)
    }
}

// ── Navigation mode: hjkl between panes, Enter to interact, global shortcuts ──

fn handle_navigation_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        // Pane navigation with hjkl
        KeyCode::Char('h') | KeyCode::Left => {
            if app.active_pane == ActivePane::MainPanel {
                app.active_pane = ActivePane::FileList;
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if matches!(
                app.active_pane,
                ActivePane::WorkspaceList | ActivePane::FileList
            ) {
                app.active_pane = ActivePane::MainPanel;
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if app.active_pane == ActivePane::WorkspaceList {
                app.active_pane = ActivePane::FileList;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.active_pane == ActivePane::FileList {
                app.active_pane = ActivePane::WorkspaceList;
            }
        }
        // Enter the active pane
        KeyCode::Enter => {
            app.interacting = true;
        }
        // Global shortcuts
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('?') => {
            app.mode = AppMode::Help;
        }
        KeyCode::Char('n') => {
            app.mode = AppMode::NewWorkspace;
            app.input_buffer.clear();
            app.dir_input_buffer.clear();
            app.active_dialog_field = DialogField::Name;
        }
        KeyCode::Char('d') => {
            if !app.workspaces.is_empty() {
                return Some(Action::DeleteWorkspace(app.selected_workspace));
            }
        }
        KeyCode::Tab => {
            if key.modifiers == KeyModifiers::SHIFT {
                app.prev_workspace();
            } else {
                app.next_workspace();
            }
        }
        KeyCode::Char(c @ '1'..='9') => {
            let idx = (c as usize) - ('1' as usize);
            app.switch_workspace(idx);
        }
        // Cycle AI provider sub-tab
        KeyCode::Char('g') => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let providers = AIProvider::all();
                let current_idx = providers.iter().position(|p| *p == ws.active_provider).unwrap_or(0);
                ws.active_provider = providers[(current_idx + 1) % providers.len()];
            }
        }
        _ => {}
    }
    None
}

// ── Interaction mode: Esc to leave, keys go to the active pane ──

fn handle_interaction_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    match app.active_pane {
        ActivePane::MainPanel => {
            if app.mode == AppMode::Diff {
                handle_diff_interaction(app, key)
            } else {
                handle_terminal_interaction(app, key)
            }
        }
        ActivePane::WorkspaceList => handle_workspace_interaction(app, key),
        ActivePane::FileList => handle_filelist_interaction(app, key),
    }
}

fn handle_terminal_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if key.code == KeyCode::Esc {
        app.interacting = false;
        return None;
    }
    // Forward all other keys to the active provider's PTY
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
        let provider = ws.active_provider;
        if let Some(pty) = ws.pty_sessions.get_mut(&provider) {
            if let Some(bytes) = pty::input::key_to_bytes(key) {
                let _ = pty.write(&bytes);
            }
        }
    }
    None
}

fn handle_diff_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.diff_content = None;
            app.diff_file_path = None;
            app.interacting = false;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.diff_scroll = app.diff_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.diff_scroll = app.diff_scroll.saturating_sub(1);
        }
        KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
            app.diff_scroll = app.diff_scroll.saturating_add(20);
        }
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
            app.diff_scroll = app.diff_scroll.saturating_sub(20);
        }
        KeyCode::Char('g') => app.diff_scroll = 0,
        KeyCode::Char('G') => app.diff_scroll = u16::MAX,
        KeyCode::Char('n') => {
            app.next_file();
            return Some(Action::OpenDiff(app.selected_file));
        }
        KeyCode::Char('p') => {
            app.prev_file();
            return Some(Action::OpenDiff(app.selected_file));
        }
        _ => {}
    }
    None
}

fn handle_workspace_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.interacting = false;
        }
        KeyCode::Char('j') | KeyCode::Down => app.select_next_workspace(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev_workspace(),
        KeyCode::Enter => {
            app.switch_workspace(app.selected_workspace);
        }
        KeyCode::Char('d') => {
            if !app.workspaces.is_empty() {
                return Some(Action::DeleteWorkspace(app.selected_workspace));
            }
        }
        _ => {}
    }
    None
}

fn handle_filelist_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.interacting = false;
        }
        KeyCode::Char('j') | KeyCode::Down => app.next_file(),
        KeyCode::Char('k') | KeyCode::Up => app.prev_file(),
        KeyCode::Enter => {
            if let Some(ws) = app.current_workspace() {
                if !ws.changed_files.is_empty() {
                    return Some(Action::OpenDiff(app.selected_file));
                }
            }
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
                DialogField::Directory => DialogField::Name,
            };
        }
        KeyCode::Char(c) => match app.active_dialog_field {
            DialogField::Name => {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    app.input_buffer.push(c);
                }
            }
            DialogField::Directory => {
                if !c.is_control() {
                    app.dir_input_buffer.push(c);
                }
            }
        },
        KeyCode::Backspace => match app.active_dialog_field {
            DialogField::Name => {
                app.input_buffer.pop();
            }
            DialogField::Directory => {
                app.dir_input_buffer.pop();
            }
        },
        KeyCode::Enter => {
            let name = app.input_buffer.clone();
            let dir_raw = app.dir_input_buffer.clone();

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
            app.mode = AppMode::Normal;
            return Some(Action::CreateWorkspace(name, dir));
        }
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.dir_input_buffer.clear();
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
    None
}
