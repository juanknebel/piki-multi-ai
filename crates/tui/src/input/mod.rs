mod dialog;
mod editor_input;
mod fuzzy_input;
mod interaction;
pub(crate) mod mouse;

use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::dialog_state::DialogState;
use crate::helpers::{copy_visible_terminal, resize_all_ptys, scrollback_max};

use self::dialog::{
    handle_about_input, handle_commit_message_input, handle_confirm_close_tab_input,
    handle_confirm_delete_input, handle_confirm_merge_input, handle_confirm_quit_input,
    handle_dashboard_input, handle_edit_workspace_input, handle_help_input, handle_new_tab_input,
    handle_new_workspace_input, handle_workspace_info_input,
};
use self::editor_input::handle_inline_edit_input;
use self::fuzzy_input::handle_fuzzy_search_input;
use self::interaction::{
    handle_diff_interaction, handle_filelist_interaction, handle_kanban_interaction,
    handle_markdown_interaction, handle_terminal_interaction, handle_workspace_interaction,
};

pub(crate) fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Modal dispatch — each mode captures all input
    match app.mode {
        AppMode::WorkspaceInfo => return handle_workspace_info_input(app, key),
        AppMode::About => return handle_about_input(app, key),
        AppMode::Help => return handle_help_input(app, key),
        AppMode::FuzzySearch => return handle_fuzzy_search_input(app, key),
        AppMode::InlineEdit => return handle_inline_edit_input(app, key),
        AppMode::NewWorkspace => return handle_new_workspace_input(app, key),
        AppMode::EditWorkspace => return handle_edit_workspace_input(app, key),
        AppMode::CommitMessage => return handle_commit_message_input(app, key),
        AppMode::ConfirmMerge => return handle_confirm_merge_input(app, key),
        AppMode::NewTab => return handle_new_tab_input(app, key),
        AppMode::ConfirmCloseTab => return handle_confirm_close_tab_input(app, key),
        AppMode::ConfirmQuit => return handle_confirm_quit_input(app, key),
        AppMode::Dashboard => return handle_dashboard_input(app, key),
        AppMode::ConfirmDelete => return handle_confirm_delete_input(app, key),
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

fn handle_navigation_mode(app: &mut App, key: KeyEvent) -> Option<Action> {
    // Pane navigation
    if app.config.matches_navigation(key, "left") || app.config.matches_navigation(key, "left_alt")
    {
        if app.active_pane == ActivePane::MainPanel {
            app.active_pane = ActivePane::GitStatus;
        }
    } else if app.config.matches_navigation(key, "right")
        || app.config.matches_navigation(key, "right_alt")
    {
        if matches!(
            app.active_pane,
            ActivePane::WorkspaceList | ActivePane::GitStatus
        ) {
            app.active_pane = ActivePane::MainPanel;
        }
    } else if app.config.matches_navigation(key, "down")
        || app.config.matches_navigation(key, "down_alt")
    {
        if app.active_pane == ActivePane::WorkspaceList {
            app.active_pane = ActivePane::GitStatus;
        }
    } else if app.config.matches_navigation(key, "up")
        || app.config.matches_navigation(key, "up_alt")
    {
        if app.active_pane == ActivePane::GitStatus {
            app.active_pane = ActivePane::WorkspaceList;
        }
    } else if app.config.matches_navigation(key, "enter_pane") {
        app.interacting = true;
    } else if app.config.matches_navigation(key, "quit") {
        app.active_dialog = Some(DialogState::ConfirmQuit);
        app.mode = AppMode::ConfirmQuit;
    } else if app.config.matches_navigation(key, "help") {
        app.active_dialog = Some(DialogState::Help { scroll: 0 });
        app.mode = AppMode::Help;
    } else if app.config.matches_navigation(key, "about") {
        app.active_dialog = Some(DialogState::About);
        app.mode = AppMode::About;
    } else if app.config.matches_navigation(key, "dashboard") {
        if !app.workspaces.is_empty() {
            app.active_dialog = Some(DialogState::Dashboard {
                selected: app.active_workspace,
                scroll_offset: 0,
            });
            app.mode = AppMode::Dashboard;
        }
    } else if app.config.matches_navigation(key, "workspace_info") {
        if !app.workspaces.is_empty() {
            app.active_dialog = Some(DialogState::WorkspaceInfo { hscroll: 0 });
            app.mode = AppMode::WorkspaceInfo;
            let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture);
        }
    } else if app.config.matches_navigation(key, "edit_workspace") {
        if !app.workspaces.is_empty() {
            let ws = &app.workspaces[app.selected_workspace];
            let kanban = ws.kanban_path.clone().unwrap_or_default();
            let prompt = ws.prompt.clone();
            let group = ws.info.group.clone().unwrap_or_default();
            app.active_dialog = Some(DialogState::EditWorkspace {
                target: app.selected_workspace,
                kanban_cursor: kanban.chars().count(),
                kanban,
                prompt_cursor: prompt.chars().count(),
                prompt,
                group_cursor: group.chars().count(),
                group,
                active_field: DialogField::KanbanPath,
            });
            app.mode = AppMode::EditWorkspace;
        }
    } else if app.config.matches_navigation(key, "clone_workspace") {
        if !app.workspaces.is_empty() {
            let ws = &app.workspaces[app.selected_workspace];
            let dir = ws.source_repo.display().to_string();
            let kanban = ws.kanban_path.clone().unwrap_or_default();
            let prompt = ws.prompt.clone();
            let group = ws.info.group.clone().unwrap_or_default();
            let ws_type = ws.info.workspace_type;
            app.active_dialog = Some(DialogState::NewWorkspace {
                name: String::new(),
                name_cursor: 0,
                dir_cursor: dir.chars().count(),
                dir,
                desc: String::new(),
                desc_cursor: 0,
                prompt_cursor: prompt.chars().count(),
                prompt,
                kanban_cursor: kanban.chars().count(),
                kanban,
                group_cursor: group.chars().count(),
                group,
                ws_type,
                active_field: DialogField::Type,
            });
            app.mode = AppMode::NewWorkspace;
        }
    } else if app.config.matches_navigation(key, "new_workspace") {
        app.active_dialog = Some(DialogState::NewWorkspace {
            name: String::new(),
            name_cursor: 0,
            dir: String::new(),
            dir_cursor: 0,
            desc: String::new(),
            desc_cursor: 0,
            prompt: String::new(),
            prompt_cursor: 0,
            kanban: String::new(),
            kanban_cursor: 0,
            group: String::new(),
            group_cursor: 0,
            ws_type: piki_core::WorkspaceType::default(),
            active_field: DialogField::Type,
        });
        app.mode = AppMode::NewWorkspace;
    } else if app.config.matches_navigation(key, "delete_workspace") {
        if !app.workspaces.is_empty() {
            app.active_dialog = Some(DialogState::ConfirmDelete {
                target: app.selected_workspace,
            });
            app.mode = AppMode::ConfirmDelete;
        }
    } else if app.config.matches_navigation(key, "commit") {
        if let Some(ws) = app.current_workspace()
            && ws.info.workspace_type != piki_core::WorkspaceType::Project
        {
            app.active_dialog = Some(DialogState::CommitMessage {
                buffer: String::new(),
            });
            app.mode = AppMode::CommitMessage;
        }
    } else if app.config.matches_navigation(key, "merge") {
        if let Some(ws) = app.current_workspace()
            && ws.info.workspace_type != piki_core::WorkspaceType::Project
        {
            app.active_dialog = Some(DialogState::ConfirmMerge);
            app.mode = AppMode::ConfirmMerge;
        }
    } else if app.config.matches_navigation(key, "push") {
        if let Some(ws) = app.current_workspace()
            && ws.info.workspace_type != piki_core::WorkspaceType::Project
        {
            return Some(Action::GitPush);
        }
    } else if app.config.matches_navigation(key, "undo") {
        return Some(Action::Undo);
    } else if app.config.matches_navigation(key, "next_workspace") {
        app.next_workspace();
    } else if app.config.matches_navigation(key, "prev_workspace") {
        app.prev_workspace();
    } else if app.config.matches_navigation(key, "scroll_up") {
        if app.active_pane == ActivePane::MainPanel
            && app.mode == AppMode::Normal
            && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
            && let Some(ref parser) = tab.pty_parser
        {
            let max = scrollback_max(parser);
            tab.term_scroll = (tab.term_scroll + 3).min(max);
        }
    } else if app.config.matches_navigation(key, "scroll_down") {
        if app.active_pane == ActivePane::MainPanel
            && app.mode == AppMode::Normal
            && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
        {
            tab.term_scroll = tab.term_scroll.saturating_sub(3);
        }
    } else if app.config.matches_navigation(key, "page_up") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
            && let Some(ref parser) = tab.pty_parser
        {
            let screen_height = app.pty_rows as usize;
            let max = scrollback_max(parser);
            tab.term_scroll = (tab.term_scroll + screen_height).min(max);
        }
    } else if app.config.matches_navigation(key, "page_down") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
        {
            let screen_height = app.pty_rows as usize;
            tab.term_scroll = tab.term_scroll.saturating_sub(screen_height);
        }
    } else if app.config.matches_navigation(key, "copy") {
        copy_visible_terminal(app);
    } else if app.config.matches_navigation(key, "fuzzy_search")
        || app.config.matches_navigation(key, "fuzzy_search_alt")
    {
        app.open_fuzzy_search();
    } else if app.config.matches_navigation(key, "sidebar_shrink")
        || app.config.matches_navigation(key, "sidebar_shrink_alt")
    {
        app.sidebar_pct = app.sidebar_pct.saturating_sub(5).max(10);
        resize_all_ptys(app);
    } else if app.config.matches_navigation(key, "sidebar_grow")
        || app.config.matches_navigation(key, "sidebar_grow_alt")
    {
        app.sidebar_pct = (app.sidebar_pct + 5).min(90);
        resize_all_ptys(app);
    } else if app.config.matches_navigation(key, "split_up")
        || app.config.matches_navigation(key, "split_up_alt")
    {
        app.left_split_pct = (app.left_split_pct + 10).min(90);
    } else if app.config.matches_navigation(key, "split_down") {
        app.left_split_pct = app.left_split_pct.saturating_sub(10).max(10);
    } else if app.config.matches_navigation(key, "next_tab") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && !ws.tabs.is_empty()
        {
            ws.active_tab = (ws.active_tab + 1) % ws.tabs.len();
        }
    } else if app.config.matches_navigation(key, "prev_tab") {
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && !ws.tabs.is_empty()
        {
            ws.active_tab = (ws.active_tab + ws.tabs.len() - 1) % ws.tabs.len();
        }
    } else if app.config.matches_navigation(key, "new_tab") {
        if app.current_workspace().is_some() {
            app.active_dialog = Some(DialogState::NewTab);
            app.mode = AppMode::NewTab;
        }
    } else if app.config.matches_navigation(key, "close_tab") {
        if let Some(ws) = app.workspaces.get(app.active_workspace) {
            if ws.current_tab().is_some_and(|t| t.closable) {
                app.active_dialog = Some(DialogState::ConfirmCloseTab {
                    target: ws.active_tab,
                });
                app.mode = AppMode::ConfirmCloseTab;
            } else {
                app.status_message = Some("Cannot close the initial shell tab".into());
            }
        }
    } else if let KeyCode::Char(c @ '1'..='9') = key.code {
        let idx = (c as usize) - ('1' as usize);
        app.switch_workspace(idx);
    }
    None
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
