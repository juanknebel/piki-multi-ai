mod chat_input;
mod code_review_input;
mod command_palette_input;
pub(crate) mod confirm_common;
mod dialog;
mod editor_input;
pub(crate) mod fuzzy_common;
mod fuzzy_input;
mod interaction;
pub(crate) mod mouse;
pub(crate) mod text_field_common;
mod workspace_switcher_input;

use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::dialog_state::DialogState;
use crate::helpers::{copy_visible_terminal, resize_all_ptys, scrollback_max};

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
    handle_new_tab_input, handle_new_workspace_input,
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
    // Pane navigation
    if app.config.matches_navigation(key, "left") || app.config.matches_navigation(key, "left_alt")
    {
        if app.active_pane == ActivePane::MainPanel {
            app.active_pane = ActivePane::WorkspaceList;
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
        match app.active_pane {
            ActivePane::WorkspaceList => app.active_pane = ActivePane::GitStatus,
            ActivePane::MainPanel => app.active_pane = ActivePane::GitStatus,
            _ => {}
        }
    } else if app.config.matches_navigation(key, "up")
        || app.config.matches_navigation(key, "up_alt")
    {
        match app.active_pane {
            ActivePane::GitStatus => app.active_pane = ActivePane::WorkspaceList,
            ActivePane::MainPanel => app.active_pane = ActivePane::WorkspaceList,
            _ => {}
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
    } else if app.config.matches_navigation(key, "logs") {
        app.active_dialog = Some(DialogState::Logs {
            scroll: u16::MAX,
            level_filter: 0,
            selected: usize::MAX,
            hscroll: 0,
        });
        app.mode = AppMode::Logs;
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
    } else if app.config.matches_navigation(key, "stash") {
        if let Some(ws) = app.current_workspace()
            && ws.info.workspace_type != piki_core::WorkspaceType::Project
        {
            return Some(Action::GitStashList);
        }
    } else if app.config.matches_navigation(key, "git_log") {
        if app.current_workspace().is_some() {
            return Some(Action::LoadGitLog);
        }
    } else if key.code == KeyCode::Char('A')
        && app
            .current_workspace()
            .is_some_and(|ws| ws.info.workspace_type == piki_core::WorkspaceType::Simple)
    {
        // Load agents for current project before opening the overlay (Simple ws only)
        if let Some(ref storage) = app.storage.agent_profiles
            && let Some(ws) = app.current_workspace()
        {
            let repo = ws.source_repo.clone();
            if let Ok(agents) = storage.load_agents(&repo) {
                app.agent_profiles = agents;
            }
        }
        app.active_dialog = Some(DialogState::ManageAgents { selected: 0 });
        app.mode = AppMode::ManageAgents;
    } else if key.code == KeyCode::Char('p') && key.modifiers.contains(crossterm::event::KeyModifiers::ALT) {
        // Open providers manager (Alt+P)
        app.active_dialog = Some(DialogState::ManageProviders { selected: 0 });
        app.mode = AppMode::ManageProviders;
    } else if app.config.matches_navigation(key, "conflicts") {
        if let Some(ws) = app.current_workspace()
            && ws.info.workspace_type != piki_core::WorkspaceType::Project
        {
            return Some(Action::DetectConflicts);
        }
    } else if app.config.matches_navigation(key, "chat_panel") {
        app.mode = AppMode::ChatPanel;
        if app.chat_panel.models.is_empty() {
            return Some(Action::ChatLoadModels);
        }
    } else if app.config.matches_navigation(key, "undo") {
        return Some(Action::Undo);
    } else if app.config.matches_navigation(key, "next_workspace") {
        match app.active_pane {
            ActivePane::WorkspaceList => app.next_workspace(),
            ActivePane::MainPanel => {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && !ws.tabs.is_empty()
                {
                    ws.active_tab = (ws.active_tab + 1) % ws.tabs.len();
                }
            }
            ActivePane::GitStatus => app.next_file(),
        }
    } else if app.config.matches_navigation(key, "prev_workspace") {
        match app.active_pane {
            ActivePane::WorkspaceList => app.prev_workspace(),
            ActivePane::MainPanel => {
                if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                    && !ws.tabs.is_empty()
                {
                    ws.active_tab = (ws.active_tab + ws.tabs.len() - 1) % ws.tabs.len();
                }
            }
            ActivePane::GitStatus => app.prev_file(),
        }
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
    } else if app.config.matches_navigation(key, "command_palette") {
        app.open_command_palette();
    } else if app.config.matches_navigation(key, "workspace_switcher") {
        app.open_workspace_switcher();
    } else if app.config.matches_navigation(key, "toggle_prev_workspace") {
        app.toggle_previous_workspace();
    } else if app.config.matches_navigation(key, "sidebar_shrink")
        || app.config.matches_navigation(key, "sidebar_shrink_alt")
    {
        app.sidebar_pct = app.sidebar_pct.saturating_sub(5).max(10);
        resize_all_ptys(app);
        app.save_layout_prefs();
    } else if app.config.matches_navigation(key, "sidebar_grow")
        || app.config.matches_navigation(key, "sidebar_grow_alt")
    {
        app.sidebar_pct = (app.sidebar_pct + 5).min(90);
        resize_all_ptys(app);
        app.save_layout_prefs();
    } else if app.config.matches_navigation(key, "split_up")
        || app.config.matches_navigation(key, "split_up_alt")
    {
        app.left_split_pct = (app.left_split_pct + 10).min(90);
        app.save_layout_prefs();
    } else if app.config.matches_navigation(key, "split_down") {
        app.left_split_pct = app.left_split_pct.saturating_sub(10).max(10);
        app.save_layout_prefs();
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
            app.active_dialog = Some(DialogState::NewTab {
                menu: crate::dialog_state::NewTabMenu::Main,
            });
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
    } else if app.config.matches_navigation(key, "stage_quick") {
        // Quick stage without entering interaction mode
        if app.active_pane == ActivePane::GitStatus
            && let Some(ws) = app.current_workspace()
            && !ws.changed_files.is_empty()
        {
            if app.selected_files.is_empty() {
                return Some(Action::GitStage(app.selected_file));
            } else {
                return Some(Action::GitStageSelected);
            }
        }
    } else if app.config.matches_navigation(key, "unstage_quick") {
        // Quick unstage without entering interaction mode
        if app.active_pane == ActivePane::GitStatus
            && let Some(ws) = app.current_workspace()
            && !ws.changed_files.is_empty()
        {
            if app.selected_files.is_empty() {
                return Some(Action::GitUnstage(app.selected_file));
            } else {
                return Some(Action::GitUnstageSelected);
            }
        }
    } else if let KeyCode::Char(c @ '1'..='9') = key.code {
        let visual_pos = (c as usize) - ('1' as usize);
        // Map visual position to actual workspace index via sidebar order
        let ws_indices: Vec<usize> = app
            .sidebar_items()
            .iter()
            .filter_map(|item| match item {
                crate::app::SidebarItem::Workspace { index } => Some(*index),
                _ => None,
            })
            .collect();
        if let Some(&ws_idx) = ws_indices.get(visual_pos) {
            app.switch_workspace(ws_idx);
        } else {
            app.set_toast(
                format!(
                    "No workspace {} (have {})",
                    visual_pos + 1,
                    ws_indices.len()
                ),
                crate::app::ToastLevel::Info,
            );
        }
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
