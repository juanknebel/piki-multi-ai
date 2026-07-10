//! Global app actions, one function per action.
//!
//! These are the bodies of the former navigation-mode arms, extracted so the
//! key-matching layer (the prefix dispatcher and pane routing) stays
//! a thin table. All helpers return `Option<Action>` so callers can uniformly
//! `return` their result.

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField};
use crate::dialog_state::{DialogState, EditWorkspaceField};
use crate::helpers::{resize_all_ptys, scrollback_max};

// ── Focus movement between panes ──

pub(crate) fn focus_left(app: &mut App) -> Option<Action> {
    if app.active_pane == ActivePane::MainPanel {
        app.active_pane = ActivePane::WorkspaceList;
    }
    None
}

pub(crate) fn focus_right(app: &mut App) -> Option<Action> {
    if matches!(
        app.active_pane,
        ActivePane::WorkspaceList | ActivePane::Agents
    ) {
        app.active_pane = ActivePane::MainPanel;
    }
    None
}

pub(crate) fn focus_down(app: &mut App) -> Option<Action> {
    match app.active_pane {
        ActivePane::WorkspaceList | ActivePane::MainPanel => {
            app.active_pane = ActivePane::Agents;
        }
        _ => {}
    }
    None
}

pub(crate) fn focus_up(app: &mut App) -> Option<Action> {
    match app.active_pane {
        ActivePane::Agents | ActivePane::MainPanel => {
            app.active_pane = ActivePane::WorkspaceList;
        }
        _ => {}
    }
    None
}

// ── Dialogs and overlays ──

pub(crate) fn open_confirm_quit(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::ConfirmQuit);
    app.mode = AppMode::ConfirmQuit;
    None
}

pub(crate) fn open_help(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::Help { scroll: 0 });
    app.mode = AppMode::Help;
    None
}

pub(crate) fn open_about(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::About);
    app.mode = AppMode::About;
    None
}

pub(crate) fn open_dashboard(app: &mut App) -> Option<Action> {
    if !app.workspaces.is_empty() {
        app.active_dialog = Some(DialogState::Dashboard {
            selected: app.active_workspace,
            scroll_offset: 0,
        });
        app.mode = AppMode::Dashboard;
    }
    None
}

pub(crate) fn open_logs(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::Logs {
        scroll: u16::MAX,
        level_filter: 0,
        selected: usize::MAX,
        hscroll: 0,
        search_active: false,
        search_buffer: String::new(),
        search_cursor: 0,
        auto_refresh: true,
    });
    app.mode = AppMode::Logs;
    None
}

pub(crate) fn open_workspace_info(app: &mut App) -> Option<Action> {
    if !app.workspaces.is_empty() {
        app.active_dialog = Some(DialogState::WorkspaceInfo { hscroll: 0 });
        app.mode = AppMode::WorkspaceInfo;
        let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture);
    }
    None
}

pub(crate) fn open_edit_workspace(app: &mut App) -> Option<Action> {
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
            active_field: EditWorkspaceField::KanbanPath,
        });
        app.mode = AppMode::EditWorkspace;
    }
    None
}

pub(crate) fn open_clone_workspace(app: &mut App) -> Option<Action> {
    // Layer 3: the former "Clone workspace" action is now "Create Worktree",
    // available only when the selected workspace has a GitHub origin.
    if let Some(ws) = app.workspaces.get(app.selected_workspace) {
        match &ws.info.origin {
            piki_core::WorkspaceOrigin::GitHub { .. } => {
                let kanban = ws.kanban_path.clone().unwrap_or_default();
                let prompt = ws.prompt.clone();
                let group = ws.info.group.clone().unwrap_or_default();
                app.active_dialog = Some(crate::dialog_state::DialogState::CreateWorktree {
                    parent_idx: app.selected_workspace,
                    mode: crate::dialog_state::CreateWorktreeMode::ChooseSource,
                    name: String::new(),
                    name_cursor: 0,
                    prompt_cursor: prompt.chars().count(),
                    prompt,
                    kanban_cursor: kanban.chars().count(),
                    kanban,
                    group_cursor: group.chars().count(),
                    group,
                    active_field: crate::dialog_state::CreateWorktreeField::Name,
                    existing: Vec::new(),
                    existing_selected: 0,
                    existing_loading: false,
                });
                app.mode = AppMode::CreateWorktree;
            }
            piki_core::WorkspaceOrigin::Local => {
                app.status_message =
                    Some("Create Worktree is available only for GitHub workspaces".into());
            }
        }
    }
    None
}

pub(crate) fn open_new_workspace(app: &mut App) -> Option<Action> {
    let default_dest = app.paths.repos_dir().to_string_lossy().to_string();
    let default_dest_cursor = default_dest.len();
    app.active_dialog = Some(DialogState::NewWorkspace {
        name: String::new(),
        name_cursor: 0,
        dir: String::new(),
        dir_cursor: 0,
        destination: default_dest,
        destination_cursor: default_dest_cursor,
        desc: String::new(),
        desc_cursor: 0,
        prompt: String::new(),
        prompt_cursor: 0,
        kanban: String::new(),
        kanban_cursor: 0,
        group: String::new(),
        group_cursor: 0,
        source: crate::app::NewWorkspaceSource::default(),
        active_field: DialogField::Source,
    });
    app.mode = AppMode::NewWorkspace;
    None
}

pub(crate) fn open_delete_workspace(app: &mut App) -> Option<Action> {
    if !app.workspaces.is_empty() {
        app.active_dialog = Some(DialogState::ConfirmDelete {
            target: app.selected_workspace,
        });
        app.mode = AppMode::ConfirmDelete;
    }
    None
}







pub(crate) fn open_manage_agents(app: &mut App) -> Option<Action> {
    if !app
        .current_workspace()
        .is_some_and(|ws| ws.info.workspace_type == piki_core::WorkspaceType::Simple)
    {
        return None;
    }
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
    None
}

pub(crate) fn open_manage_providers(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::ManageProviders { selected: 0 });
    app.mode = AppMode::ManageProviders;
    None
}

pub(crate) fn open_chat_panel(app: &mut App) -> Option<Action> {
    app.mode = AppMode::ChatPanel;
    if app.chat_panel.models.is_empty() {
        return Some(Action::ChatLoadModels);
    }
    None
}


// ── Workspaces & tabs ──

pub(crate) fn cycle_next_tab(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && !ws.tabs.is_empty()
    {
        ws.active_tab = (ws.active_tab + 1) % ws.tabs.len();
    }
    None
}

pub(crate) fn cycle_prev_tab(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && !ws.tabs.is_empty()
    {
        ws.active_tab = (ws.active_tab + ws.tabs.len() - 1) % ws.tabs.len();
    }
    None
}

pub(crate) fn open_new_tab(app: &mut App) -> Option<Action> {
    if app.current_workspace().is_some() {
        app.active_dialog = Some(DialogState::NewTab {
            menu: crate::dialog_state::NewTabMenu::Main,
        });
        app.mode = AppMode::NewTab;
    }
    None
}

pub(crate) fn request_close_tab(app: &mut App) -> Option<Action> {
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
    None
}

// ── Terminal scrollback ──

pub(crate) fn term_scroll_up(app: &mut App, lines: usize) -> Option<Action> {
    if app.active_pane == ActivePane::MainPanel
        && app.mode == AppMode::Normal
        && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
        && let Some(ref parser) = tab.pty_parser
    {
        let max = scrollback_max(parser);
        tab.term_scroll = (tab.term_scroll + lines).min(max);
    }
    None
}

pub(crate) fn term_scroll_down(app: &mut App, lines: usize) -> Option<Action> {
    if app.active_pane == ActivePane::MainPanel
        && app.mode == AppMode::Normal
        && let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        tab.term_scroll = tab.term_scroll.saturating_sub(lines);
    }
    None
}

pub(crate) fn term_page_up(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
        && let Some(ref parser) = tab.pty_parser
    {
        let screen_height = app.pty_rows as usize;
        let max = scrollback_max(parser);
        tab.term_scroll = (tab.term_scroll + screen_height).min(max);
    }
    None
}

pub(crate) fn term_page_down(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        let screen_height = app.pty_rows as usize;
        tab.term_scroll = tab.term_scroll.saturating_sub(screen_height);
    }
    None
}

pub(crate) fn term_scroll_top(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
        && let Some(ref parser) = tab.pty_parser
    {
        tab.term_scroll = scrollback_max(parser);
    }
    None
}

pub(crate) fn term_scroll_bottom(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        tab.term_scroll = 0;
    }
    None
}

/// Enter terminal scroll mode (`prefix [`). Only meaningful when the current
/// tab is a real terminal; focus moves to the main panel.
pub(crate) fn enter_term_scroll(app: &mut App) -> Option<Action> {
    let has_pty = app.mode == AppMode::Normal
        && app
            .current_workspace()
            .and_then(|ws| ws.current_tab())
            .is_some_and(|tab| tab.pty_parser.is_some());
    if has_pty {
        app.active_pane = ActivePane::MainPanel;
        app.input_state = crate::app::InputState::TermScroll;
    } else {
        app.set_toast("No scrollback here", crate::app::ToastLevel::Info);
    }
    None
}

/// Leave terminal scroll mode and snap the view back to the live bottom.
pub(crate) fn exit_term_scroll(app: &mut App) -> Option<Action> {
    app.input_state = crate::app::InputState::Normal;
    if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
        && let Some(tab) = ws.current_tab_mut()
    {
        tab.term_scroll = 0;
    }
    None
}

/// `prefix g`: open-or-focus the lazygit tab of the current workspace.
/// If a Git tab exists but its process died (e.g. the user quit lazygit),
/// close it and respawn transparently.
/// Open the in-terminal search overlay over the active tab. A prefix action
/// (default `Ctrl+G f`) rather than a direct chord, so it can't collide with a
/// terminal emulator's own `Ctrl+Shift+*` bindings (e.g. ghostty).
///
/// No-op (with a toast) unless the active tab is actually a live terminal:
/// searching only makes sense over PTY output, not over a chat, kanban board,
/// markdown viewer, or a "command not found" placeholder — all of which render
/// in the MainPanel without a `pty_parser`.
pub(crate) fn open_terminal_search(app: &mut App) -> Option<Action> {
    let has_terminal = app
        .workspaces
        .get(app.active_workspace)
        .and_then(|ws| ws.current_tab())
        .is_some_and(|tab| tab.pty_parser.is_some());
    if !has_terminal {
        app.set_toast("No terminal to search", crate::app::ToastLevel::Info);
        return None;
    }
    app.active_pane = ActivePane::MainPanel;
    app.term_search = Some(crate::app::TermSearchState {
        query: String::new(),
        cursor: 0,
        matches: Vec::new(),
        current_match: 0,
    });
    None
}

pub(crate) fn open_git_tab(app: &mut App) -> Option<Action> {
    let Some(ws) = app.workspaces.get_mut(app.active_workspace) else {
        app.set_toast("No active workspace", crate::app::ToastLevel::Info);
        return None;
    };
    if let Some(idx) = ws
        .tabs
        .iter()
        .position(|t| t.provider == piki_core::AIProvider::Git)
    {
        let alive = ws.tabs[idx]
            .pty_session
            .as_ref()
            .is_some_and(|p| p.peek_alive());
        if alive {
            ws.active_tab = idx;
            ws.tabs[idx].term_scroll = 0;
            app.active_pane = ActivePane::MainPanel;
            return None;
        }
        ws.close_tab(idx);
    }
    app.active_pane = ActivePane::MainPanel;
    Some(Action::SpawnTab(piki_core::AIProvider::Git))
}

// ── Layout ──

pub(crate) fn sidebar_shrink(app: &mut App) -> Option<Action> {
    app.sidebar_pct = app.sidebar_pct.saturating_sub(5).max(10);
    resize_all_ptys(app);
    app.save_layout_prefs();
    None
}

pub(crate) fn sidebar_grow(app: &mut App) -> Option<Action> {
    app.sidebar_pct = (app.sidebar_pct + 5).min(90);
    resize_all_ptys(app);
    app.save_layout_prefs();
    None
}

pub(crate) fn split_up(app: &mut App) -> Option<Action> {
    app.left_split_pct = (app.left_split_pct + 10).min(90);
    app.save_layout_prefs();
    None
}

pub(crate) fn split_down(app: &mut App) -> Option<Action> {
    app.left_split_pct = app.left_split_pct.saturating_sub(10).max(10);
    app.save_layout_prefs();
    None
}
