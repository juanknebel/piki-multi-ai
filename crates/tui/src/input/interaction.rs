use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::clipboard;
use crate::dialog_state::DialogState;
use crate::helpers::copy_visible_terminal;

pub(super) fn handle_kanban_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
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
                edit.focus = edit.focus.next();
                edit.cursor_pos = edit.current_text().len();
            }
            KeyCode::Enter => {
                let card_id = edit.card_id.clone();
                let title = edit.title.clone();
                let description = edit.description.clone();
                let priority = edit.priority;
                if let Err(e) =
                    kanban_provider.update_card(&card_id, &title, &description, priority)
                {
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
            KeyCode::Home => {
                if edit.focus != flow::app::EditFocus::Priority {
                    edit.cursor_pos = 0;
                }
            }
            KeyCode::End => {
                if edit.focus != flow::app::EditFocus::Priority {
                    edit.cursor_pos = edit.current_text().len();
                }
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
                            priority: flow::Priority::Medium,
                            cursor_pos: 8,
                            focus: flow::app::EditFocus::Title,
                        });
                    }
                    Err(e) => {
                        kanban_app.banner = Some(format!("Create failed: {}", e));
                    }
                }
            }
            flow::Action::Edit => {
                let col = kanban_app.board.columns.get(kanban_app.col)?;
                let Some(card) = col.cards.get(kanban_app.row) else {
                    kanban_app.banner = Some("Edit failed: no card selected".to_string());
                    return None;
                };
                kanban_app.edit_state = Some(flow::app::EditState {
                    card_id: card.id.clone(),
                    title: card.title.clone(),
                    description: card.description.clone(),
                    priority: card.priority,
                    cursor_pos: card.title.len(),
                    focus: flow::app::EditFocus::Title,
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
            flow::Action::Refresh => match kanban_provider.load_board() {
                Ok(b) => {
                    kanban_app.board = b;
                    kanban_app.clamp();
                    kanban_app.banner = Some("Refreshed".to_string());
                }
                Err(e) => {
                    kanban_app.banner = Some(format!("Refresh failed: {}", e));
                }
            },
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

pub(super) fn handle_terminal_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Terminal search overlay captures input when active
    if app.term_search.is_some() {
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
        return None;
    }

    if app.config.matches_interaction(key, "exit_interaction") {
        app.interacting = false;
        app.term_search = None;
        return None;
    }
    // Ctrl+Shift+F: open terminal search
    if app.config.matches_interaction(key, "search") {
        app.term_search = Some(crate::app::TermSearchState {
            query: String::new(),
            cursor: 0,
            matches: Vec::new(),
            current_match: 0,
        });
        return None;
    }
    // Ctrl+Shift+V: paste from clipboard
    if app.config.matches_interaction(key, "paste") {
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
    if app.config.matches_markdown(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
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

pub(super) fn handle_diff_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
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

pub(super) fn handle_workspace_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_workspace_list(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    if app.config.matches_workspace_list(key, "down")
        || app.config.matches_workspace_list(key, "down_alt")
    {
        app.select_next_sidebar_row();
    } else if app.config.matches_workspace_list(key, "up")
        || app.config.matches_workspace_list(key, "up_alt")
    {
        app.select_prev_sidebar_row();
    } else if app.config.matches_workspace_list(key, "select") {
        let items = app.sidebar_items();
        match items.get(app.selected_sidebar_row) {
            Some(crate::app::SidebarItem::GroupHeader { .. }) => {
                app.toggle_selected_group();
            }
            Some(crate::app::SidebarItem::Workspace { index }) => {
                app.switch_workspace(*index);
            }
            None => {}
        }
    } else if app.config.matches_workspace_list(key, "delete") {
        if !app.workspaces.is_empty()
            && let Some(ws_idx) = app.sidebar_row_to_workspace(app.selected_sidebar_row)
        {
            app.active_dialog = Some(DialogState::ConfirmDelete { target: ws_idx });
            app.mode = AppMode::ConfirmDelete;
        }
    } else if app.config.matches_navigation(key, "edit_workspace")
        && let Some(ws_idx) = app.sidebar_row_to_workspace(app.selected_sidebar_row)
        && let Some(ws) = app.workspaces.get(ws_idx)
    {
        let kanban = ws.kanban_path.clone().unwrap_or_default();
        let prompt = ws.prompt.clone();
        let group = ws.info.group.clone().unwrap_or_default();
        app.active_dialog = Some(DialogState::EditWorkspace {
            target: ws_idx,
            kanban_cursor: kanban.chars().count(),
            kanban,
            prompt_cursor: prompt.chars().count(),
            prompt,
            group_cursor: group.chars().count(),
            group,
            active_field: DialogField::KanbanPath,
        });
        app.mode = AppMode::EditWorkspace;
        app.interacting = false;
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
    if app.config.matches_interaction(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }

    let ws = app.workspaces.get_mut(app.active_workspace)?;
    let tab = ws.current_tab_mut()?;
    let api = tab.api_state.as_mut()?;

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

    // Ctrl+F: open search in response panel
    if key.code == KeyCode::Char('f')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
        && !api.responses.is_empty()
    {
        api.search = Some(crate::app::ApiSearchState {
            query: String::new(),
            cursor: 0,
            matches: Vec::new(),
            current_match: 0,
        });
        return None;
    }

    // Ctrl+S: send the request block at cursor
    if key.code == KeyCode::Char('s')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
    {
        let block_text = extract_block_at_cursor(&api.editor);
        return Some(Action::SendApiRequest(block_text));
    }

    // Ctrl+J: scroll response down
    if key.code == KeyCode::Char('j')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
    {
        api.response_scroll = api.response_scroll.saturating_add(1);
        return None;
    }

    // Ctrl+K: scroll response up
    if key.code == KeyCode::Char('k')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
    {
        api.response_scroll = api.response_scroll.saturating_sub(1);
        return None;
    }

    // Editor input
    match key.code {
        KeyCode::Up => api.editor.move_up(),
        KeyCode::Down => api.editor.move_down(),
        KeyCode::Left => api.editor.move_left(),
        KeyCode::Right => api.editor.move_right(),
        KeyCode::Enter => api.editor.enter(),
        KeyCode::Backspace => api.editor.backspace(),
        KeyCode::Char(c) => api.editor.insert_char(c),
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

pub(super) fn handle_filelist_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_file_list(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }

    // Project workspaces: navigate sub-directories, Enter opens NewWorkspace dialog
    let is_project = app
        .current_workspace()
        .is_some_and(|ws| ws.info.workspace_type == piki_core::WorkspaceType::Project);

    if is_project {
        if app.config.matches_file_list(key, "down")
            || app.config.matches_file_list(key, "down_alt")
        {
            app.next_file();
        } else if app.config.matches_file_list(key, "up")
            || app.config.matches_file_list(key, "up_alt")
        {
            app.prev_file();
        } else if key.code == KeyCode::Enter
            && let Some(ws) = app.current_workspace()
            && let Some(dir_name) = ws.sub_directories.get(app.selected_file)
        {
            let full_dir = ws.path.join(dir_name).display().to_string();
            let prompt = ws.prompt.clone();
            let kanban = ws.kanban_path.clone().unwrap_or_default();
            let group = ws.info.group.clone().unwrap_or_default();
            app.active_dialog = Some(DialogState::NewWorkspace {
                name: String::new(),
                name_cursor: 0,
                dir_cursor: full_dir.chars().count(),
                dir: full_dir,
                desc: String::new(),
                desc_cursor: 0,
                prompt_cursor: prompt.chars().count(),
                prompt,
                kanban_cursor: kanban.chars().count(),
                kanban,
                group_cursor: group.chars().count(),
                group,
                ws_type: piki_core::WorkspaceType::Simple,
                active_field: DialogField::Type,
            });
            app.mode = AppMode::NewWorkspace;
            app.interacting = false;
        }
        return None;
    }

    if app.config.matches_file_list(key, "down") || app.config.matches_file_list(key, "down_alt") {
        app.next_file();
    } else if app.config.matches_file_list(key, "up") || app.config.matches_file_list(key, "up_alt")
    {
        app.prev_file();
    } else if app.config.matches_file_list(key, "diff") {
        if let Some(ws) = app.current_workspace()
            && !ws.changed_files.is_empty()
        {
            return Some(Action::OpenDiff(app.selected_file));
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
        if let Some(ws) = app.current_workspace()
            && !ws.changed_files.is_empty()
        {
            return Some(Action::GitStage(app.selected_file));
        }
    } else if app.config.matches_file_list(key, "unstage")
        && let Some(ws) = app.current_workspace()
        && !ws.changed_files.is_empty()
    {
        return Some(Action::GitUnstage(app.selected_file));
    }
    None
}
