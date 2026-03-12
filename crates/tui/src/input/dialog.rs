use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use piki_core::{AIProvider, MergeStrategy};

/// Result of processing a text input key event
enum TextInputResult {
    /// Key was consumed by text editing
    Consumed,
    /// Key was not a text editing key
    NotConsumed,
}

/// Handle common text editing keys (Char, Backspace, Delete, Left, Right, Home, End).
/// `validator` returns true if the char should be inserted.
fn handle_text_input(
    buf: &mut String,
    cursor: &mut usize,
    key: KeyEvent,
    validator: impl Fn(char) -> bool,
) -> TextInputResult {
    match key.code {
        KeyCode::Char(c) => {
            if validator(c) {
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor -= 1;
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.remove(byte_idx);
            }
            TextInputResult::Consumed
        }
        KeyCode::Delete => {
            if let Some((byte_idx, _)) = buf.char_indices().nth(*cursor) {
                buf.remove(byte_idx);
            }
            TextInputResult::Consumed
        }
        KeyCode::Left => {
            if *cursor > 0 {
                *cursor -= 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Right => {
            let len = buf.chars().count();
            if *cursor < len {
                *cursor += 1;
            }
            TextInputResult::Consumed
        }
        KeyCode::Home => {
            *cursor = 0;
            TextInputResult::Consumed
        }
        KeyCode::End => {
            *cursor = buf.chars().count();
            TextInputResult::Consumed
        }
        _ => TextInputResult::NotConsumed,
    }
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

fn is_cancel(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
}

pub(super) fn handle_edit_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.active_dialog_field = match app.active_dialog_field {
                DialogField::KanbanPath => DialogField::Prompt,
                _ => DialogField::KanbanPath,
            };
            return None;
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
        _ if is_cancel(key) => {
            app.edit_target = None;
            app.kanban_input_buffer.clear();
            app.prompt_input_buffer.clear();
            app.kanban_input_cursor = 0;
            app.prompt_input_cursor = 0;
            app.mode = AppMode::Normal;
            return None;
        }
        _ => {}
    }

    let (buf, cursor) = dialog_buf_and_cursor(app);
    handle_text_input(buf, cursor, key, |c| !c.is_control());
    None
}

pub(super) fn handle_new_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.active_dialog_field = match app.active_dialog_field {
                DialogField::Name => DialogField::Directory,
                DialogField::Directory => DialogField::Description,
                DialogField::Description => DialogField::Prompt,
                DialogField::Prompt => DialogField::KanbanPath,
                DialogField::KanbanPath => DialogField::Name,
            };
            return None;
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
            return Some(Action::CreateWorkspace(
                name,
                description,
                prompt,
                kanban_path,
                dir,
            ));
        }
        _ if is_cancel(key) => {
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
            return None;
        }
        _ => {}
    }

    let field = app.active_dialog_field;
    let (buf, cursor) = dialog_buf_and_cursor(app);
    let validator = |c: char| -> bool {
        match field {
            DialogField::Name => c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/',
            _ => !c.is_control(),
        }
    };
    handle_text_input(buf, cursor, key, validator);
    None
}

pub(super) fn handle_confirm_close_tab_input(app: &mut App, key: KeyEvent) -> Option<Action> {
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

pub(super) fn handle_confirm_quit_input(app: &mut App, key: KeyEvent) -> Option<Action> {
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

pub(super) fn handle_confirm_delete_input(app: &mut App, key: KeyEvent) -> Option<Action> {
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

pub(super) fn handle_commit_message_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
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
        _ => {
            // Commit message uses a simple cursor-at-end model
            let buf = &mut app.commit_msg_buffer;
            let mut cursor = buf.chars().count();
            handle_text_input(buf, &mut cursor, key, |c| !c.is_control());
        }
    }
    None
}

pub(super) fn handle_confirm_merge_input(app: &mut App, key: KeyEvent) -> Option<Action> {
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

pub(super) fn handle_new_tab_input(app: &mut App, key: KeyEvent) -> Option<Action> {
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
