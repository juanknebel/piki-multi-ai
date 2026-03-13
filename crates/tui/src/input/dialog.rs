use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::dialog_state::DialogState;
use piki_core::{AIProvider, MergeStrategy, WorkspaceType};

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

fn is_cancel(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
}

pub(super) fn handle_edit_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::EditWorkspace {
        ref mut target,
        ref mut kanban,
        ref mut kanban_cursor,
        ref mut prompt,
        ref mut prompt_cursor,
        ref mut group,
        ref mut group_cursor,
        ref mut active_field,
    }) = app.active_dialog
    else {
        return None;
    };

    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            *active_field = match *active_field {
                DialogField::KanbanPath => DialogField::Prompt,
                DialogField::Prompt => DialogField::Group,
                DialogField::Group => DialogField::KanbanPath,
                _ => DialogField::KanbanPath,
            };
            return None;
        }
        KeyCode::Enter => {
            let kanban_path_raw = kanban.trim();
            let kanban_path = if kanban_path_raw.is_empty() {
                None
            } else {
                Some(kanban_path_raw.to_string())
            };
            let prompt_val = prompt.clone();
            let group_raw = group.trim();
            let group_val = if group_raw.is_empty() {
                None
            } else {
                Some(group_raw.to_string())
            };
            let idx = *target;

            app.active_dialog = None;
            app.mode = AppMode::Normal;
            return Some(Action::EditWorkspace(
                idx,
                kanban_path,
                prompt_val,
                group_val,
            ));
        }
        _ if is_cancel(key) => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            return None;
        }
        _ => {}
    }

    // Text input for the active field
    let (buf, cursor) = match *active_field {
        DialogField::KanbanPath => (kanban as &mut String, kanban_cursor as &mut usize),
        DialogField::Prompt => (prompt, prompt_cursor),
        DialogField::Group => (group, group_cursor),
        _ => return None,
    };
    handle_text_input(buf, cursor, key, |c| !c.is_control());
    None
}

pub(super) fn handle_new_workspace_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::NewWorkspace {
        ref mut name,
        ref mut name_cursor,
        ref mut dir,
        ref mut dir_cursor,
        ref mut desc,
        ref mut desc_cursor,
        ref mut prompt,
        ref mut prompt_cursor,
        ref mut kanban,
        ref mut kanban_cursor,
        ref mut group,
        ref mut group_cursor,
        ref mut ws_type,
        ref mut active_field,
    }) = app.active_dialog
    else {
        return None;
    };

    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            let is_simple = *ws_type == WorkspaceType::Simple;
            *active_field = match *active_field {
                DialogField::Type if is_simple => DialogField::Directory,
                DialogField::Type => DialogField::Name,
                DialogField::Name => DialogField::Directory,
                DialogField::Directory => DialogField::Description,
                DialogField::Description => DialogField::Prompt,
                DialogField::Prompt => DialogField::KanbanPath,
                DialogField::KanbanPath => DialogField::Group,
                DialogField::Group => DialogField::Type,
            };
            return None;
        }
        KeyCode::Enter => {
            let dir_raw = dir.clone();
            let description = desc.clone();
            let prompt_val = prompt.clone();
            let kanban_path_raw = kanban.trim();
            let kanban_path = if kanban_path_raw.is_empty() {
                None
            } else {
                Some(kanban_path_raw.to_string())
            };
            let group_raw = group.trim();
            let group_val = if group_raw.is_empty() {
                None
            } else {
                Some(group_raw.to_string())
            };
            let ws_type_val = *ws_type;

            // For Simple workspaces, derive name from directory basename
            let ws_name = if ws_type_val == WorkspaceType::Simple {
                if name.is_empty() {
                    PathBuf::from(&dir_raw)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    name.clone()
                }
            } else {
                name.clone()
            };

            if ws_name.is_empty() || dir_raw.is_empty() {
                let msg = if ws_type_val == WorkspaceType::Simple {
                    "Directory is required"
                } else {
                    "Name and directory are required"
                };
                app.status_message = Some(msg.into());
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

            let dir_path = PathBuf::from(&dir_str);
            if !dir_path.exists() {
                app.status_message = Some(format!("Directory does not exist: {}", dir_str));
                return None;
            }

            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            return Some(Action::CreateWorkspace(
                ws_name,
                description,
                prompt_val,
                kanban_path,
                dir_path,
                ws_type_val,
                group_val,
            ));
        }
        _ if is_cancel(key) => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            return None;
        }
        _ => {}
    }

    // Type field: toggle with Space/Left/Right, not a text input
    if *active_field == DialogField::Type {
        match key.code {
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                *ws_type = match *ws_type {
                    WorkspaceType::Worktree => WorkspaceType::Simple,
                    WorkspaceType::Simple => WorkspaceType::Worktree,
                };
                // If switching to Simple, clear name
                if *ws_type == WorkspaceType::Simple {
                    name.clear();
                    *name_cursor = 0;
                }
            }
            _ => {}
        }
        return None;
    }

    let field = *active_field;
    let (buf, cursor) = match field {
        DialogField::Name => (name as &mut String, name_cursor as &mut usize),
        DialogField::Directory => (dir, dir_cursor),
        DialogField::Description => (desc, desc_cursor),
        DialogField::Prompt => (prompt, prompt_cursor),
        DialogField::KanbanPath => (kanban, kanban_cursor),
        DialogField::Group => (group, group_cursor),
        DialogField::Type => return None,
    };
    let validator = |c: char| -> bool {
        match field {
            DialogField::Name => {
                c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/'
            }
            _ => !c.is_control(),
        }
    };
    handle_text_input(buf, cursor, key, validator);
    None
}

pub(super) fn handle_confirm_close_tab_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ConfirmCloseTab { target }) = app.active_dialog else {
        return None;
    };

    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                ws.close_tab(target);
            }
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            None
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.active_dialog = None;
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
            app.active_dialog = None;
            None
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_confirm_delete_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ConfirmDelete { target }) = app.active_dialog else {
        return None;
    };

    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            Some(Action::DeleteWorkspace(target))
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            Some(Action::RemoveFromList(target))
        }
        KeyCode::Esc => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            app.active_pane = ActivePane::WorkspaceList;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_commit_message_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::CommitMessage { ref mut buffer }) = app.active_dialog else {
        return None;
    };

    match key.code {
        KeyCode::Enter => {
            let message = buffer.clone();
            if message.is_empty() {
                app.status_message = Some("Commit message cannot be empty".into());
                return None;
            }
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            return Some(Action::GitCommit(message));
        }
        KeyCode::Esc => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
        }
        _ => {
            let mut cursor = buffer.chars().count();
            handle_text_input(buffer, &mut cursor, key, |c| !c.is_control());
        }
    }
    None
}

pub(super) fn handle_confirm_merge_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('m') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::GitMerge(MergeStrategy::Merge))
        }
        KeyCode::Char('r') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::GitMerge(MergeStrategy::Rebase))
        }
        KeyCode::Esc => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_new_tab_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('1') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Claude))
        }
        KeyCode::Char('2') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Gemini))
        }
        KeyCode::Char('3') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Codex))
        }
        KeyCode::Char('4') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Shell))
        }
        KeyCode::Char('5') => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            Some(Action::SpawnTab(AIProvider::Kanban))
        }
        KeyCode::Esc => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_help_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::Help { ref mut scroll }) = app.active_dialog else {
        return None;
    };

    if app.config.matches_help(key, "down") || app.config.matches_help(key, "down_alt") {
        *scroll = scroll.saturating_add(1);
    } else if app.config.matches_help(key, "up") || app.config.matches_help(key, "up_alt") {
        *scroll = scroll.saturating_sub(1);
    } else if app.config.matches_help(key, "page_down") {
        *scroll = scroll.saturating_add(10);
    } else if app.config.matches_help(key, "page_up") {
        *scroll = scroll.saturating_sub(10);
    } else if app.config.matches_help(key, "scroll_top") {
        *scroll = 0;
    } else if app.config.matches_help(key, "scroll_bottom") {
        *scroll = u16::MAX;
    } else if app.config.matches_help(key, "exit")
        || app.config.matches_help(key, "exit_alt")
        || app.config.matches_help(key, "exit_help")
    {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
    }
    None
}

pub(super) fn handle_about_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    if app.config.matches_about(key, "exit") {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
    }
    None
}

pub(super) fn handle_workspace_info_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::WorkspaceInfo { ref mut hscroll }) = app.active_dialog else {
        return None;
    };

    if app.config.matches_workspace_info(key, "right")
        || app.config.matches_workspace_info(key, "right_alt")
    {
        *hscroll = hscroll.saturating_add(4);
    } else if app.config.matches_workspace_info(key, "left")
        || app.config.matches_workspace_info(key, "left_alt")
    {
        *hscroll = hscroll.saturating_sub(4);
    } else if app.config.matches_workspace_info(key, "exit")
        || app.config.matches_workspace_info(key, "exit_info")
    {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
        let _ = crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture);
    }
    None
}
