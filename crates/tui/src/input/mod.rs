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
use crate::app::{ActivePane, App, AppMode, InputState};

use self::command_palette_input::handle_command_palette_input;
use self::dialog::{
    handle_about_input, handle_confirm_close_tab_input,
    handle_confirm_delete_input, handle_confirm_quit_input,
    handle_create_worktree_input, handle_dashboard_input,
    handle_dispatch_agent_input, handle_dispatch_card_move_input, handle_edit_agent_input,
    handle_edit_agent_role_input, handle_edit_provider_input, handle_edit_workspace_input,
    handle_help_input, handle_import_agents_input,
    handle_logs_input, handle_manage_agents_input, handle_manage_providers_input,
    handle_new_tab_input, handle_new_workspace_input, handle_workspace_info_input,
};
use self::editor_input::handle_inline_edit_input;
use self::fuzzy_input::handle_fuzzy_search_input;
use self::interaction::{
    handle_agents_interaction, handle_api_interaction,
    handle_kanban_interaction, handle_markdown_interaction, handle_terminal_interaction,
    
};

/// Handle a bracketed paste event — insert full text at once into the active context.
pub(crate) fn handle_paste(app: &mut App, text: &str) {
    // A paste while a prefix chord is pending cancels the chord
    if app.input_state == InputState::PrefixPending {
        app.input_state = InputState::Normal;
    }
    // Focused terminal: write to PTY
    if app.active_pane == ActivePane::MainPanel
        && app.mode == AppMode::Normal
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
                state.refilter();
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
        AppMode::DispatchAgent => return handle_dispatch_agent_input(app, key),
        AppMode::ManageAgents => return handle_manage_agents_input(app, key),
        AppMode::EditAgent => return handle_edit_agent_input(app, key),
        AppMode::EditAgentRole => return handle_edit_agent_role_input(app, key),
        AppMode::ImportAgents => return handle_import_agents_input(app, key),
        AppMode::DispatchCardMove => return handle_dispatch_card_move_input(app, key),
        AppMode::ManageProviders => return handle_manage_providers_input(app, key),
        AppMode::EditProvider => return handle_edit_provider_input(app, key),
        AppMode::ChatPanel => return chat_input::handle_chat_panel_input(app, key),
        // Normal mode falls through to the prefix/pane dispatch
        AppMode::Normal => {}
    }

    // Clear status message, toast, and selection on any key
    app.status_message = None;
    app.toast = None;
    app.selection = None;

    // The terminal search overlay captures everything, including the prefix key
    if app.active_pane == ActivePane::MainPanel && app.term_search.is_some() {
        return interaction::handle_term_search_key(app, key);
    }

    match app.input_state {
        InputState::PrefixPending => {
            // One-shot: always reset before dispatching
            app.input_state = InputState::Normal;
            if key.code == KeyCode::Esc {
                return None;
            }
            if app.config.is_prefix_key(key) {
                send_literal_prefix(app);
                return None;
            }
            handle_prefix_key(app, key)
        }
        InputState::TermScroll => {
            if app.config.is_prefix_key(key) {
                app_actions::exit_term_scroll(app);
                app.input_state = InputState::PrefixPending;
                return None;
            }
            handle_term_scroll_key(app, key)
        }
        InputState::Normal => {
            if app.config.is_prefix_key(key) {
                app.input_state = InputState::PrefixPending;
                return None;
            }
            if let Some(result) = try_direct_app_binding(app, key) {
                return result;
            }
            handle_pane_key(app, key)
        }
    }
}

/// App actions addressable from the `[keybindings.app]` table, minus the
/// terminal clipboard/search chords (those stay pane-scoped so e.g. copy in an
/// API tab can copy the response instead of the terminal screen).
const APP_ACTIONS: &[&str] = &[
    "focus_left",
    "focus_down",
    "focus_up",
    "focus_right",
    "new_tab",
    "close_tab",
    "next_tab",
    "prev_tab",
    "workspace_switcher",
    "next_workspace",
    "prev_workspace",
    "toggle_prev_workspace",
    "new_workspace",
    "edit_workspace",
    "delete_workspace",
    "workspace_info",
    "clone_workspace",
    "git",
    "terminal_search",
    "help",
    "about",
    "dashboard",
    "command_palette",
    "fuzzy_search",
    "chat_panel",
    "quit",
    "manage_agents",
    "manage_providers",
    "logs",
    "scroll_mode",
    "sidebar_shrink",
    "sidebar_grow",
    "split_up",
    "split_down",
];

/// Execute an `[keybindings.app]` action by name.
fn dispatch_app_action(app: &mut App, action: &str) -> Option<Action> {
    match action {
        "focus_left" => app_actions::focus_left(app),
        "focus_down" => app_actions::focus_down(app),
        "focus_up" => app_actions::focus_up(app),
        "focus_right" => app_actions::focus_right(app),
        "new_tab" => app_actions::open_new_tab(app),
        "close_tab" => app_actions::request_close_tab(app),
        "next_tab" => app_actions::cycle_next_tab(app),
        "prev_tab" => app_actions::cycle_prev_tab(app),
        "workspace_switcher" => {
            app.open_workspace_switcher();
            None
        }
        "next_workspace" => {
            app.next_workspace();
            None
        }
        "prev_workspace" => {
            app.prev_workspace();
            None
        }
        "toggle_prev_workspace" => {
            app.toggle_previous_workspace();
            None
        }
        "new_workspace" => app_actions::open_new_workspace(app),
        "edit_workspace" => app_actions::open_edit_workspace(app),
        "delete_workspace" => app_actions::open_delete_workspace(app),
        "workspace_info" => app_actions::open_workspace_info(app),
        "clone_workspace" => app_actions::open_clone_workspace(app),
        "git" => app_actions::open_git_tab(app),
        "terminal_search" => app_actions::open_terminal_search(app),
        "help" => app_actions::open_help(app),
        "about" => app_actions::open_about(app),
        "dashboard" => app_actions::open_dashboard(app),
        "command_palette" => {
            app.open_command_palette();
            None
        }
        "fuzzy_search" => {
            app.open_fuzzy_search();
            None
        }
        "chat_panel" => app_actions::open_chat_panel(app),
        "quit" => app_actions::open_confirm_quit(app),
        "manage_agents" => app_actions::open_manage_agents(app),
        "manage_providers" => app_actions::open_manage_providers(app),
        "logs" => app_actions::open_logs(app),
        "scroll_mode" => app_actions::enter_term_scroll(app),
        "sidebar_shrink" => app_actions::sidebar_shrink(app),
        "sidebar_grow" => app_actions::sidebar_grow(app),
        "split_up" => app_actions::split_up(app),
        "split_down" => app_actions::split_down(app),
        _ => None,
    }
}

/// Dispatch the key following the prefix chord against the app table.
fn handle_prefix_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    // prefix 1..9 → jump to tab N (not configurable, like tmux)
    if let KeyCode::Char(c @ '1'..='9') = key.code
        && (key.modifiers.is_empty() || key.modifiers == crossterm::event::KeyModifiers::NONE)
    {
        let n = c as usize - '1' as usize;
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && n < ws.tabs.len()
        {
            ws.active_tab = n;
        }
        return None;
    }
    for action in APP_ACTIONS {
        if app.config.matches_app_prefix(key, action) {
            return dispatch_app_action(app, action);
        }
    }
    app.set_toast(
        format!("Unbound key after {}", app.config.prefix_display()),
        crate::app::ToastLevel::Info,
    );
    None
}

/// Direct (non-prefix) chords from the app table, e.g. a user-promoted
/// `next_tab = "alt-n"`. Defaults have none (copy/paste/search are pane-scoped).
fn try_direct_app_binding(app: &mut App, key: KeyEvent) -> Option<Option<Action>> {
    for action in APP_ACTIONS {
        if app.config.matches_app_direct(key, action) {
            return Some(dispatch_app_action(app, action));
        }
    }
    None
}

/// Send the prefix key literally to the focused terminal (prefix-prefix).
/// Writes BEL (0x07) for the default Ctrl+G; other prefixes go through the
/// regular key-to-bytes conversion.
fn send_literal_prefix(app: &mut App) {
    if app.active_pane != ActivePane::MainPanel {
        return;
    }
    let bytes = if app.config.keybindings.prefix_key == "ctrl-g" {
        Some(vec![0x07])
    } else {
        crate::config::parse_key_event(&app.config.keybindings.prefix_key)
            .and_then(crate::pty::input::key_to_bytes)
    };
    if let Some(bytes) = bytes
        && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
        && let Some(ref mut pty) = tab.pty_session
    {
        let _ = pty.write(&bytes);
    }
}

/// Terminal scroll mode (`prefix [`): navigate the scrollback of the focused
/// terminal, tmux-copy-mode style.
fn handle_term_scroll_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    fn m(app: &App, key: KeyEvent, action: &str) -> bool {
        app.config.matches_scroll(key, action)
    }
    if m(app, key, "exit") || m(app, key, "exit_alt") {
        app_actions::exit_term_scroll(app);
    } else if m(app, key, "down") || m(app, key, "down_alt") {
        app_actions::term_scroll_down(app, 1);
    } else if m(app, key, "up") || m(app, key, "up_alt") {
        app_actions::term_scroll_up(app, 1);
    } else if m(app, key, "page_down") || m(app, key, "page_down_alt") {
        app_actions::term_page_down(app);
    } else if m(app, key, "page_up") || m(app, key, "page_up_alt") {
        app_actions::term_page_up(app);
    } else if m(app, key, "top") {
        app_actions::term_scroll_top(app);
    } else if m(app, key, "bottom") {
        app_actions::term_scroll_bottom(app);
    } else if m(app, key, "search") {
        app.term_search = Some(crate::app::TermSearchState {
            query: String::new(),
            cursor: 0,
            matches: Vec::new(),
            current_match: 0,
        });
    }
    None
}

// ── Pane routing: keys go to the focused pane ──

fn handle_pane_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match app.active_pane {
        ActivePane::MainPanel => {
            if app
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
        // The workspace list is display-only: workspace actions live behind
        // the prefix (switcher, new/edit/delete), never on bare keys.
        ActivePane::WorkspaceList => None,
        ActivePane::Agents => handle_agents_interaction(app, key),
    }
}
