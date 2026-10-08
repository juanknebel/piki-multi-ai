use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{App, AppMode, ProjectTreeRow};
use crate::clipboard;
use crate::config::has_ctrl;
use crate::dialog_state::DialogState;
use crate::helpers::copy_visible_terminal;

pub(super) fn handle_kanban_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let (kanban_app, kanban_provider) = match (&mut ws.kanban_app, &mut ws.kanban_provider) {
        (Some(a), Some(p)) => (a, p),
        _ => return None,
    };

    // Helper to get selected card ID
    let selected_card_id = |a: &flow_tui::App| -> Option<String> {
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
                edit.focus = edit.focus.next();
                edit.cursor_pos = edit.current_text().len();
            }
            KeyCode::Enter => {
                if edit.title.trim().is_empty() {
                    kanban_app.banner = Some("Title is required".to_string());
                    edit.focus = flow_tui::app::EditFocus::Title;
                    edit.cursor_pos = 0;
                    return None;
                }
                if edit.project.trim().is_empty() {
                    kanban_app.banner = Some("Project is required".to_string());
                    edit.focus = flow_tui::app::EditFocus::Project;
                    edit.cursor_pos = 0;
                    return None;
                }
                let card_id = edit.card_id.clone();
                let title = edit.title.clone();
                let description = edit.description.clone();
                let priority = edit.priority;
                let assignee = edit.assignee.clone();
                let project = edit.project.clone();
                if let Err(e) = kanban_provider.update_card(
                    &card_id,
                    &title,
                    &description,
                    priority,
                    &assignee,
                    &project,
                ) {
                    kanban_app.banner = Some(format!("Save failed: {}", e));
                } else {
                    match kanban_provider.load_board() {
                        Ok(b) => {
                            kanban_app.board = b;
                            kanban_app.clamp();
                            kanban_app.banner = Some("Card saved".to_string());
                        }
                        Err(e) => kanban_app.banner = Some(format!("Reload failed: {}", e)),
                    }
                }
                kanban_app.edit_state = None;
            }
            KeyCode::Left => {
                edit.move_cursor_left();
            }
            KeyCode::Right => {
                edit.move_cursor_right();
            }
            KeyCode::Home if edit.focus != flow_tui::app::EditFocus::Priority => {
                edit.cursor_pos = 0;
            }
            KeyCode::End if edit.focus != flow_tui::app::EditFocus::Priority => {
                edit.cursor_pos = edit.current_text().len();
            }
            KeyCode::Delete => {
                edit.delete_curr();
            }
            KeyCode::Char(c) => {
                edit.insert_char(c);
            }
            KeyCode::Backspace => {
                edit.delete_prev();
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

    // Search mode: intercept keys while search overlay is active
    if kanban_app.search_state.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                kanban_app.search_state = None;
            }
            KeyCode::Char(c) => {
                if let Some(search) = kanban_app.search_state.as_mut() {
                    search.insert_char(c);
                }
                let matches = kanban_app.search_matches();
                if !matches.is_empty() {
                    let current = (kanban_app.col, kanban_app.row);
                    if !matches.contains(&current) {
                        kanban_app.col = matches[0].0;
                        kanban_app.row = matches[0].1;
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(search) = kanban_app.search_state.as_mut() {
                    search.delete_prev();
                }
                let matches = kanban_app.search_matches();
                if !matches.is_empty() {
                    let current = (kanban_app.col, kanban_app.row);
                    if !matches.contains(&current) {
                        kanban_app.col = matches[0].0;
                        kanban_app.row = matches[0].1;
                    }
                }
            }
            KeyCode::Down => {
                kanban_app.select_next_match();
            }
            KeyCode::Up => {
                kanban_app.select_prev_match();
            }
            _ => {}
        }
        return None;
    }

    // Project filter mode
    if kanban_app.project_filter_state.is_some() {
        match key.code {
            KeyCode::Esc => {
                kanban_app.project_filter_state = None;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(pf) = kanban_app.project_filter_state.as_mut()
                    && pf.cursor < pf.projects.len()
                {
                    pf.selected[pf.cursor] = !pf.selected[pf.cursor];
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(pf) = kanban_app.project_filter_state.as_mut()
                    && pf.cursor + 1 < pf.projects.len()
                {
                    pf.cursor += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(pf) = kanban_app.project_filter_state.as_mut()
                    && pf.cursor > 0
                {
                    pf.cursor -= 1;
                }
            }
            KeyCode::Char('a') => {
                if let Some(pf) = kanban_app.project_filter_state.as_mut() {
                    let all_selected = pf.selected.iter().all(|&s| s);
                    for s in pf.selected.iter_mut() {
                        *s = !all_selected;
                    }
                }
            }
            KeyCode::Tab => {
                // Apply filter and close
                if let Some(pf) = kanban_app.project_filter_state.take() {
                    let selected: Vec<String> = pf
                        .projects
                        .iter()
                        .zip(pf.selected.iter())
                        .filter(|(_, sel)| **sel)
                        .map(|(name, _)| name.clone())
                        .collect();
                    kanban_app.project_filter = selected;
                    kanban_app
                        .board
                        .apply_project_filter(&kanban_app.project_filter);
                    kanban_app.clamp();
                }
            }
            _ => {}
        }
        return None;
    }

    // Dispatch agent: extract card data before borrowing app for dialog
    if key.code == KeyCode::Char('D') {
        let card_data = kanban_app
            .board
            .columns
            .get(kanban_app.col)
            .and_then(|col| col.cards.get(kanban_app.row))
            .map(|card| {
                (
                    card.id.clone(),
                    card.title.clone(),
                    card.description.clone(),
                    card.priority,
                    card.project.clone(),
                )
            });
        let source_ws = app.active_workspace;
        if let Some((card_id, card_title, card_description, card_priority, card_project)) =
            card_data
        {
            // Load & snapshot configured agents for this project
            if let Some(ref storage) = app.storage.agent_profiles
                && let Some(ws) = app.current_workspace()
            {
                let repo = ws.source_repo.clone();
                if let Ok(agents) = storage.load_agents(&repo) {
                    app.agent_profiles = agents;
                }
            }
            let agents: Vec<(String, String, String)> = app
                .agent_profiles
                .iter()
                .map(|a| (a.name.clone(), a.provider.clone(), a.role.clone()))
                .collect();
            app.active_dialog = Some(DialogState::DispatchAgent {
                source_ws,
                card_id,
                card_title,
                card_description,
                card_priority,
                card_project,
                agent_idx: 0,
                agents,
                additional_prompt: String::new(),
                additional_prompt_cursor: 0,
                step: 0,
                use_current_ws: false,
            });
            app.mode = AppMode::DispatchAgent;
        }
        return None;
    }

    let action = match key.code {
        KeyCode::Char('q') => Some(flow_tui::Action::Quit),
        KeyCode::Esc => Some(flow_tui::Action::CloseOrQuit),
        KeyCode::Char('h') | KeyCode::Left => Some(flow_tui::Action::FocusLeft),
        KeyCode::Char('l') | KeyCode::Right => Some(flow_tui::Action::FocusRight),
        KeyCode::Char('j') | KeyCode::Down => Some(flow_tui::Action::SelectDown),
        KeyCode::Char('k') | KeyCode::Up => Some(flow_tui::Action::SelectUp),
        KeyCode::Char('H') => Some(flow_tui::Action::MoveLeft),
        KeyCode::Char('L') => Some(flow_tui::Action::MoveRight),
        KeyCode::Enter => Some(flow_tui::Action::ToggleDetail),
        KeyCode::Char('r') => Some(flow_tui::Action::Refresh),
        KeyCode::Char('d') => Some(flow_tui::Action::Delete),
        KeyCode::Char('a') | KeyCode::Char('n') => Some(flow_tui::Action::Add),
        KeyCode::Char('e') => Some(flow_tui::Action::Edit),
        KeyCode::Char('s') => Some(flow_tui::Action::ToggleSort),
        KeyCode::Char('/') => Some(flow_tui::Action::Search),
        KeyCode::Char('p') => Some(flow_tui::Action::ProjectFilter),
        _ => None,
    };

    if let Some(a) = action {
        match a {
            flow_tui::Action::Add => {
                let Some(col) = kanban_app.board.columns.get(kanban_app.col) else {
                    kanban_app.banner = Some("Create failed: no column selected".to_string());
                    return None;
                };
                match kanban_provider.create_card(&col.id, "") {
                    Ok(id) => {
                        kanban_app.edit_state = Some(flow_tui::app::EditState {
                            card_id: id,
                            col_id: col.id.clone(),
                            is_new: true,
                            title: "New card".to_string(),
                            description: "".to_string(),
                            priority: flow_core::Priority::Medium,
                            assignee: "".to_string(),
                            project: "".to_string(),
                            cursor_pos: 8,
                            focus: flow_tui::app::EditFocus::Title,
                        });
                    }
                    Err(e) => {
                        kanban_app.banner = Some(format!("Create failed: {}", e));
                    }
                }
            }
            flow_tui::Action::Edit => {
                let col = kanban_app.board.columns.get(kanban_app.col)?;
                let Some(card) = col.cards.get(kanban_app.row) else {
                    kanban_app.banner = Some("Edit failed: no card selected".to_string());
                    return None;
                };
                kanban_app.edit_state = Some(flow_tui::app::EditState {
                    card_id: card.id.clone(),
                    col_id: col.id.clone(),
                    is_new: false,
                    title: card.title.clone(),
                    description: card.description.clone(),
                    priority: card.priority,
                    assignee: card.assignee.clone(),
                    project: card.project.clone(),
                    cursor_pos: card.title.len(),
                    focus: flow_tui::app::EditFocus::Title,
                });
            }
            flow_tui::Action::MoveLeft => {
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
            flow_tui::Action::MoveRight => {
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
            flow_tui::Action::Refresh => match kanban_provider.load_board() {
                Ok(b) => {
                    kanban_app.board = b;
                    kanban_app.clamp();
                    kanban_app.banner = Some("Refreshed".to_string());
                }
                Err(e) => {
                    kanban_app.banner = Some(format!("Refresh failed: {}", e));
                }
            },
            flow_tui::Action::ToggleSort => {
                kanban_app.apply(a);
                let label = kanban_app.sort_order.label();
                kanban_app.banner = Some(format!("Sorted by priority {}", label));
            }
            flow_tui::Action::Search => {
                kanban_app.search_state = Some(flow_tui::app::SearchState::new());
            }
            flow_tui::Action::ProjectFilter => {
                let mut all_projects = match kanban_provider.load_board() {
                    Ok(b) => b.projects(),
                    Err(_) => kanban_app.board.projects(),
                };
                let has_unassigned = kanban_app
                    .board
                    .columns
                    .iter()
                    .any(|c| c.cards.iter().any(|card| card.project.is_empty()));
                if has_unassigned {
                    all_projects.push(String::new());
                }
                if all_projects.is_empty() {
                    kanban_app.banner = Some("No projects found".to_string());
                } else {
                    let selected: Vec<bool> = all_projects
                        .iter()
                        .map(|p| {
                            kanban_app.project_filter.is_empty()
                                || kanban_app.project_filter.contains(p)
                        })
                        .collect();
                    kanban_app.project_filter_state = Some(flow_tui::app::ProjectFilterState {
                        projects: all_projects,
                        selected,
                        cursor: 0,
                    });
                }
            }
            _ => {
                // "Quit" from the kanban sub-app is a no-op now that there is
                // no interaction mode to leave.
                let _ = kanban_app.apply(a);
            }
        }
    }
    None
}

fn search_terminal(app: &mut App) {
    let search = match app.term_search.as_mut() {
        Some(s) if !s.query.is_empty() => s,
        _ => return,
    };
    search.matches.clear();
    search.current_match = 0;

    let ws = match app.workspaces.get(app.active_workspace) {
        Some(ws) => ws,
        None => return,
    };
    let tab = match ws.current_tab() {
        Some(t) => t,
        None => return,
    };
    let parser = match tab.pty_parser.as_ref() {
        Some(p) => p,
        None => return,
    };

    let guard = parser.lock();
    let screen = guard.screen();
    let query = &search.query;
    let rows = screen.size().0;
    let cols = screen.size().1;
    for row in 0..rows {
        // Build the row text from cell contents
        let mut line = String::with_capacity(cols as usize);
        for col in 0..cols {
            let cell = screen.cell(row, col);
            if let Some(cell) = cell {
                line.push_str(cell.contents());
            } else {
                line.push(' ');
            }
        }
        // Find all substring matches in this row
        let query_lower = query.to_lowercase();
        let line_lower = line.to_lowercase();
        let mut start = 0;
        while let Some(pos) = line_lower[start..].find(&query_lower) {
            search.matches.push((row as usize, start + pos));
            start += pos + 1;
        }
    }
}

/// Terminal search overlay input. Captures everything while the overlay is
/// open; dispatched from `handle_key_event` before the input-state machine so
/// the prefix key can be typed into the query.
pub(super) fn handle_term_search_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.term_search = None;
        }
        KeyCode::Enter => {
            if let Some(ref mut search) = app.term_search
                && !search.matches.is_empty()
            {
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::SHIFT)
                {
                    // Shift+Enter → previous match
                    search.current_match = if search.current_match == 0 {
                        search.matches.len() - 1
                    } else {
                        search.current_match - 1
                    };
                } else {
                    // Enter → next match
                    search.current_match = (search.current_match + 1) % search.matches.len();
                }
            }
        }
        KeyCode::Char(c) => {
            if let Some(ref mut search) = app.term_search {
                search.query.insert(
                    search
                        .query
                        .char_indices()
                        .nth(search.cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(search.query.len()),
                    c,
                );
                search.cursor += 1;
            }
            search_terminal(app);
        }
        KeyCode::Backspace => {
            if let Some(ref mut search) = app.term_search
                && search.cursor > 0
            {
                search.cursor -= 1;
                let byte_pos = search
                    .query
                    .char_indices()
                    .nth(search.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(search.query.len());
                search.query.remove(byte_pos);
            }
            search_terminal(app);
        }
        _ => {}
    }
    None
}

pub(super) fn handle_terminal_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Terminal search now lives behind the prefix (Ctrl+G f), dispatched as an
    // app action, so it can't collide with the emulator's own Ctrl+Shift+F.
    // Ctrl+Shift+V: paste from clipboard
    if app.config.matches_app_direct(key, "paste") {
        match clipboard::paste_from_clipboard() {
            Ok(text) => {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && let Some(tab) = ws.current_tab_mut()
                {
                    let bracketed = tab
                        .pty_parser
                        .as_ref()
                        .map(|p| p.lock().screen().bracketed_paste())
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
            Err(e) => {
                app.set_toast(
                    format!("Paste failed: {}", e),
                    crate::app::ToastLevel::Error,
                );
            }
        }
        return None;
    }
    // Ctrl+Shift+C: copy visible terminal content
    if app.config.matches_app_direct(key, "copy") {
        copy_visible_terminal(app);
        return None;
    }
    // Any other key means the user is typing into the terminal again: snap
    // the view back to the live bottom instead of leaving it stranded in
    // scrollback (e.g. after a mouse-wheel scroll up). Copy above is the
    // one exception — it reads the scrollback the user is looking at.
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        tab.term_scroll = 0;
    }
    // Forward all other keys to the active tab's PTY
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
        && let Some(ref mut pty) = tab.pty_session
        && let Some(bytes) = crate::pty::input::key_to_bytes(key)
    {
        let _ = pty.write(&bytes);
    }
    None
}

pub(super) fn handle_markdown_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        if app.config.matches_markdown(key, "down") || app.config.matches_markdown(key, "down_alt")
        {
            tab.markdown_scroll = tab.markdown_scroll.saturating_add(1);
        } else if app.config.matches_markdown(key, "up")
            || app.config.matches_markdown(key, "up_alt")
        {
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
    None
}

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "GRPC"];

/// Check if a line looks like a METHOD URL request line.
fn is_method_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return false;
    }
    if let Some((word, rest)) = trimmed.split_once(char::is_whitespace) {
        HTTP_METHODS.contains(&word.to_uppercase().as_str()) && !rest.trim().is_empty()
    } else {
        false
    }
}

/// Extract the request block text where the cursor is positioned.
/// Blocks are delimited by METHOD lines (GET, POST, etc.).
fn extract_block_at_cursor(editor: &crate::app::EditorState) -> String {
    let cursor_row = editor.cursor_row;
    let lines = &editor.lines;

    // Find all METHOD line indices
    let method_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_method_line(l))
        .map(|(i, _)| i)
        .collect();

    if method_indices.is_empty() {
        // No method lines found, send everything
        return editor.contents();
    }

    // Find the block containing the cursor:
    // block_start = last METHOD line at or before cursor
    // block_end = next METHOD line after cursor (or end of file)
    let block_start = method_indices
        .iter()
        .rev()
        .find(|&&i| i <= cursor_row)
        .copied()
        .unwrap_or(0);

    let block_end = method_indices
        .iter()
        .find(|&&i| i > cursor_row && i > block_start)
        .copied()
        .unwrap_or(lines.len());

    let block_lines = &lines[block_start..block_end];
    let mut s = block_lines.join("\n");
    s.push('\n');
    s
}

/// Search the API response body for matches, updating the search state.
fn search_api_response(api: &mut crate::app::ApiTabState) {
    let search = match api.search.as_mut() {
        Some(s) if !s.query.is_empty() => s,
        _ => return,
    };
    search.matches.clear();
    search.current_match = 0;

    let query_lower = search.query.to_lowercase();
    let mut global_line: usize = 0;

    for (idx, resp) in api.responses.iter().enumerate() {
        // Header line (matches the render_responses layout)
        global_line += 1;

        // Body lines
        for body_line in resp.body.lines() {
            let line_lower = body_line.to_lowercase();
            let mut start = 0;
            while let Some(pos) = line_lower[start..].find(&query_lower) {
                search.matches.push((global_line, start + pos));
                start += pos + 1;
            }
            global_line += 1;
        }

        // Separator between responses
        if idx + 1 < api.responses.len() {
            global_line += 1;
        }
    }

    // Auto-scroll to the first match
    if let Some(&(line, _)) = api.search.as_ref().and_then(|s| s.matches.first()) {
        api.response_scroll = line.saturating_sub(1) as u16;
    }
}

pub(super) fn handle_api_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Ctrl+C / Ctrl+Shift+C: copy entire response body
    // Ctrl+C is safe here (no PTY in API tab); Ctrl+Shift+C may be intercepted by the terminal
    if (key.code == KeyCode::Char('c')
        && has_ctrl(key.modifiers, app.config.platform)
        && !key.modifiers.contains(KeyModifiers::SHIFT))
        || app.config.matches_app_direct(key, "copy")
    {
        let text = app
            .workspaces
            .get(app.active_workspace)
            .and_then(|ws| ws.current_tab())
            .and_then(|tab| tab.api_state.as_ref())
            .map(|api| {
                api.responses
                    .iter()
                    .map(|r| r.body.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .unwrap_or_default();
        if text.is_empty() {
            app.set_toast("No response to copy", crate::app::ToastLevel::Info);
        } else {
            match clipboard::copy_to_clipboard(&text) {
                Ok(()) => {
                    app.set_toast("Response copied", crate::app::ToastLevel::Info);
                }
                Err(e) => {
                    app.set_toast(format!("Copy failed: {e}"), crate::app::ToastLevel::Error);
                }
            }
        }
        return None;
    }

    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let repo_path = ws.source_repo.clone();
    let tab = ws.current_tab_mut()?;
    let api = tab.api_state.as_mut()?;

    // History overlay captures input when active
    if api.history.is_some() {
        match key.code {
            KeyCode::Esc => {
                api.history = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut hist) = api.history {
                    hist.selected = hist.selected.saturating_sub(1);
                    if hist.selected < hist.scroll_offset {
                        hist.scroll_offset = hist.selected;
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut hist) = api.history {
                    if hist.selected + 1 < hist.entries.len() {
                        hist.selected += 1;
                    }
                    // Auto-scroll: keep 2 lines of context
                    let visible_height = 15_usize; // approximate
                    if hist.selected >= hist.scroll_offset + visible_height {
                        hist.scroll_offset = hist.selected.saturating_sub(visible_height - 1);
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(ref hist) = api.history
                    && let Some(entry) = hist.entries.get(hist.selected)
                {
                    // Load request into editor and response into panel
                    api.editor = crate::app::EditorState::new(&entry.request_text);
                    api.responses = vec![crate::app::ApiResponseDisplay {
                        status: entry.status,
                        elapsed_ms: entry.elapsed_ms,
                        body: entry.response_body.clone(),
                        headers: entry.response_headers.clone(),
                    }];
                    api.response_scroll = 0;
                }
                api.history = None;
            }
            KeyCode::Char('d') => {
                if let Some(ref mut hist) = api.history
                    && let Some(entry) = hist.entries.get(hist.selected)
                {
                    if let Some(id) = entry.id
                        && let Some(ref api_storage) = app.storage.api_history
                    {
                        let _ = api_storage.delete_api_entry(id);
                    }
                    hist.entries.remove(hist.selected);
                    if hist.selected >= hist.entries.len() && hist.selected > 0 {
                        hist.selected -= 1;
                    }
                    if hist.entries.is_empty() {
                        api.history = None;
                    }
                }
                return None;
            }
            KeyCode::Char('/') => {
                if let Some(ref mut hist) = api.history {
                    hist.searching = true;
                    hist.search_query.clear();
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut hist) = api.history
                    && hist.searching
                {
                    hist.search_query.pop();
                    // Re-search
                    if let Some(ref api_storage) = app.storage.api_history {
                        if hist.search_query.is_empty() {
                            if let Ok(entries) =
                                api_storage.load_recent_api_history(&repo_path, 100)
                            {
                                hist.entries = entries;
                            }
                        } else if let Ok(entries) =
                            api_storage.search_api_history(&repo_path, &hist.search_query, 100)
                        {
                            hist.entries = entries;
                        }
                        hist.selected = 0;
                        hist.scroll_offset = 0;
                    }
                }
                return None;
            }
            KeyCode::Char(c) => {
                if let Some(ref mut hist) = api.history
                    && hist.searching
                {
                    hist.search_query.push(c);
                    // Search via FTS
                    if let Some(ref api_storage) = app.storage.api_history
                        && let Ok(entries) =
                            api_storage.search_api_history(&repo_path, &hist.search_query, 100)
                    {
                        hist.entries = entries;
                        hist.selected = 0;
                        hist.scroll_offset = 0;
                    }
                }
                return None;
            }
            _ => {}
        }
        return None;
    }

    // Search overlay captures input when active
    if api.search.is_some() {
        match key.code {
            KeyCode::Esc => {
                api.search = None;
            }
            KeyCode::Enter => {
                if let Some(ref mut search) = api.search
                    && !search.matches.is_empty()
                {
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::SHIFT)
                    {
                        // Shift+Enter → previous match
                        search.current_match = if search.current_match == 0 {
                            search.matches.len() - 1
                        } else {
                            search.current_match - 1
                        };
                    } else {
                        // Enter → next match
                        search.current_match = (search.current_match + 1) % search.matches.len();
                    }
                    // Auto-scroll to current match
                    let line = search.matches[search.current_match].0;
                    api.response_scroll = line.saturating_sub(1) as u16;
                }
            }
            KeyCode::Char(c) => {
                if let Some(ref mut search) = api.search {
                    search.query.insert(
                        search
                            .query
                            .char_indices()
                            .nth(search.cursor)
                            .map(|(i, _)| i)
                            .unwrap_or(search.query.len()),
                        c,
                    );
                    search.cursor += 1;
                }
                search_api_response(api);
            }
            KeyCode::Backspace => {
                if let Some(ref mut search) = api.search
                    && search.cursor > 0
                {
                    search.cursor -= 1;
                    let byte_pos = search
                        .query
                        .char_indices()
                        .nth(search.cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(search.query.len());
                    search.query.remove(byte_pos);
                }
                search_api_response(api);
            }
            _ => {}
        }
        return None;
    }

    // jq filter bar captures input while open. Ctrl+J/Ctrl+K stay live so the
    // response can be scrolled without closing the bar first.
    if api.jq.is_some() {
        if key.code == KeyCode::Char('j') && has_ctrl(key.modifiers, app.config.platform) {
            api.response_scroll = api.response_scroll.saturating_add(1);
            return None;
        }
        if key.code == KeyCode::Char('k') && has_ctrl(key.modifiers, app.config.platform) {
            api.response_scroll = api.response_scroll.saturating_sub(1);
            return None;
        }
        if key.code == KeyCode::Char('u') && has_ctrl(key.modifiers, app.config.platform) {
            if let Some(ref mut jq) = api.jq {
                jq.query.clear();
                jq.cursor = 0;
                jq.error = None;
            }
            return None;
        }
        match key.code {
            KeyCode::Esc => {
                // Closing drops the filter: the panel goes back to the raw
                // responses, so what you see always matches what is on screen.
                api.jq = None;
                api.jq_output = None;
                api.response_scroll = 0;
            }
            KeyCode::Enter => {
                let query = api.jq.as_ref().map(|jq| jq.query.clone())?;
                return Some(Action::RunJqFilter(query));
            }
            // A Ctrl/Alt chord the bar doesn't handle is a shortcut, not
            // text: swallow it rather than typing its letter into the filter.
            KeyCode::Char(_)
                if key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) => {}
            _ => {
                if let Some(ref mut jq) = api.jq {
                    let mut query = std::mem::take(&mut jq.query);
                    let mut cursor = jq.cursor;
                    crate::input::text_field_common::handle_text_input(
                        &mut query,
                        &mut cursor,
                        key,
                        |_| true,
                    );
                    jq.query = query;
                    jq.cursor = cursor;
                }
            }
        }
        return None;
    }

    // Ctrl+Q (Cmd+Q on macOS): open the jq filter bar over the response
    if key.code == KeyCode::Char('q')
        && has_ctrl(key.modifiers, app.config.platform)
        && !api.responses.is_empty()
    {
        api.search = None; // one bar at a time — they share the same row
        api.jq = Some(crate::app::ApiJqState {
            query: String::new(),
            cursor: 0,
            error: None,
            running: false,
        });
        return None;
    }

    // Ctrl+F (Cmd+F on macOS): open search in response panel
    if key.code == KeyCode::Char('f')
        && has_ctrl(key.modifiers, app.config.platform)
        && !api.responses.is_empty()
    {
        api.jq = None;
        api.search = Some(crate::app::ApiSearchState {
            query: String::new(),
            cursor: 0,
            matches: Vec::new(),
            current_match: 0,
        });
        return None;
    }

    // Ctrl+H (Cmd+H on macOS): open API history overlay
    if key.code == KeyCode::Char('h') && has_ctrl(key.modifiers, app.config.platform) {
        if let Some(ref api_storage) = app.storage.api_history {
            let repo = app
                .workspaces
                .get(app.active_workspace)
                .map(|ws| ws.source_repo.clone());
            if let Some(repo) = repo
                && let Ok(entries) = api_storage.load_recent_api_history(&repo, 100)
            {
                let ws = app.workspaces.get_mut(app.active_workspace)?;
                let tab = ws.current_tab_mut()?;
                let api = tab.api_state.as_mut()?;
                api.history = Some(crate::app::ApiHistoryState {
                    entries,
                    selected: 0,
                    scroll_offset: 0,
                    search_query: String::new(),
                    searching: false,
                });
            }
        }
        return None;
    }

    // Ctrl+S (Cmd+S on macOS): send the request block at cursor
    if key.code == KeyCode::Char('s') && has_ctrl(key.modifiers, app.config.platform) {
        let block_text = extract_block_at_cursor(&api.editor);
        return Some(Action::SendApiRequest(block_text));
    }

    // Ctrl+J (Cmd+J on macOS): scroll response down
    if key.code == KeyCode::Char('j') && has_ctrl(key.modifiers, app.config.platform) {
        api.response_scroll = api.response_scroll.saturating_add(1);
        return None;
    }

    // Ctrl+K (Cmd+K on macOS): scroll response up
    if key.code == KeyCode::Char('k') && has_ctrl(key.modifiers, app.config.platform) {
        api.response_scroll = api.response_scroll.saturating_sub(1);
        return None;
    }

    // Editor input — skip chars with Ctrl/Alt modifiers (those are shortcuts, not text input)
    match key.code {
        KeyCode::Up => api.editor.move_up(),
        KeyCode::Down => api.editor.move_down(),
        KeyCode::Left => api.editor.move_left(),
        KeyCode::Right => api.editor.move_right(),
        KeyCode::Enter => api.editor.enter(),
        KeyCode::Backspace => api.editor.backspace(),
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            api.editor.insert_char(c);
        }
        KeyCode::Tab => {
            for _ in 0..4 {
                api.editor.insert_char(' ');
            }
        }
        _ => {}
    }

    // Keep cursor visible
    api.editor.adjust_scroll(20); // approximate visible height

    None
}

pub(super) fn handle_agents_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    let rows = app.agent_rows();
    if rows.is_empty() {
        return None;
    }
    // Clamp: tabs can close asynchronously between renders
    if app.selected_agent_row >= rows.len() {
        app.selected_agent_row = rows.len() - 1;
        app.reveal_agent_selection();
    }
    if app.config.matches_agents(key, "down") || app.config.matches_agents(key, "down_alt") {
        crate::input::list_nav::move_selection(&mut app.selected_agent_row, rows.len(), 1, false);
        app.reveal_agent_selection();
    } else if app.config.matches_agents(key, "up") || app.config.matches_agents(key, "up_alt") {
        crate::input::list_nav::move_selection(&mut app.selected_agent_row, rows.len(), -1, false);
        app.reveal_agent_selection();
    } else if app.config.matches_agents(key, "select") {
        jump_to_agent(app, rows[app.selected_agent_row]);
    }
    None
}

/// Keyboard navigation for the focused top-left pane: the project tree.
///
/// One row model, one handler — `[keybindings.projects]` holds every key. The
/// vertical keys move the cursor (follow-focus: landing on a checkout switches
/// to it), the horizontal ones collapse/expand a header or repo group, and the
/// row-scoped keys act on whatever the cursor stands on.
pub(super) fn handle_workspace_list_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    let cfg = &app.config;
    if cfg.matches_projects(key, "down") || cfg.matches_projects(key, "down_alt") {
        app.select_next_sidebar_row();
    } else if cfg.matches_projects(key, "up") || cfg.matches_projects(key, "up_alt") {
        app.select_prev_sidebar_row();
    } else if cfg.matches_projects(key, "collapse") || cfg.matches_projects(key, "collapse_alt") {
        app.collapse_selected_group();
    } else if cfg.matches_projects(key, "expand") || cfg.matches_projects(key, "expand_alt") {
        app.expand_selected_group();
    } else if cfg.matches_projects(key, "new") {
        super::dialog::open_project_editor_modal(app, None);
    } else if cfg.matches_projects(key, "select") {
        return activate_sidebar_row(app);
    } else if cfg.matches_projects(key, "edit") {
        let project = app.selected_project().cloned();
        match project {
            Some(p) => super::dialog::open_project_editor_modal(app, Some(p)),
            None => app.set_toast(NO_PROJECT_HERE, crate::app::ToastLevel::Info),
        }
    } else if cfg.matches_projects(key, "delete") {
        match app.selected_project().and_then(|p| p.id) {
            Some(id) => return Some(Action::DeleteProject(id)),
            None => app.set_toast(NO_PROJECT_HERE, crate::app::ToastLevel::Info),
        }
    } else if cfg.matches_projects(key, "add_repo") {
        // The New Workspace dialog does the work (local folder or GitHub
        // clone); the project it lands in is remembered until creation
        // finishes — see `App::pending_project_member`.
        match app.selected_project().and_then(|p| p.id) {
            Some(id) => {
                app.pending_project_member = Some(id);
                return super::app_actions::open_new_workspace(app);
            }
            None => app.set_toast(NO_PROJECT_HERE, crate::app::ToastLevel::Info),
        }
    } else if cfg.matches_projects(key, "new_worktree") {
        return new_worktree_for_selected_repo(app);
    }
    None
}

/// Shown when a project-scoped key lands on a synthetic bucket header (PR
/// review / no project), which has no stored project behind it.
const NO_PROJECT_HERE: &str = "Not a project row";

/// `select` (Enter) on the row under the cursor: collapse/expand a header or
/// repo group, open a checkout, or adopt a plain-directory member.
pub(super) fn activate_sidebar_row(app: &mut App) -> Option<Action> {
    let row = app.sidebar_rows().get(app.selected_sidebar_row)?.clone();
    match row {
        ProjectTreeRow::Project { .. } | ProjectTreeRow::Repo { .. } => {
            app.toggle_selected_group();
            None
        }
        ProjectTreeRow::Checkout {
            workspace_index: idx,
            ..
        } => {
            // A checkout with tabs open takes the focus with it; an empty one
            // just becomes active, so the user can open a tab in it.
            if app
                .workspaces
                .get(idx)
                .is_some_and(|ws| !ws.tabs.is_empty())
            {
                app.switch_workspace_and_focus(idx);
            } else {
                app.switch_workspace(idx);
            }
            if app.workspaces.get(idx).is_some_and(|ws| ws.review_broken) {
                return Some(Action::RetryReviewCheckout(idx));
            }
            None
        }
        // Membership stores only the path, so adopting the directory as a
        // workspace upgrades this row in place with no migration.
        ProjectTreeRow::Dir { path, .. } => Some(Action::ProjectAdoptDirectory { path }),
    }
}

/// `new_worktree`: create a branch + worktree in the repository under the
/// cursor. Works from a repo row and from any checkout of that repo, since
/// both identify the same repository; the new worktree joins the project on
/// its own (the tree adopts every worktree of a member repo).
fn new_worktree_for_selected_repo(app: &mut App) -> Option<Action> {
    let row = app.sidebar_rows().get(app.selected_sidebar_row)?.clone();
    let repo = match &row {
        ProjectTreeRow::Repo { root, .. } => root.clone(),
        ProjectTreeRow::Checkout {
            workspace_index, ..
        } => app
            .workspaces
            .get(*workspace_index)?
            .info
            .source_repo
            .clone(),
        _ => {
            app.set_toast("Select a repository first", crate::app::ToastLevel::Info);
            return None;
        }
    };
    // The create-worktree dialog works off a loaded checkout of the repo (it
    // reads its origin, prompt and kanban path), so a repo nobody has opened
    // yet has nothing to branch from.
    match app
        .workspaces
        .iter()
        .position(|w| w.info.source_repo == repo && !w.info.ephemeral)
    {
        Some(parent) => super::app_actions::open_create_worktree_for(app, parent),
        None => {
            app.set_toast(
                "Open this repository before branching it",
                crate::app::ToastLevel::Info,
            );
            None
        }
    }
}

/// Focus the given (workspace, tab) pair from the Agents pane.
pub(super) fn jump_to_agent(app: &mut App, (ws_idx, tab_idx): (usize, usize)) {
    // Defensive: a mouse jump could arrive while in terminal scroll mode
    app.input_state = crate::app::InputState::Normal;
    app.switch_workspace_and_focus(ws_idx);
    // After switch_workspace, which resets the previous tab's scroll
    if let Some(ws) = app.workspaces.get_mut(ws_idx)
        && tab_idx < ws.tabs.len()
    {
        ws.active_tab = tab_idx;
        ws.tabs[tab_idx].term_scroll = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{add_terminal_tab, add_test_workspace, key, test_app};

    /// Seed the sidebar's project list directly (the JSON test storage has no
    /// project backend, so `reload_sidebar_projects` would clear it).
    fn seed_project(app: &mut App, member_paths: Vec<std::path::PathBuf>) {
        app.sidebar_projects = vec![piki_core::projects::Project {
            id: Some(1),
            name: "frontend".to_string(),
            color: 0,
            order: 0,
            members: member_paths
                .into_iter()
                .map(piki_core::projects::ProjectMember::new)
                .collect(),
        }];
    }

    /// Enter on a project header collapses it; on a checkout it opens that
    /// workspace.
    #[test]
    fn enter_collapses_a_project_then_opens_a_checkout() {
        let mut app = test_app();
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        app.active_workspace = a;
        // The shared fixture gives every workspace the same path — members
        // resolve BY path, so give b its own.
        app.workspaces[b].info.path = std::path::PathBuf::from("/tmp/test-b");
        let b_path = app.workspaces[b].info.path.clone();
        seed_project(&mut app, vec![b_path]);

        // Rows: [project, b, bucket, a] — one checkout per repo, both hoisted.
        assert_eq!(app.sidebar_rows().len(), 4);
        app.selected_sidebar_row = 0;
        assert!(handle_workspace_list_interaction(&mut app, key(KeyCode::Enter)).is_none());
        assert_eq!(
            app.sidebar_rows().len(),
            3,
            "the collapsed project hides its checkout"
        );

        // Re-expand, walk onto b's checkout row, open it.
        handle_workspace_list_interaction(&mut app, key(KeyCode::Enter));
        app.selected_sidebar_row = 1;
        assert!(handle_workspace_list_interaction(&mut app, key(KeyCode::Enter)).is_none());
        assert_eq!(app.active_workspace, b);
    }

    /// Enter on a plain-directory member adopts it as a workspace.
    #[test]
    fn enter_adopts_an_unregistered_directory_member() {
        let mut app = test_app();
        seed_project(&mut app, vec![std::path::PathBuf::from("/tmp/free-dir")]);
        // Rows: [project, dir].
        app.selected_sidebar_row = 1;
        let action = handle_workspace_list_interaction(&mut app, key(KeyCode::Enter));
        assert!(matches!(
            action,
            Some(Action::ProjectAdoptDirectory { path }) if path == std::path::Path::new("/tmp/free-dir")
        ));
    }

    /// Delete on any row targets the project that row belongs to.
    #[test]
    fn delete_targets_the_rows_project() {
        let mut app = test_app();
        seed_project(&mut app, vec![std::path::PathBuf::from("/tmp/free-dir")]);
        app.selected_sidebar_row = 1; // the member row, not the header
        let action = handle_workspace_list_interaction(&mut app, key(KeyCode::Char('d')));
        assert!(matches!(action, Some(Action::DeleteProject(1))));
    }

    /// A synthetic bucket has no stored project, so the project-scoped keys
    /// say so instead of acting on whatever happens to be nearby.
    #[test]
    fn project_keys_are_inert_on_a_synthetic_bucket() {
        let mut app = test_app();
        add_test_workspace(&mut app);
        // No projects: row 0 is the no-project bucket.
        app.selected_sidebar_row = 0;
        assert!(handle_workspace_list_interaction(&mut app, key(KeyCode::Char('d'))).is_none());
        assert!(handle_workspace_list_interaction(&mut app, key(KeyCode::Char('e'))).is_none());
        assert!(app.active_dialog.is_none(), "no editor opened");
    }

    /// `new_worktree` needs a repository under the cursor; on a bucket header
    /// it is a no-op rather than guessing one.
    #[test]
    fn new_worktree_needs_a_repository_row() {
        let mut app = test_app();
        add_test_workspace(&mut app);
        app.selected_sidebar_row = 0;
        assert!(handle_workspace_list_interaction(&mut app, key(KeyCode::Char('w'))).is_none());
        assert!(app.active_dialog.is_none());
    }

    /// Typing into the terminal snaps a wheel-scrolled view back to live:
    /// the keystroke reaches the shell, so the stranded scrollback offset
    /// must go instead of staying pinned up top.
    #[test]
    fn terminal_keypress_snaps_scrollback_to_live() {
        let mut app = test_app();
        let ws = add_test_workspace(&mut app);
        app.active_workspace = ws;
        add_terminal_tab(&mut app, ws);
        app.workspaces[ws]
            .current_tab_mut()
            .expect("terminal tab")
            .term_scroll = 25;

        handle_terminal_interaction(
            &mut app,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()),
        );

        assert_eq!(
            app.workspaces[ws]
                .current_tab()
                .expect("terminal tab")
                .term_scroll,
            0
        );
    }

    // ── API Explorer: the jq filter bar ──

    /// An API tab with one response, which is what the jq bar needs to open.
    fn api_tab_with_response(app: &mut App) -> usize {
        let ws = add_test_workspace(app);
        let idx = app.workspaces[ws].add_tab(piki_core::AIProvider::Api, true, None);
        let mut api = crate::app::ApiTabState::new();
        api.responses = vec![crate::app::ApiResponseDisplay {
            status: 200,
            elapsed_ms: 3,
            body: "{\"a\":1}".to_string(),
            headers: String::new(),
        }];
        app.workspaces[ws].tabs[idx].api_state = Some(api);
        app.workspaces[ws].active_tab = idx;
        ws
    }

    fn api_of(app: &App) -> &crate::app::ApiTabState {
        app.workspaces[app.active_workspace]
            .current_tab()
            .expect("api tab")
            .api_state
            .as_ref()
            .expect("api state")
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn jq_bar_opens_types_and_runs() {
        let mut app = test_app();
        api_tab_with_response(&mut app);

        handle_api_interaction(&mut app, ctrl('q'));
        assert!(api_of(&app).jq.is_some(), "the bar should be open");

        for c in ".a".chars() {
            handle_api_interaction(&mut app, key(KeyCode::Char(c)));
        }
        assert_eq!(api_of(&app).jq.as_ref().unwrap().query, ".a");

        let action = handle_api_interaction(&mut app, key(KeyCode::Enter));
        match action {
            Some(Action::RunJqFilter(filter)) => assert_eq!(filter, ".a"),
            other => panic!("expected RunJqFilter, got {other:?}"),
        }
    }

    /// Esc drops the filter as well as the bar, so what is on screen always
    /// matches what the panel says it is showing.
    #[test]
    fn jq_esc_closes_and_restores_the_raw_response() {
        let mut app = test_app();
        api_tab_with_response(&mut app);
        handle_api_interaction(&mut app, ctrl('q'));
        app.workspaces[0]
            .current_tab_mut()
            .unwrap()
            .api_state
            .as_mut()
            .unwrap()
            .jq_output = Some(vec!["1".to_string()]);

        handle_api_interaction(&mut app, key(KeyCode::Esc));

        assert!(api_of(&app).jq.is_none());
        assert!(api_of(&app).jq_output.is_none());
    }

    #[test]
    fn jq_ctrl_u_clears_the_query() {
        let mut app = test_app();
        api_tab_with_response(&mut app);
        handle_api_interaction(&mut app, ctrl('q'));
        handle_api_interaction(&mut app, key(KeyCode::Char('.')));

        handle_api_interaction(&mut app, ctrl('u'));

        let jq = api_of(&app).jq.as_ref().unwrap();
        assert!(jq.query.is_empty());
        assert_eq!(jq.cursor, 0);
    }

    /// The bar owns the keyboard while it is open, but a chord it does not
    /// implement must not end up typed into the filter.
    #[test]
    fn jq_bar_swallows_chords_instead_of_typing_them() {
        let mut app = test_app();
        api_tab_with_response(&mut app);
        handle_api_interaction(&mut app, ctrl('q'));

        handle_api_interaction(&mut app, ctrl('f'));

        assert!(api_of(&app).jq.is_some(), "the bar stays open");
        assert!(
            api_of(&app).jq.as_ref().unwrap().query.is_empty(),
            "ctrl-f must not type an 'f'"
        );
        assert!(api_of(&app).search.is_none());
    }

    /// C-j / C-k keep scrolling the response while the filter bar has focus.
    #[test]
    fn jq_bar_leaves_the_scroll_chords_live() {
        let mut app = test_app();
        api_tab_with_response(&mut app);
        handle_api_interaction(&mut app, ctrl('q'));

        handle_api_interaction(&mut app, ctrl('j'));
        assert_eq!(api_of(&app).response_scroll, 1);
        handle_api_interaction(&mut app, ctrl('k'));
        assert_eq!(api_of(&app).response_scroll, 0);
        assert!(api_of(&app).jq.as_ref().unwrap().query.is_empty());
    }

    /// Nothing to filter yet: the chord must not open a bar over an empty
    /// panel.
    #[test]
    fn jq_bar_does_not_open_without_a_response() {
        let mut app = test_app();
        let ws = add_test_workspace(&mut app);
        let idx = app.workspaces[ws].add_tab(piki_core::AIProvider::Api, true, None);
        app.workspaces[ws].tabs[idx].api_state = Some(crate::app::ApiTabState::new());
        app.workspaces[ws].active_tab = idx;

        handle_api_interaction(&mut app, ctrl('q'));

        assert!(api_of(&app).jq.is_none());
    }
}
