use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::dialog_state::{ConflictStrategy, DialogState, NewTabMenu};
use piki_core::{AIProvider, MergeStrategy, WorkspaceType};

use super::confirm_common::{ConfirmResult, dismiss_dialog, handle_yn_input};
use super::text_field_common::{handle_text_input, is_cancel};

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
        KeyCode::Tab => {
            *active_field = match *active_field {
                DialogField::KanbanPath => DialogField::Prompt,
                DialogField::Prompt => DialogField::Group,
                DialogField::Group => DialogField::KanbanPath,
                _ => DialogField::KanbanPath,
            };
            return None;
        }
        KeyCode::BackTab => {
            *active_field = match *active_field {
                DialogField::KanbanPath => DialogField::Group,
                DialogField::Prompt => DialogField::KanbanPath,
                DialogField::Group => DialogField::Prompt,
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
        KeyCode::Tab => {
            let hide_name = *ws_type != WorkspaceType::Worktree;
            *active_field = match *active_field {
                DialogField::Type if hide_name => DialogField::Directory,
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
        KeyCode::BackTab => {
            let hide_name = *ws_type != WorkspaceType::Worktree;
            *active_field = match *active_field {
                DialogField::Type => DialogField::Group,
                DialogField::Name => DialogField::Type,
                DialogField::Directory if hide_name => DialogField::Type,
                DialogField::Directory => DialogField::Name,
                DialogField::Description => DialogField::Directory,
                DialogField::Prompt => DialogField::Description,
                DialogField::KanbanPath => DialogField::Prompt,
                DialogField::Group => DialogField::KanbanPath,
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

            // For Simple/Project workspaces, derive name from directory basename
            let ws_name = if ws_type_val != WorkspaceType::Worktree {
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
                let msg = if ws_type_val != WorkspaceType::Worktree {
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

    // Type field: cycle with Space/Right (visual left→right) or Left (reverse)
    // Visual order: Simple  Worktree  Project
    if *active_field == DialogField::Type {
        match key.code {
            KeyCode::Char(' ') | KeyCode::Right => {
                *ws_type = match *ws_type {
                    WorkspaceType::Simple => WorkspaceType::Worktree,
                    WorkspaceType::Worktree => WorkspaceType::Project,
                    WorkspaceType::Project => WorkspaceType::Simple,
                };
                if *ws_type != WorkspaceType::Worktree {
                    name.clear();
                    *name_cursor = 0;
                }
            }
            KeyCode::Left => {
                *ws_type = match *ws_type {
                    WorkspaceType::Simple => WorkspaceType::Project,
                    WorkspaceType::Worktree => WorkspaceType::Simple,
                    WorkspaceType::Project => WorkspaceType::Worktree,
                };
                if *ws_type != WorkspaceType::Worktree {
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

    match handle_yn_input(key) {
        ConfirmResult::Yes => {
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace) {
                ws.close_tab(target);
            }
            dismiss_dialog(app);
        }
        ConfirmResult::No | ConfirmResult::Cancel => {
            dismiss_dialog(app);
        }
        ConfirmResult::NotHandled => {}
    }
    None
}

pub(super) fn handle_confirm_quit_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Quit also accepts Enter as confirmation
    if key.code == KeyCode::Enter {
        app.should_quit = true;
        dismiss_dialog(app);
        return None;
    }
    match handle_yn_input(key) {
        ConfirmResult::Yes => {
            app.should_quit = true;
            dismiss_dialog(app);
        }
        ConfirmResult::No | ConfirmResult::Cancel => {
            dismiss_dialog(app);
        }
        ConfirmResult::NotHandled => {}
    }
    None
}

pub(super) fn handle_confirm_delete_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ConfirmDelete { target }) = app.active_dialog else {
        return None;
    };

    match handle_yn_input(key) {
        ConfirmResult::Yes => {
            dismiss_dialog(app);
            app.active_pane = ActivePane::WorkspaceList;
            Some(Action::DeleteWorkspace(target))
        }
        ConfirmResult::No => {
            dismiss_dialog(app);
            app.active_pane = ActivePane::WorkspaceList;
            Some(Action::RemoveFromList(target))
        }
        ConfirmResult::Cancel => {
            dismiss_dialog(app);
            app.active_pane = ActivePane::WorkspaceList;
            None
        }
        ConfirmResult::NotHandled => None,
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
    let menu = match app.active_dialog {
        Some(DialogState::NewTab { menu }) => menu,
        _ => return None,
    };

    match menu {
        NewTabMenu::Main => match key.code {
            KeyCode::Char('1') => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::SpawnTab(AIProvider::Shell))
            }
            KeyCode::Char('2') => {
                app.active_dialog = Some(DialogState::NewTab {
                    menu: NewTabMenu::Agents,
                });
                None
            }
            KeyCode::Char('3') => {
                app.active_dialog = Some(DialogState::NewTab {
                    menu: NewTabMenu::Tools,
                });
                None
            }
            KeyCode::Esc => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                None
            }
            _ => None,
        },
        NewTabMenu::Agents => match key.code {
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
                Some(Action::SpawnTab(AIProvider::OpenCode))
            }
            KeyCode::Char('4') => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::SpawnTab(AIProvider::Kilo))
            }
            KeyCode::Char('5') => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::SpawnTab(AIProvider::Codex))
            }
            KeyCode::Esc => {
                app.active_dialog = Some(DialogState::NewTab {
                    menu: NewTabMenu::Main,
                });
                None
            }
            _ => None,
        },
        NewTabMenu::Tools => match key.code {
            KeyCode::Char('1') => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::SpawnTab(AIProvider::Kanban))
            }
            KeyCode::Char('2') => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::SpawnTab(AIProvider::CodeReview))
            }
            KeyCode::Char('3') => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::SpawnTab(AIProvider::Api))
            }
            KeyCode::Esc => {
                app.active_dialog = Some(DialogState::NewTab {
                    menu: NewTabMenu::Main,
                });
                None
            }
            _ => None,
        },
    }
}

pub(super) fn handle_dashboard_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::Dashboard {
        ref mut selected,
        ref mut scroll_offset,
    }) = app.active_dialog
    else {
        return None;
    };

    let count = app.workspaces.len();
    if count == 0 {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
        return None;
    }

    // Compute the first visual line for each workspace (1 header + max(1, tabs) per ws)
    let ws_first_line = |idx: usize| -> usize {
        let mut line = 0;
        for i in 0..idx {
            line += 1 + app.workspaces[i].tabs.len().max(1);
        }
        line
    };
    let ws_height = |idx: usize| -> usize { 1 + app.workspaces[idx].tabs.len().max(1) };

    if app.config.matches_dashboard(key, "down") || app.config.matches_dashboard(key, "down_alt") {
        if *selected + 1 < count {
            *selected += 1;
        }
        // Keep selected workspace fully visible (estimate 15 visible lines)
        let visible = 15usize;
        let ws_end = ws_first_line(*selected) + ws_height(*selected);
        if ws_end > *scroll_offset + visible {
            *scroll_offset = ws_end.saturating_sub(visible);
        }
    } else if app.config.matches_dashboard(key, "up") || app.config.matches_dashboard(key, "up_alt")
    {
        *selected = selected.saturating_sub(1);
        let ws_start = ws_first_line(*selected);
        if ws_start < *scroll_offset {
            *scroll_offset = ws_start;
        }
    } else if app.config.matches_dashboard(key, "select") {
        let idx = *selected;
        app.active_dialog = None;
        app.switch_workspace(idx);
    } else if app.config.matches_dashboard(key, "exit")
        || app.config.matches_dashboard(key, "exit_alt")
    {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
    }
    None
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

pub(super) fn handle_logs_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Compute filtered count to resolve usize::MAX selected
    let filter_val = match app.active_dialog {
        Some(DialogState::Logs { level_filter, .. }) => level_filter,
        _ => return None,
    };
    let total = {
        let buf = app.log_buffer.lock();
        buf.iter()
            .filter(|entry| {
                if filter_val == 0 {
                    return true;
                }
                let n = match entry.level {
                    tracing::Level::ERROR => 1,
                    tracing::Level::WARN => 2,
                    tracing::Level::INFO => 3,
                    tracing::Level::DEBUG => 4,
                    tracing::Level::TRACE => 5,
                };
                n <= filter_val
            })
            .count()
    };
    let last = total.saturating_sub(1);

    let Some(DialogState::Logs {
        ref mut scroll,
        ref mut level_filter,
        ref mut selected,
        ref mut hscroll,
    }) = app.active_dialog
    else {
        return None;
    };

    // Resolve sentinel to concrete value
    if *selected > last {
        *selected = last;
        // Also resolve scroll so render uses concrete tracking
        if *scroll == u16::MAX {
            *scroll = total.saturating_sub(20) as u16; // approximate; render will adjust
        }
    }

    if app.config.matches_logs(key, "down") || app.config.matches_logs(key, "down_alt") {
        *selected = (*selected + 1).min(last);
    } else if app.config.matches_logs(key, "up") || app.config.matches_logs(key, "up_alt") {
        *selected = selected.saturating_sub(1);
    } else if app.config.matches_logs(key, "page_down") {
        *selected = (*selected + 10).min(last);
    } else if app.config.matches_logs(key, "page_up") {
        *selected = selected.saturating_sub(10);
    } else if app.config.matches_logs(key, "scroll_top") {
        *selected = 0;
        *scroll = 0;
    } else if app.config.matches_logs(key, "scroll_bottom") {
        *selected = last;
        *scroll = u16::MAX;
    } else if app.config.matches_logs(key, "right") || app.config.matches_logs(key, "right_alt") {
        *hscroll = hscroll.saturating_add(4);
    } else if app.config.matches_logs(key, "left") || app.config.matches_logs(key, "left_alt") {
        *hscroll = hscroll.saturating_sub(4);
    } else if app.config.matches_logs(key, "copy") || app.config.matches_logs(key, "copy_alt") {
        // Copy selected line to clipboard
        let sel = *selected;
        let filter = *level_filter;
        let buf = app.log_buffer.lock();
        let filtered: Vec<_> = buf
            .iter()
            .filter(|entry| {
                if filter == 0 {
                    return true;
                }
                let entry_num = match entry.level {
                    tracing::Level::ERROR => 1,
                    tracing::Level::WARN => 2,
                    tracing::Level::INFO => 3,
                    tracing::Level::DEBUG => 4,
                    tracing::Level::TRACE => 5,
                };
                entry_num <= filter
            })
            .collect();
        let clamped = sel.min(filtered.len().saturating_sub(1));
        if let Some(entry) = filtered.get(clamped) {
            let level_str = match entry.level {
                tracing::Level::ERROR => "ERROR",
                tracing::Level::WARN => "WARN",
                tracing::Level::INFO => "INFO",
                tracing::Level::DEBUG => "DEBUG",
                tracing::Level::TRACE => "TRACE",
            };
            let text = format!(
                "{} {} {} {}",
                entry.timestamp, level_str, entry.target, entry.message
            );
            drop(buf);
            match crate::clipboard::copy_to_clipboard(&text) {
                Ok(()) => app.status_message = Some("Log line copied".into()),
                Err(e) => app.status_message = Some(format!("Copy failed: {e}")),
            }
        }
    } else if let KeyCode::Char(c @ '0'..='5') = key.code {
        *level_filter = (c as u8) - b'0';
        // Reset selection when filter changes
        *selected = usize::MAX;
        *scroll = u16::MAX;
    } else if app.config.matches_logs(key, "exit") || app.config.matches_logs(key, "exit_alt") {
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

pub(super) fn handle_git_stash_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::GitStash {
        ref entries,
        ref mut selected,
        ref mut scroll,
        ref mut input_mode,
        ref mut input_buffer,
        ref mut input_cursor,
    }) = app.active_dialog
    else {
        return None;
    };

    if *input_mode {
        // Text input mode for stash message
        match key.code {
            KeyCode::Enter => {
                let msg = input_buffer.clone();
                *input_mode = false;
                input_buffer.clear();
                *input_cursor = 0;
                if !msg.is_empty() {
                    return Some(Action::GitStashSave(msg));
                }
            }
            KeyCode::Esc => {
                *input_mode = false;
                input_buffer.clear();
                *input_cursor = 0;
            }
            _ => {
                handle_text_input(input_buffer, input_cursor, key, |c| !c.is_control());
            }
        }
        return None;
    }

    // List mode
    let total = entries.len();
    let last = total.saturating_sub(1);
    let _ = scroll; // scroll is managed by the renderer

    if app.config.matches_git_stash(key, "down") || app.config.matches_git_stash(key, "down_alt") {
        if total > 0 {
            *selected = (*selected + 1).min(last);
        }
    } else if app.config.matches_git_stash(key, "up") || app.config.matches_git_stash(key, "up_alt")
    {
        *selected = selected.saturating_sub(1);
    } else if app.config.matches_git_stash(key, "save") {
        *input_mode = true;
    } else if app.config.matches_git_stash(key, "pop") {
        if !entries.is_empty() {
            return Some(Action::GitStashPop(*selected));
        }
    } else if app.config.matches_git_stash(key, "apply") {
        if !entries.is_empty() {
            return Some(Action::GitStashApply(*selected));
        }
    } else if app.config.matches_git_stash(key, "drop") {
        if !entries.is_empty() {
            return Some(Action::GitStashDrop(*selected));
        }
    } else if app.config.matches_git_stash(key, "show") {
        if !entries.is_empty() {
            return Some(Action::GitStashShow(*selected));
        }
    } else if app.config.matches_git_stash(key, "exit")
        || app.config.matches_git_stash(key, "exit_alt")
    {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
    }
    None
}

pub(super) fn handle_git_log_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::GitLog {
        ref lines,
        ref mut selected,
        ref mut scroll,
    }) = app.active_dialog
    else {
        return None;
    };

    let total = lines.len();
    let last = total.saturating_sub(1);

    if app.config.matches_git_log(key, "down") || app.config.matches_git_log(key, "down_alt") {
        *selected = (*selected + 1).min(last);
    } else if app.config.matches_git_log(key, "up") || app.config.matches_git_log(key, "up_alt") {
        *selected = selected.saturating_sub(1);
    } else if app.config.matches_git_log(key, "page_down") {
        *selected = (*selected + 10).min(last);
    } else if app.config.matches_git_log(key, "page_up") {
        *selected = selected.saturating_sub(10);
    } else if app.config.matches_git_log(key, "scroll_top") {
        *selected = 0;
        *scroll = 0;
    } else if app.config.matches_git_log(key, "scroll_bottom") {
        *selected = last;
    } else if app.config.matches_git_log(key, "select") {
        if let Some(entry) = lines.get(*selected)
            && let Some(ref sha) = entry.sha
        {
            let sha = sha.clone();
            return Some(Action::ViewCommitDiff(sha));
        }
    } else if app.config.matches_git_log(key, "exit") || app.config.matches_git_log(key, "exit_alt")
    {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
    }
    None
}

pub(super) fn handle_conflict_resolution_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ConflictResolution {
        ref files,
        ref mut selected,
        ref repo_path,
    }) = app.active_dialog
    else {
        return None;
    };

    let total = files.len();
    let last = total.saturating_sub(1);

    if app
        .config
        .matches_conflict_resolution(key, "down")
        || app
            .config
            .matches_conflict_resolution(key, "down_alt")
    {
        *selected = (*selected + 1).min(last);
    } else if app
        .config
        .matches_conflict_resolution(key, "up")
        || app
            .config
            .matches_conflict_resolution(key, "up_alt")
    {
        *selected = selected.saturating_sub(1);
    } else if app.config.matches_conflict_resolution(key, "ours") {
        if let Some(f) = files.get(*selected) {
            let path = f.path.clone();
            return Some(Action::ResolveConflict {
                file: path,
                strategy: ConflictStrategy::Ours,
            });
        }
    } else if app.config.matches_conflict_resolution(key, "theirs") {
        if let Some(f) = files.get(*selected) {
            let path = f.path.clone();
            return Some(Action::ResolveConflict {
                file: path,
                strategy: ConflictStrategy::Theirs,
            });
        }
    } else if app
        .config
        .matches_conflict_resolution(key, "mark_resolved")
    {
        if let Some(f) = files.get(*selected) {
            let path = f.path.clone();
            return Some(Action::ResolveConflict {
                file: path,
                strategy: ConflictStrategy::MarkResolved,
            });
        }
    } else if app.config.matches_conflict_resolution(key, "select")
        || key.code == KeyCode::Enter
    {
        // View conflict diff
        if let Some(f) = files.get(*selected) {
            let path = f.path.clone();
            return Some(Action::ViewConflictDiff(path));
        }
    } else if app.config.matches_conflict_resolution(key, "edit") {
        if let Some(f) = files.get(*selected) {
            let full_path = repo_path.join(&f.path);
            return Some(Action::OpenEditor(full_path));
        }
    } else if app.config.matches_conflict_resolution(key, "abort") {
        return Some(Action::AbortMerge);
    } else if app
        .config
        .matches_conflict_resolution(key, "exit")
        || app
            .config
            .matches_conflict_resolution(key, "exit_alt")
    {
        app.active_dialog = None;
        app.mode = AppMode::Normal;
        app.interacting = false;
        app.diff_content = None;
        app.diff_file_path = None;
    }
    None
}
