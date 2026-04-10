use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::config::has_ctrl;
use crate::dialog_state::{ConflictStrategy, DialogState, EditAgentField, EditProviderField, NewTabMenu};
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
        _ if is_cancel(key, app.config.platform) => {
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
                if let Ok(home) = std::env::var("HOME") {
                    dir_raw.replacen('~', &home, 1)
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
        _ if is_cancel(key, app.config.platform) => {
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
            // If this is a dispatched workspace, show column picker instead of deleting immediately
            let is_dispatched = app
                .workspaces
                .get(target)
                .is_some_and(|ws| ws.info.dispatch_card_id.is_some());
            if is_dispatched
                && let Some(columns) = get_dispatch_board_columns(app, target)
            {
                app.active_dialog = Some(DialogState::DispatchCardMove {
                    target,
                    columns,
                    selected: 0,
                });
                app.mode = AppMode::DispatchCardMove;
                return None;
            }
            dismiss_dialog(app);
            app.active_pane = ActivePane::WorkspaceList;
            Some(Action::DeleteWorkspace(target, None))
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

/// Load the board columns from the source kanban for a dispatched workspace.
fn get_dispatch_board_columns(app: &mut App, ws_idx: usize) -> Option<Vec<(String, String)>> {
    let ws = app.workspaces.get(ws_idx)?;
    let kanban_path = ws.info.dispatch_source_kanban.as_ref()?;
    let source_ws_idx = app.workspaces.iter().position(|w| {
        w.kanban_path.as_deref() == Some(kanban_path.as_str()) && w.kanban_provider.is_some()
    })?;
    let src_ws = &mut app.workspaces[source_ws_idx];
    let kp = src_ws.kanban_provider.as_mut()?;
    let board = kp.load_board().ok()?;
    let columns: Vec<(String, String)> = board
        .columns
        .iter()
        .map(|c| (c.id.clone(), c.title.clone()))
        .collect();
    if columns.is_empty() { None } else { Some(columns) }
}

pub(super) fn handle_dispatch_card_move_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::DispatchCardMove {
        target,
        ref columns,
        ref mut selected,
    }) = app.active_dialog
    else {
        return None;
    };
    let num_columns = columns.len();

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if *selected > 0 {
                *selected -= 1;
            }
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if *selected + 1 < num_columns {
                *selected += 1;
            }
            None
        }
        KeyCode::Enter => {
            let col_id = columns[*selected].0.clone();
            dismiss_dialog(app);
            app.active_pane = ActivePane::WorkspaceList;
            Some(Action::DeleteWorkspace(target, Some(col_id)))
        }
        KeyCode::Esc => {
            dismiss_dialog(app);
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
    let menu = match app.active_dialog {
        Some(DialogState::NewTab { ref menu }) => menu.clone(),
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
                    menu: NewTabMenu::Agents { selected: 0 },
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
        NewTabMenu::Agents { selected } => {
            // Build provider list: built-in dispatchable + custom providers
            let providers = app.new_tab_agent_list();
            let count = providers.len();

            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    let next = if selected + 1 < count { selected + 1 } else { 0 };
                    app.active_dialog = Some(DialogState::NewTab {
                        menu: NewTabMenu::Agents { selected: next },
                    });
                    None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let prev = if selected > 0 { selected - 1 } else { count.saturating_sub(1) };
                    app.active_dialog = Some(DialogState::NewTab {
                        menu: NewTabMenu::Agents { selected: prev },
                    });
                    None
                }
                KeyCode::Enter => {
                    if let Some(provider) = providers.get(selected) {
                        app.active_dialog = None;
                        app.mode = AppMode::Normal;
                        Some(Action::SpawnTab(provider.clone()))
                    } else {
                        None
                    }
                }
                // Digit shortcuts for first 9 providers
                KeyCode::Char(c @ '1'..='9') => {
                    let idx = (c as usize) - ('1' as usize);
                    if let Some(provider) = providers.get(idx) {
                        app.active_dialog = None;
                        app.mode = AppMode::Normal;
                        Some(Action::SpawnTab(provider.clone()))
                    } else {
                        None
                    }
                }
                KeyCode::Esc => {
                    app.active_dialog = Some(DialogState::NewTab {
                        menu: NewTabMenu::Main,
                    });
                    None
                }
                _ => None,
            }
        }
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

pub(super) fn handle_dispatch_agent_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Pre-compute provider list before mutably borrowing dialog state
    let dispatchable_providers = app.new_tab_agent_list();

    let Some(DialogState::DispatchAgent {
        source_ws,
        ref card_id,
        ref card_title,
        ref card_description,
        card_priority,
        ref card_project,
        ref mut agent_idx,
        ref agents,
        ref mut additional_prompt,
        ref mut additional_prompt_cursor,
        ref mut step,
        ref mut use_current_ws,
    }) = app.active_dialog
    else {
        return None;
    };

    if *step == 1 {
        // Step 2: workspace destination selection
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                *use_current_ws = !*use_current_ws;
                None
            }
            KeyCode::Enter => {
                let (provider, agent_name, agent_role) = if agents.is_empty() {
                    let p = dispatchable_providers.get(*agent_idx).cloned().unwrap_or(AIProvider::Claude);
                    (p, None, None)
                } else if *agent_idx == agents.len() {
                    (AIProvider::Claude, None, None)
                } else {
                    let (name, prov_str, role) = &agents[*agent_idx];
                    let p = AIProvider::from_label(prov_str);
                    (p, Some(name.clone()), Some(role.clone()))
                };
                let action = Action::DispatchAgent {
                    source_ws,
                    card_id: card_id.clone(),
                    card_title: card_title.clone(),
                    card_description: card_description.clone(),
                    card_priority,
                    card_project: card_project.clone(),
                    provider,
                    agent_name,
                    agent_role,
                    additional_prompt: additional_prompt.clone(),
                    use_current_ws: *use_current_ws,
                };
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(action)
            }
            _ if is_cancel(key, app.config.platform) => {
                // Back to step 0
                *step = 0;
                None
            }
            _ => None,
        }
    } else {
        // Step 0: agent/provider selection
        let count = if agents.is_empty() {
            dispatchable_providers.len()
        } else {
            agents.len() + 1 // +1 for "(None)" option
        };

        match key.code {
            KeyCode::Left => {
                *agent_idx = (*agent_idx + count - 1) % count;
                None
            }
            KeyCode::Right | KeyCode::Tab => {
                *agent_idx = (*agent_idx + 1) % count;
                None
            }
            KeyCode::Enter => {
                let no_agent = agents.is_empty() || *agent_idx == agents.len();
                if no_agent {
                    // No agent selected — ask about workspace destination
                    *step = 1;
                    None
                } else {
                    // Agent selected — dispatch directly to new worktree
                    let (name, prov_str, role) = &agents[*agent_idx];
                    let p = AIProvider::from_label(prov_str);
                    let action = Action::DispatchAgent {
                        source_ws,
                        card_id: card_id.clone(),
                        card_title: card_title.clone(),
                        card_description: card_description.clone(),
                        card_priority,
                        card_project: card_project.clone(),
                        provider: p,
                        agent_name: Some(name.clone()),
                        agent_role: Some(role.clone()),
                        additional_prompt: additional_prompt.clone(),
                        use_current_ws: false,
                    };
                    app.active_dialog = None;
                    app.mode = AppMode::Normal;
                    Some(action)
                }
            }
            _ if is_cancel(key, app.config.platform) => {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                None
            }
            _ => {
                handle_text_input(additional_prompt, additional_prompt_cursor, key, |c| {
                    !c.is_control()
                });
                None
            }
        }
    }
}

pub(super) fn handle_manage_agents_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ManageAgents {
        ref mut selected, ..
    }) = app.active_dialog
    else {
        return None;
    };

    let count = app.agent_profiles.len();

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if count > 0 {
                *selected = (*selected + 1) % count;
            }
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if count > 0 {
                *selected = (*selected + count - 1) % count;
            }
            None
        }
        KeyCode::Char('n') => {
            // New agent
            app.active_dialog = Some(DialogState::EditAgent {
                editing_id: None,
                name: String::new(),
                name_cursor: 0,
                provider_idx: 0,
                role: String::new(),
                active_field: EditAgentField::Name,
            });
            app.mode = AppMode::EditAgent;
            None
        }
        KeyCode::Char('e') | KeyCode::Enter => {
            // Edit selected agent
            if let Some(agent) = app.agent_profiles.get(*selected) {
                let providers = app.new_tab_agent_list();
                let provider_idx = providers
                    .iter()
                    .position(|p| p.label() == agent.provider)
                    .unwrap_or(0);
                let name = agent.name.clone();
                let role = agent.role.clone();
                app.active_dialog = Some(DialogState::EditAgent {
                    editing_id: agent.id,
                    name_cursor: name.len(),
                    name,
                    provider_idx,
                    role,
                    active_field: EditAgentField::Name,
                });
                app.mode = AppMode::EditAgent;
            }
            None
        }
        KeyCode::Char('d') => {
            // Delete selected agent
            if let Some(agent) = app.agent_profiles.get(*selected)
                && let Some(id) = agent.id
            {
                let action = Action::DeleteAgent(id);
                if *selected > 0 && *selected >= count.saturating_sub(1) {
                    *selected = selected.saturating_sub(1);
                }
                return Some(action);
            }
            None
        }
        KeyCode::Char('p') => {
            // Persist agent to repo (Simple workspace only)
            if let Some(agent) = app.agent_profiles.get(*selected)
                && let Some(id) = agent.id
            {
                return Some(Action::SyncAgentToRepo(id));
            }
            None
        }
        KeyCode::Char('i') => {
            // Import agents from repo files
            Some(Action::ScanRepoAgents)
        }
        _ if is_cancel(key, app.config.platform) => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_edit_agent_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let providers = app.new_tab_agent_list();

    let Some(DialogState::EditAgent {
        editing_id,
        ref mut name,
        ref mut name_cursor,
        ref mut provider_idx,
        ref role,
        ref mut active_field,
    }) = app.active_dialog
    else {
        return None;
    };

    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            *active_field = match active_field {
                EditAgentField::Name => EditAgentField::Provider,
                EditAgentField::Provider => EditAgentField::Name,
            };
            None
        }
        KeyCode::Left if *active_field == EditAgentField::Provider => {
            *provider_idx = (*provider_idx + providers.len() - 1) % providers.len();
            None
        }
        KeyCode::Right if *active_field == EditAgentField::Provider => {
            *provider_idx = (*provider_idx + 1) % providers.len();
            None
        }
        KeyCode::Enter => {
            if name.trim().is_empty() {
                return None;
            }
            // Advance to step 2: role editor
            let role_text = role.clone();
            let cursor = role_text.len();
            app.active_dialog = Some(DialogState::EditAgentRole {
                editing_id,
                name: name.trim().to_string(),
                provider_idx: *provider_idx,
                role: role_text,
                role_cursor: cursor,
                scroll: 0,
            });
            app.mode = AppMode::EditAgentRole;
            None
        }
        _ if is_cancel(key, app.config.platform) => {
            app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
            app.mode = AppMode::ManageAgents;
            None
        }
        _ => {
            if *active_field == EditAgentField::Name {
                handle_text_input(name, name_cursor, key, |c| {
                    c.is_alphanumeric() || c == '-' || c == '_'
                });
            }
            None
        }
    }
}

pub(super) fn handle_edit_agent_role_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let source_repo_str = app
        .current_workspace()
        .map(|ws| ws.source_repo.display().to_string())
        .unwrap_or_default();
    let providers = app.new_tab_agent_list();

    let Some(DialogState::EditAgentRole {
        editing_id,
        ref name,
        provider_idx,
        ref mut role,
        ref mut role_cursor,
        ref mut scroll,
    }) = app.active_dialog
    else {
        return None;
    };

    // Ctrl+S (Cmd+S on macOS): save and close
    if key.code == KeyCode::Char('s') && has_ctrl(key.modifiers, app.config.platform) {
        let profile = piki_core::storage::AgentProfile {
            id: editing_id,
            source_repo: source_repo_str.clone(),
            name: name.clone(),
            provider: providers[provider_idx].label().to_string(),
            role: role.clone(),
            version: 0, // DB handles version increment
            last_synced_at: None,
        };
        let action = Action::SaveAgent {
            source_repo: PathBuf::from(&source_repo_str),
            profile,
        };
        app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
        app.mode = AppMode::ManageAgents;
        return Some(action);
    }

    // Esc: go back to step 1 without saving
    if is_cancel(key, app.config.platform) {
        app.active_dialog = Some(DialogState::EditAgent {
            editing_id,
            name: name.clone(),
            name_cursor: name.len(),
            provider_idx,
            role: role.clone(),
            active_field: EditAgentField::Name,
        });
        app.mode = AppMode::EditAgent;
        return None;
    }

    // Ctrl+D (Cmd+D on macOS): clear all text
    if key.code == KeyCode::Char('d') && has_ctrl(key.modifiers, app.config.platform) {
        role.clear();
        *role_cursor = 0;
        *scroll = 0;
        return None;
    }

    // Text editing
    match key.code {
        KeyCode::Enter => {
            // Insert newline
            let byte_idx = cursor_to_byte(role, *role_cursor);
            role.insert(byte_idx, '\n');
            *role_cursor += 1;
        }
        KeyCode::Down => {
            move_cursor_vertical(role, role_cursor, 1);
        }
        KeyCode::Up => {
            move_cursor_vertical(role, role_cursor, -1);
        }
        KeyCode::PageDown => {
            move_cursor_vertical(role, role_cursor, 10);
        }
        KeyCode::PageUp => {
            move_cursor_vertical(role, role_cursor, -10);
        }
        _ => {
            handle_text_input(role, role_cursor, key, |c| c != '\t');
        }
    }
    None
}

/// Move cursor up or down by `delta` lines, preserving column position.
pub(super) fn move_cursor_vertical(text: &str, cursor: &mut usize, delta: i32) {
    let (cur_line, cur_col, line_starts) = cursor_line_col(text, *cursor);
    let target = if delta > 0 {
        (cur_line + delta as usize).min(line_starts.len() - 1)
    } else {
        cur_line.saturating_sub((-delta) as usize)
    };
    if target == cur_line {
        // Already at boundary
        if delta < 0 {
            *cursor = 0;
        } else {
            *cursor = char_count(text);
        }
        return;
    }
    let start = line_starts[target];
    let end = line_starts
        .get(target + 1)
        .map(|e| e - 1) // exclude the \n
        .unwrap_or(char_count(text));
    let line_len = end - start;
    *cursor = start + cur_col.min(line_len);
}

/// Returns (line_index, column, line_start_offsets) for a cursor position in text.
fn cursor_line_col(text: &str, cursor: usize) -> (usize, usize, Vec<usize>) {
    let mut line_starts = vec![0usize];
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            line_starts.push(i + 1);
        }
    }
    let mut line = 0;
    for (i, &start) in line_starts.iter().enumerate() {
        if cursor >= start {
            line = i;
        }
    }
    let col = cursor - line_starts[line];
    (line, col, line_starts)
}

fn cursor_to_byte(text: &str, cursor: usize) -> usize {
    if text.is_ascii() {
        cursor.min(text.len())
    } else {
        text.char_indices()
            .nth(cursor)
            .map_or(text.len(), |(i, _)| i)
    }
}

fn char_count(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    }
}

pub(super) fn handle_import_agents_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ImportAgents {
        ref discovered,
        ref mut selected,
        ref mut cursor,
    }) = app.active_dialog
    else {
        return None;
    };

    let count = discovered.len();
    if count == 0 {
        if is_cancel(key, app.config.platform) || key.code == KeyCode::Enter {
            app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
            app.mode = AppMode::ManageAgents;
        }
        return None;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            *cursor = (*cursor + 1) % count;
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            *cursor = (*cursor + count - 1) % count;
            None
        }
        KeyCode::Char(' ') => {
            selected[*cursor] = !selected[*cursor];
            if *cursor + 1 < count {
                *cursor += 1;
            }
            None
        }
        KeyCode::Char('a') => {
            let all_selected = selected.iter().all(|&s| s);
            for s in selected.iter_mut() {
                *s = !all_selected;
            }
            None
        }
        KeyCode::Enter => {
            // Collect selected agents for import
            let to_import: Vec<(String, String, String)> = discovered
                .iter()
                .zip(selected.iter())
                .filter(|(_, sel)| **sel)
                .map(|((name, provider, role, _), _)| {
                    (name.clone(), provider.clone(), role.clone())
                })
                .collect();
            if to_import.is_empty() {
                // Nothing selected, go back
                app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
                app.mode = AppMode::ManageAgents;
                None
            } else {
                app.active_dialog = None;
                app.mode = AppMode::Normal;
                Some(Action::ImportAgents(to_import))
            }
        }
        _ if is_cancel(key, app.config.platform) => {
            app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
            app.mode = AppMode::ManageAgents;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_manage_providers_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    let Some(DialogState::ManageProviders {
        ref mut selected, ..
    }) = app.active_dialog
    else {
        return None;
    };

    let count = app.provider_manager.all().len();

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if count > 0 {
                *selected = (*selected + 1) % count;
            }
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if count > 0 {
                *selected = (*selected + count - 1) % count;
            }
            None
        }
        KeyCode::Char('n') => {
            // New provider
            app.active_dialog = Some(DialogState::EditProvider {
                original_name: None,
                name: String::new(),
                name_cursor: 0,
                description: String::new(),
                desc_cursor: 0,
                command: String::new(),
                command_cursor: 0,
                default_args: String::new(),
                args_cursor: 0,
                prompt_format_idx: 0, // Positional
                prompt_flag: String::new(),
                flag_cursor: 0,
                dispatchable: true,
                agent_dir: String::new(),
                agent_dir_cursor: 0,
                active_field: EditProviderField::Name,
            });
            app.mode = AppMode::EditProvider;
            None
        }
        KeyCode::Char('e') | KeyCode::Enter => {
            // Edit selected provider
            if let Some(config) = app.provider_manager.all().get(*selected) {
                let prompt_format_idx = match &config.prompt_format {
                    piki_core::providers::PromptFormat::Positional => 0,
                    piki_core::providers::PromptFormat::Flag(_) => 1,
                    piki_core::providers::PromptFormat::None => 2,
                };
                let prompt_flag = match &config.prompt_format {
                    piki_core::providers::PromptFormat::Flag(f) => f.clone(),
                    _ => String::new(),
                };
                let args_str = config.default_args.join(" ");
                let agent_dir = config.agent_dir.clone().unwrap_or_default();
                let name = config.name.clone();
                let desc = config.description.clone();
                let cmd = config.command.clone();
                app.active_dialog = Some(DialogState::EditProvider {
                    original_name: Some(name.clone()),
                    name_cursor: name.len(),
                    name,
                    desc_cursor: desc.len(),
                    description: desc,
                    command_cursor: cmd.len(),
                    command: cmd,
                    args_cursor: args_str.len(),
                    default_args: args_str,
                    prompt_format_idx,
                    flag_cursor: prompt_flag.len(),
                    prompt_flag,
                    dispatchable: config.dispatchable,
                    agent_dir_cursor: agent_dir.len(),
                    agent_dir,
                    active_field: EditProviderField::Name,
                });
                app.mode = AppMode::EditProvider;
            }
            None
        }
        KeyCode::Char('d') => {
            // Delete selected provider
            if let Some(config) = app.provider_manager.all().get(*selected) {
                let name = config.name.clone();
                app.provider_manager.remove(&name);
                let _ = app.provider_manager.save(&app.paths.providers_path());
                if *selected > 0 && *selected >= app.provider_manager.all().len() {
                    *selected = selected.saturating_sub(1);
                }
                app.set_toast(format!("Provider deleted: {}", name), crate::app::ToastLevel::Success);
            }
            None
        }
        _ if is_cancel(key, app.config.platform) => {
            app.active_dialog = None;
            app.mode = AppMode::Normal;
            None
        }
        _ => None,
    }
}

pub(super) fn handle_edit_provider_input(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Ctrl+S: collect data from dialog, save, and return to manager
    if key.code == KeyCode::Char('s') && has_ctrl(key.modifiers, app.config.platform) {
        let config = if let Some(DialogState::EditProvider {
            ref original_name,
            ref name,
            ref description,
            ref command,
            ref default_args,
            prompt_format_idx,
            ref prompt_flag,
            dispatchable,
            ref agent_dir,
            ..
        }) = app.active_dialog
        {
            if name.is_empty() || command.is_empty() {
                app.set_toast("Name and command are required", crate::app::ToastLevel::Error);
                return None;
            }
            let prompt_format = match prompt_format_idx {
                1 => piki_core::providers::PromptFormat::Flag(prompt_flag.clone()),
                2 => piki_core::providers::PromptFormat::None,
                _ => piki_core::providers::PromptFormat::Positional,
            };
            let args: Vec<String> = if default_args.trim().is_empty() {
                Vec::new()
            } else {
                default_args.split_whitespace().map(String::from).collect()
            };
            let old_name = original_name.clone();
            Some((old_name, piki_core::providers::ProviderConfig {
                name: name.clone(),
                description: description.clone(),
                command: command.clone(),
                default_args: args,
                prompt_format,
                dispatchable,
                agent_dir: if agent_dir.is_empty() { None } else { Some(agent_dir.clone()) },
            }))
        } else {
            None
        };
        if let Some((old_name, config)) = config {
            let saved_name = config.name.clone();
            // If editing and name changed, remove old entry first
            if let Some(ref old) = old_name {
                if *old != saved_name {
                    app.provider_manager.remove(old);
                }
            }
            app.provider_manager.upsert(config);
            let _ = app.provider_manager.save(&app.paths.providers_path());
            app.set_toast(format!("Provider saved: {saved_name}"), crate::app::ToastLevel::Success);
            app.active_dialog = Some(DialogState::ManageProviders { selected: 0 });
            app.mode = AppMode::ManageProviders;
        }
        return None;
    }

    // Esc: cancel back to manage
    if is_cancel(key, app.config.platform) {
        app.active_dialog = Some(DialogState::ManageProviders { selected: 0 });
        app.mode = AppMode::ManageProviders;
        return None;
    }

    let Some(DialogState::EditProvider {
        ref mut name,
        ref mut name_cursor,
        ref mut description,
        ref mut desc_cursor,
        ref mut command,
        ref mut command_cursor,
        ref mut default_args,
        ref mut args_cursor,
        ref mut prompt_format_idx,
        ref mut prompt_flag,
        ref mut flag_cursor,
        ref mut dispatchable,
        ref mut agent_dir,
        ref mut agent_dir_cursor,
        ref mut active_field,
        ..
    }) = app.active_dialog
    else {
        return None;
    };

    // Tab / BackTab to cycle fields
    if key.code == KeyCode::Tab {
        *active_field = active_field.next();
        return None;
    }
    if key.code == KeyCode::BackTab {
        *active_field = active_field.prev();
        return None;
    }

    // Field-specific handling
    let accept_any = |c: char| !c.is_control();
    match *active_field {
        EditProviderField::Name => { handle_text_input(name, name_cursor, key, accept_any); }
        EditProviderField::Description => { handle_text_input(description, desc_cursor, key, accept_any); }
        EditProviderField::Command => { handle_text_input(command, command_cursor, key, accept_any); }
        EditProviderField::DefaultArgs => { handle_text_input(default_args, args_cursor, key, accept_any); }
        EditProviderField::PromptFormat => {
            match key.code {
                KeyCode::Left => *prompt_format_idx = (*prompt_format_idx + 2) % 3,
                KeyCode::Right => *prompt_format_idx = (*prompt_format_idx + 1) % 3,
                _ => {}
            }
        }
        EditProviderField::PromptFlag => { handle_text_input(prompt_flag, flag_cursor, key, accept_any); }
        EditProviderField::Dispatchable => {
            match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                    *dispatchable = !*dispatchable;
                }
                _ => {}
            }
        }
        EditProviderField::AgentDir => { handle_text_input(agent_dir, agent_dir_cursor, key, accept_any); }
    }
    None
}
