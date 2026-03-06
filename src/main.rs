mod app;
mod clipboard;
mod diff;
mod pty;
mod theme;
mod ui;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use app::{AIProvider, ActivePane, App, AppMode, DialogField};
use pty::PtySession;
use workspace::{FileWatcher, WorkspaceManager, config as ws_config};

const TICK_RATE: Duration = Duration::from_millis(50);
const DEBOUNCE: Duration = Duration::from_millis(500);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    app.theme = theme::load();

    // Compute real terminal dimensions for PTY spawning
    let term_size = terminal.size()?;
    let pty_area =
        ui::layout::compute_terminal_area(Rect::new(0, 0, term_size.width, term_size.height));
    app.pty_rows = pty_area.height;
    app.pty_cols = pty_area.width;

    // Restore persisted workspaces from all project configs
    let entries = ws_config::load_all();
    for entry in entries {
        let mut ws = app::Workspace::new(
            entry.name,
            entry.description,
            entry.branch,
            entry.worktree_path,
            entry.source_repo,
        );

        // Spawn PTY for each AI provider
        spawn_all_providers(&mut ws, app.pty_rows, app.pty_cols).await;

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
                Ok(Event::Mouse(mouse)) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                            && let Some(parser) = ws.pty_parsers.get(&ws.active_provider)
                        {
                            let max = scrollback_max(&parser);
                            ws.term_scroll = (ws.term_scroll + 3).min(max);
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                            ws.term_scroll = ws.term_scroll.saturating_sub(3);
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if app.mode == AppMode::Normal {
                            if let Some(inner) = app.terminal_inner_area {
                                if mouse.row >= inner.y
                                    && mouse.row < inner.y + inner.height
                                    && mouse.column >= inner.x
                                    && mouse.column < inner.x + inner.width
                                {
                                    let cell_row = mouse.row - inner.y;
                                    let cell_col = mouse.column - inner.x;
                                    app.selection =
                                        Some(app::Selection::new(cell_row, cell_col));
                                }
                            }
                        }
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if let Some(ref mut sel) = app.selection {
                            if let Some(inner) = app.terminal_inner_area {
                                let cell_row = mouse
                                    .row
                                    .saturating_sub(inner.y)
                                    .min(inner.height.saturating_sub(1));
                                let cell_col = mouse
                                    .column
                                    .saturating_sub(inner.x)
                                    .min(inner.width.saturating_sub(1));
                                sel.end_row = cell_row;
                                sel.end_col = cell_col;
                            }
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if let Some(ref mut sel) = app.selection {
                            sel.active = false;
                            let (sr, sc, er, ec) = sel.normalized();
                            // Only copy if non-empty selection
                            if sr != er || sc != ec {
                                if let Some(ws) =
                                    app.workspaces.get(app.active_workspace)
                                    && let Some(parser) =
                                        ws.pty_parsers.get(&ws.active_provider)
                                {
                                    let mut guard = parser.lock().unwrap();
                                    guard.screen_mut().set_scrollback(ws.term_scroll);
                                    let text = guard.screen().contents_between(
                                        sr, sc, er, ec + 1,
                                    );
                                    guard.screen_mut().set_scrollback(0);
                                    if let Err(e) = clipboard::copy_to_clipboard(&text) {
                                        app.status_message =
                                            Some(format!("Copy failed: {}", e));
                                    } else {
                                        app.status_message =
                                            Some("Selection copied".into());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Ok(Event::Resize(cols, rows)) => {
                    let new_area = ui::layout::compute_terminal_area(Rect::new(0, 0, cols, rows));
                    app.pty_rows = new_area.height;
                    app.pty_cols = new_area.width;
                    // Resize all PTY sessions in all workspaces
                    for ws in &mut app.workspaces {
                        for (_, pty) in ws.pty_sessions.iter_mut() {
                            let _ = pty.resize(new_area.height, new_area.width);
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
            // Check if active provider PTY process has exited
            if let Some(pty) = ws.pty_sessions.get_mut(&ws.active_provider) {
                if !pty.is_alive() {
                    ws.status = app::WorkspaceStatus::Done;
                }
                // Track bytes for status detection
                let current_bytes = pty.bytes_processed();
                if current_bytes != ws.last_bytes_processed {
                    ws.last_bytes_processed = current_bytes;
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
    CreateWorkspace(String, String, PathBuf),
    DeleteWorkspace(usize),
    /// Remove workspace from app list but keep worktree on disk
    RemoveFromList(usize),
    /// Open diff for the file at the given index in the active workspace
    OpenDiff(usize),
    /// Open $EDITOR for a file path
    OpenEditor(PathBuf),
}

async fn execute_action(
    app: &mut App,
    manager: &WorkspaceManager,
    action: Action,
    terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::CreateWorkspace(name, description, dir) => {
            match manager.create(&name, &description, &dir).await {
                Ok(ws) => {
                    app.workspaces.push(ws);
                    let new_idx = app.workspaces.len() - 1;
                    app.switch_workspace(new_idx);

                    // Spawn PTY for each AI provider
                    spawn_all_providers(&mut app.workspaces[new_idx], app.pty_rows, app.pty_cols)
                        .await;

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
        Action::RemoveFromList(idx) => {
            if idx < app.workspaces.len() {
                // Kill all PTY sessions
                for (_, pty) in app.workspaces[idx].pty_sessions.iter_mut() {
                    let _ = pty.kill();
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
async fn spawn_all_providers(ws: &mut app::Workspace, rows: u16, cols: u16) {
    for provider in AIProvider::all() {
        let cmd = provider.resolved_command();
        match PtySession::spawn(&ws.path, rows, cols, &cmd).await {
            Ok(session) => {
                ws.pty_parsers
                    .insert(*provider, Arc::clone(session.parser()));
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
        && let Some(parser) = ws.pty_parsers.get(&ws.active_provider)
    {
        let mut guard = parser.lock().unwrap();
        guard.screen_mut().set_scrollback(ws.term_scroll);
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

fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Help overlay — any key closes it
    if app.mode == AppMode::Help {
        app.mode = AppMode::Normal;
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
            app.desc_input_buffer.clear();
            app.active_dialog_field = DialogField::Name;
        }
        KeyCode::Char('d') => {
            if !app.workspaces.is_empty() {
                app.delete_target = Some(app.selected_workspace);
                app.mode = AppMode::ConfirmDelete;
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
        // Scrollback: Shift+K / PageUp = scroll up, Shift+J / PageDown = scroll down
        KeyCode::Char('K') => {
            if app.active_pane == ActivePane::MainPanel
                && app.mode == AppMode::Normal
                && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(parser) = ws.pty_parsers.get(&ws.active_provider)
            {
                let max = scrollback_max(parser);
                ws.term_scroll = (ws.term_scroll + 3).min(max);
            }
        }
        KeyCode::Char('J') => {
            if app.active_pane == ActivePane::MainPanel
                && app.mode == AppMode::Normal
                && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            {
                ws.term_scroll = ws.term_scroll.saturating_sub(3);
            }
        }
        KeyCode::PageUp => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(parser) = ws.pty_parsers.get(&ws.active_provider)
            {
                let screen_height = app.pty_rows as usize;
                let max = scrollback_max(parser);
                ws.term_scroll = (ws.term_scroll + screen_height).min(max);
            }
        }
        KeyCode::PageDown => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let screen_height = app.pty_rows as usize;
                ws.term_scroll = ws.term_scroll.saturating_sub(screen_height);
            }
        }
        // Ctrl+Shift+C: copy visible terminal content
        KeyCode::Char('C')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            copy_visible_terminal(app);
        }
        // Fuzzy search (/ like vim, or Ctrl+f)
        KeyCode::Char('/') => {
            app.open_fuzzy_search();
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.open_fuzzy_search();
        }
        // Cycle AI provider sub-tab
        KeyCode::Char('g') => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                let providers = AIProvider::all();
                let current_idx = providers
                    .iter()
                    .position(|p| *p == ws.active_provider)
                    .unwrap_or(0);
                ws.active_provider = providers[(current_idx + 1) % providers.len()];
                ws.term_scroll = 0;
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
    if key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.interacting = false;
        return None;
    }
    // Ctrl+Shift+V: paste from clipboard
    if key.code == KeyCode::Char('V')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && key.modifiers.contains(KeyModifiers::SHIFT)
    {
        match clipboard::paste_from_clipboard() {
            Ok(text) => {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                    let provider = ws.active_provider;
                    if let Some(pty) = ws.pty_sessions.get_mut(&provider) {
                        let bracketed = ws
                            .pty_parsers
                            .get(&provider)
                            .map(|p| p.lock().unwrap().screen().bracketed_paste())
                            .unwrap_or(false);
                        let data = if bracketed {
                            format!("\x1b[200~{}\x1b[201~", text)
                        } else {
                            text
                        };
                        let _ = pty.write(data.as_bytes());
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
    if key.code == KeyCode::Char('C')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && key.modifiers.contains(KeyModifiers::SHIFT)
    {
        copy_visible_terminal(app);
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
    if key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        app.mode = AppMode::Normal;
        app.diff_content = None;
        app.diff_file_path = None;
        app.interacting = false;
        return None;
    }
    match key.code {
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
    if key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.interacting = false;
        return None;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next_workspace(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev_workspace(),
        KeyCode::Enter => {
            app.switch_workspace(app.selected_workspace);
        }
        KeyCode::Char('d') => {
            if !app.workspaces.is_empty() {
                app.delete_target = Some(app.selected_workspace);
                app.mode = AppMode::ConfirmDelete;
            }
        }
        _ => {}
    }
    None
}

fn handle_filelist_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.interacting = false;
        return None;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.next_file(),
        KeyCode::Char('k') | KeyCode::Up => app.prev_file(),
        KeyCode::Enter => {
            if let Some(ws) = app.current_workspace() {
                if !ws.changed_files.is_empty() {
                    return Some(Action::OpenDiff(app.selected_file));
                }
            }
        }
        KeyCode::Char('e') => {
            if let Some(ws) = app.current_workspace()
                && let Some(file) = ws.changed_files.get(app.selected_file)
            {
                let full_path = ws.path.join(&file.path);
                return Some(Action::OpenEditor(full_path));
            }
        }
        KeyCode::Char('v') => {
            if let Some(ws) = app.current_workspace()
                && let Some(file) = ws.changed_files.get(app.selected_file)
            {
                let full_path = ws.path.join(&file.path);
                app.open_inline_editor(full_path);
            }
        }
        _ => {}
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

fn handle_new_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.active_dialog_field = match app.active_dialog_field {
                DialogField::Name => DialogField::Directory,
                DialogField::Directory => DialogField::Description,
                DialogField::Description => DialogField::Name,
            };
        }
        KeyCode::Char(c) => match app.active_dialog_field {
            DialogField::Name => {
                // Allow characters valid in git branch names: alphanumeric, '-', '_', '.', '/'
                if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/' {
                    app.input_buffer.push(c);
                }
            }
            DialogField::Directory => {
                if !c.is_control() {
                    app.dir_input_buffer.push(c);
                }
            }
            DialogField::Description => {
                if !c.is_control() {
                    app.desc_input_buffer.push(c);
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
            DialogField::Description => {
                app.desc_input_buffer.pop();
            }
        },
        KeyCode::Enter => {
            let name = app.input_buffer.clone();
            let dir_raw = app.dir_input_buffer.clone();
            let description = app.desc_input_buffer.clone();

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
            app.mode = AppMode::Normal;
            return Some(Action::CreateWorkspace(name, description, dir));
        }
        _ if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('g')
                && key.modifiers.contains(KeyModifiers::CONTROL)) =>
        {
            app.input_buffer.clear();
            app.dir_input_buffer.clear();
            app.desc_input_buffer.clear();
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
    None
}

fn handle_confirm_delete_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let target = app.delete_target.take();
            app.mode = AppMode::Normal;
            target.map(Action::DeleteWorkspace)
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let target = app.delete_target.take();
            app.mode = AppMode::Normal;
            target.map(Action::RemoveFromList)
        }
        KeyCode::Esc => {
            app.delete_target = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}
