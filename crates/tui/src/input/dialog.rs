use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use piki_core::{AIProvider, MergeStrategy};

fn dialog_buf_and_cursor(app: &mut App) -> (&mut String, &mut usize) {
    match app.active_dialog_field {
        DialogField::Name => (&mut app.input_buffer, &mut app.input_cursor),
        DialogField::Directory => (&mut app.dir_input_buffer, &mut app.dir_input_cursor),
        DialogField::Description => (&mut app.desc_input_buffer, &mut app.desc_input_cursor),
        DialogField::Prompt => (&mut app.prompt_input_buffer, &mut app.prompt_input_cursor),
        DialogField::KanbanPath => (&mut app.kanban_input_buffer, &mut app.kanban_input_cursor),
    }
}

pub(super) fn handle_edit_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
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
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
        }
        KeyCode::Backspace => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            if *cursor > 0 {
                *cursor -= 1;
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.remove(byte_idx);
            }
        }
        KeyCode::Delete => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            let len = buf.chars().count();
            if *cursor < len {
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
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
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor)
                    .map_or(buf.len(), |(i, _)| i);
                buf.insert(byte_idx, c);
                *cursor += 1;
            }
        }
        KeyCode::Backspace => {
            let (buf, cursor) = dialog_buf_and_cursor(app);
            if *cursor > 0 {
                let byte_idx = buf
                    .char_indices()
                    .nth(*cursor - 1)
                    .map_or(buf.len(), |(i, _)| i);
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
            return Some(Action::CreateWorkspace(
                name,
                description,
                prompt,
                kanban_path,
                dir,
            ));
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
