use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::clipboard;
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
                let Some(col) = kanban_app.board.columns.get(kanban_app.col) else {
                    return None;
                };
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

pub(super) fn handle_terminal_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
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
                if let Some(bytes) = crate::pty::input::key_to_bytes(key) {
                    let _ = pty.write(&bytes);
                }
            }
        }
    }
    None
}

pub(super) fn handle_markdown_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_markdown(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
        if let Some(tab) = ws.current_tab_mut() {
            if app.config.matches_markdown(key, "down")
                || app.config.matches_markdown(key, "down_alt")
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
        app.select_next_workspace();
    } else if app.config.matches_workspace_list(key, "up")
        || app.config.matches_workspace_list(key, "up_alt")
    {
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

pub(super) fn handle_filelist_interaction(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_file_list(key, "exit_interaction") {
        app.interacting = false;
        return None;
    }
    if app.config.matches_file_list(key, "down") || app.config.matches_file_list(key, "down_alt") {
        app.next_file();
    } else if app.config.matches_file_list(key, "up") || app.config.matches_file_list(key, "up_alt")
    {
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
