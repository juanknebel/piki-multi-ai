pub(crate) mod app_actions;
mod chat_input;
mod code_review_input;
mod command_palette_input;
pub(crate) mod confirm_common;
pub(super) mod dialog;
#[cfg(test)]
mod dialog_tests;
mod editor_input;
pub(crate) mod fuzzy_common;
mod fuzzy_input;
mod interaction;
pub(crate) mod list_nav;
pub(crate) mod mouse;
pub(crate) mod text_field_common;
mod workspace_switcher_input;

use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode};
use crate::helpers::copy_visible_terminal;

use self::command_palette_input::handle_command_palette_input;
use self::dialog::{
    handle_about_input, handle_commit_message_input, handle_confirm_close_tab_input,
    handle_confirm_delete_input, handle_confirm_merge_input, handle_confirm_quit_input,
    handle_conflict_resolution_input, handle_dashboard_input, handle_dispatch_agent_input,
    handle_dispatch_card_move_input, handle_edit_agent_input, handle_edit_agent_role_input,
    handle_edit_workspace_input,
    handle_git_log_input,
    handle_git_stash_input, handle_help_input, handle_import_agents_input, handle_logs_input,
    handle_manage_agents_input, handle_manage_providers_input, handle_edit_provider_input,
    handle_create_worktree_input, handle_new_tab_input, handle_new_workspace_input,
    handle_workspace_info_input,
};
use self::editor_input::handle_inline_edit_input;
use self::fuzzy_input::handle_fuzzy_search_input;
use self::interaction::{
    handle_api_interaction, handle_diff_interaction, handle_filelist_interaction,
    handle_kanban_interaction, handle_markdown_interaction, handle_terminal_interaction,
    handle_workspace_interaction,
};

/// Handle a bracketed paste event — insert full text at once into the active context.
pub(crate) fn handle_paste(app: &mut App, text: &str) {
    // Terminal interaction: write to PTY
    if app.interacting
        && matches!(app.mode, AppMode::Normal | AppMode::Diff)
        && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        if tab.pty_session.is_some() {
            let bracketed = tab
                .pty_parser
                .as_ref()
                .map(|p| p.lock().screen().bracketed_paste())
                .unwrap_or(false);
            let data = if bracketed {
                format!("\x1b[200~{text}\x1b[201~")
            } else {
                text.to_string()
            };
            if let Some(ref mut pty) = tab.pty_session {
                let _ = pty.write(data.as_bytes());
            }
            return;
        }
        if let Some(ref mut api) = tab.api_state {
            api.editor.insert_text(text);
            return;
        }
    }

    // Inline editor
    if app.mode == AppMode::InlineEdit {
        if let Some(ref mut editor) = app.editor {
            editor.insert_text(text);
        }
        return;
    }

    // Fuzzy overlays: insert into query
    match app.mode {
        AppMode::FuzzySearch => {
            if let Some(ref mut state) = app.fuzzy {
                state.query.push_str(text);
                let q = state.query.clone();
                state
                    .nucleo
                    .pattern
                    .reparse(0, &q, nucleo::pattern::CaseMatching::Smart, true);
            }
            return;
        }
        AppMode::CommandPalette => {
            if let Some(ref mut state) = app.command_palette {
                state.query.push_str(text);
                let q = state.query.clone();
                state
                    .nucleo
                    .pattern
                    .reparse(0, &q, nucleo::pattern::CaseMatching::Smart, true);
            }
            return;
        }
        AppMode::WorkspaceSwitcher => {
            if let Some(ref mut state) = app.workspace_switcher {
                state.query.push_str(text);
                let q = state.query.clone();
                state
                    .nucleo
                    .pattern
                    .reparse(0, &q, nucleo::pattern::CaseMatching::Smart, true);
            }
            return;
        }
        _ => {}
    }

    // Dialog text fields
    text_field_common::handle_bulk_insert(app, text);
}

pub(crate) fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Code review is a locked mode — ALL keys route here, nothing leaks
    if code_review_input::is_code_review_locked(app) {
        app.status_message = None;
        app.toast = None;
        return code_review_input::handle_code_review_key(app, key);
    }

    // Modal dispatch — each mode captures all input
    match app.mode {
        AppMode::WorkspaceInfo => return handle_workspace_info_input(app, key),
        AppMode::About => return handle_about_input(app, key),
        AppMode::Help => return handle_help_input(app, key),
        AppMode::FuzzySearch => return handle_fuzzy_search_input(app, key),
        AppMode::InlineEdit => return handle_inline_edit_input(app, key),
        AppMode::NewWorkspace => return handle_new_workspace_input(app, key),
        AppMode::EditWorkspace => return handle_edit_workspace_input(app, key),
        AppMode::CreateWorktree => return handle_create_worktree_input(app, key),
        AppMode::CommitMessage => return handle_commit_message_input(app, key),
        AppMode::ConfirmMerge => return handle_confirm_merge_input(app, key),
        AppMode::NewTab => return handle_new_tab_input(app, key),
        AppMode::ConfirmCloseTab => return handle_confirm_close_tab_input(app, key),
        AppMode::ConfirmQuit => return handle_confirm_quit_input(app, key),
        AppMode::Dashboard => return handle_dashboard_input(app, key),
        AppMode::Logs => return handle_logs_input(app, key),
        AppMode::CommandPalette => return handle_command_palette_input(app, key),
        AppMode::WorkspaceSwitcher => {
            return workspace_switcher_input::handle_workspace_switcher_input(app, key);
        }
        AppMode::ConfirmDelete => return handle_confirm_delete_input(app, key),
        AppMode::SubmitReview => return code_review_input::handle_submit_review_input(app, key),
        AppMode::GitStash => return handle_git_stash_input(app, key),
        AppMode::GitLog => return handle_git_log_input(app, key),
        AppMode::ConflictResolution => return handle_conflict_resolution_input(app, key),
        AppMode::DispatchAgent => return handle_dispatch_agent_input(app, key),
        AppMode::ManageAgents => return handle_manage_agents_input(app, key),
        AppMode::EditAgent => return handle_edit_agent_input(app, key),
        AppMode::EditAgentRole => return handle_edit_agent_role_input(app, key),
        AppMode::ImportAgents => return handle_import_agents_input(app, key),
        AppMode::DispatchCardMove => return handle_dispatch_card_move_input(app, key),
        AppMode::ManageProviders => return handle_manage_providers_input(app, key),
        AppMode::EditProvider => return handle_edit_provider_input(app, key),
        AppMode::ChatPanel => return chat_input::handle_chat_panel_input(app, key),
        // Normal and Diff modes fall through to navigation/interaction handling
        AppMode::Normal | AppMode::Diff => {}
    }

    // Clear status message, toast, and selection on any key
    app.status_message = None;
    app.toast = None;
    app.selection = None;

    if app.interacting {
        handle_interaction_mode(app, key)
    } else {
        handle_navigation_mode(app, key)
    }
}

// ── Navigation mode: hjkl between panes, Enter to interact, global shortcuts ──

pub(crate) fn handle_navigation_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    fn nav(app: &App, key: KeyEvent, action: &str) -> bool {
        app.config.matches_navigation(key, action)
    }
    let nav = |app: &App, a: &str| nav(app, key, a);
    if nav(app, "left") || nav(app, "left_alt") {
        app_actions::focus_left(app)
    } else if nav(app, "right") || nav(app, "right_alt") {
        app_actions::focus_right(app)
    } else if nav(app, "down") || nav(app, "down_alt") {
        app_actions::focus_down(app)
    } else if nav(app, "up") || nav(app, "up_alt") {
        app_actions::focus_up(app)
    } else if nav(app, "enter_pane") {
        app.interacting = true;
        None
    } else if nav(app, "quit") {
        app_actions::open_confirm_quit(app)
    } else if nav(app, "help") {
        app_actions::open_help(app)
    } else if nav(app, "about") {
        app_actions::open_about(app)
    } else if nav(app, "dashboard") {
        app_actions::open_dashboard(app)
    } else if nav(app, "logs") {
        app_actions::open_logs(app)
    } else if nav(app, "workspace_info") {
        app_actions::open_workspace_info(app)
    } else if nav(app, "edit_workspace") {
        app_actions::open_edit_workspace(app)
    } else if nav(app, "clone_workspace") {
        app_actions::open_clone_workspace(app)
    } else if nav(app, "new_workspace") {
        app_actions::open_new_workspace(app)
    } else if nav(app, "delete_workspace") {
        app_actions::open_delete_workspace(app)
    } else if nav(app, "commit") {
        app_actions::open_commit_dialog(app)
    } else if nav(app, "merge") {
        app_actions::open_confirm_merge(app)
    } else if nav(app, "push") {
        app_actions::git_push(app)
    } else if nav(app, "stash") {
        app_actions::git_stash_list(app)
    } else if nav(app, "git_log") {
        app_actions::git_log(app)
    } else if key.code == KeyCode::Char('A')
        && app
            .current_workspace()
            .is_some_and(|ws| ws.info.workspace_type == piki_core::WorkspaceType::Simple)
    {
        app_actions::open_manage_agents(app)
    } else if key.code == KeyCode::Char('p')
        && key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
    {
        app_actions::open_manage_providers(app)
    } else if nav(app, "conflicts") {
        app_actions::detect_conflicts(app)
    } else if nav(app, "chat_panel") {
        app_actions::open_chat_panel(app)
    } else if nav(app, "undo") {
        app_actions::undo(app)
    } else if nav(app, "next_workspace") {
        app_actions::cycle_next_by_pane(app)
    } else if nav(app, "prev_workspace") {
        app_actions::cycle_prev_by_pane(app)
    } else if nav(app, "scroll_up") {
        app_actions::term_scroll_up(app, 3)
    } else if nav(app, "scroll_down") {
        app_actions::term_scroll_down(app, 3)
    } else if nav(app, "page_up") {
        app_actions::term_page_up(app)
    } else if nav(app, "page_down") {
        app_actions::term_page_down(app)
    } else if nav(app, "copy") {
        copy_visible_terminal(app);
        None
    } else if nav(app, "fuzzy_search") || nav(app, "fuzzy_search_alt") {
        app.open_fuzzy_search();
        None
    } else if nav(app, "command_palette") {
        app.open_command_palette();
        None
    } else if nav(app, "workspace_switcher") {
        app.open_workspace_switcher();
        None
    } else if nav(app, "toggle_prev_workspace") {
        app.toggle_previous_workspace();
        None
    } else if nav(app, "sidebar_shrink") || nav(app, "sidebar_shrink_alt") {
        app_actions::sidebar_shrink(app)
    } else if nav(app, "sidebar_grow") || nav(app, "sidebar_grow_alt") {
        app_actions::sidebar_grow(app)
    } else if nav(app, "split_up") || nav(app, "split_up_alt") {
        app_actions::split_up(app)
    } else if nav(app, "split_down") {
        app_actions::split_down(app)
    } else if nav(app, "next_tab") {
        app_actions::cycle_next_tab(app)
    } else if nav(app, "prev_tab") {
        app_actions::cycle_prev_tab(app)
    } else if nav(app, "new_tab") {
        app_actions::open_new_tab(app)
    } else if nav(app, "close_tab") {
        app_actions::request_close_tab(app)
    } else if nav(app, "stage_quick") {
        app_actions::stage_quick(app)
    } else if nav(app, "unstage_quick") {
        app_actions::unstage_quick(app)
    } else {
        None
    }
}

// ── Interaction mode: Esc to leave, keys go to the active pane ──

fn handle_interaction_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    match app.active_pane {
        ActivePane::MainPanel => {
            if app.mode == AppMode::Diff {
                handle_diff_interaction(app, key)
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.api_state.is_some())
            {
                handle_api_interaction(app, key)
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.provider == piki_core::AIProvider::Kanban)
            {
                handle_kanban_interaction(app, key)
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.markdown_content.is_some())
            {
                handle_markdown_interaction(app, key)
            } else {
                handle_terminal_interaction(app, key)
            }
        }
        ActivePane::WorkspaceList => handle_workspace_interaction(app, key),
        ActivePane::GitStatus => handle_filelist_interaction(app, key),
    }
}
