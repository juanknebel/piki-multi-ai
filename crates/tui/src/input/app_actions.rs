//! Global app actions, one function per action.
//!
//! These are the bodies of the former navigation-mode arms, extracted so the
//! key-matching layer (navigation mode today, the prefix dispatcher next) stays
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
        ActivePane::WorkspaceList | ActivePane::GitStatus
    ) {
        app.active_pane = ActivePane::MainPanel;
    }
    None
}

pub(crate) fn focus_down(app: &mut App) -> Option<Action> {
    match app.active_pane {
        ActivePane::WorkspaceList | ActivePane::MainPanel => {
            app.active_pane = ActivePane::GitStatus;
        }
        _ => {}
    }
    None
}

pub(crate) fn focus_up(app: &mut App) -> Option<Action> {
    match app.active_pane {
        ActivePane::GitStatus | ActivePane::MainPanel => {
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
                    name: String::new(),
                    name_cursor: 0,
                    prompt_cursor: prompt.chars().count(),
                    prompt,
                    kanban_cursor: kanban.chars().count(),
                    kanban,
                    group_cursor: group.chars().count(),
                    group,
                    active_field: crate::dialog_state::CreateWorktreeField::Name,
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

pub(crate) fn open_commit_dialog(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.current_workspace()
        && ws.info.workspace_type != piki_core::WorkspaceType::Project
    {
        app.active_dialog = Some(DialogState::CommitMessage {
            buffer: String::new(),
        });
        app.mode = AppMode::CommitMessage;
    }
    None
}

pub(crate) fn open_confirm_merge(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.current_workspace()
        && ws.info.workspace_type != piki_core::WorkspaceType::Project
    {
        app.active_dialog = Some(DialogState::ConfirmMerge);
        app.mode = AppMode::ConfirmMerge;
    }
    None
}

pub(crate) fn git_push(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.current_workspace()
        && ws.info.workspace_type != piki_core::WorkspaceType::Project
    {
        return Some(Action::GitPush);
    }
    None
}

pub(crate) fn git_stash_list(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.current_workspace()
        && ws.info.workspace_type != piki_core::WorkspaceType::Project
    {
        return Some(Action::GitStashList);
    }
    None
}

pub(crate) fn git_log(app: &mut App) -> Option<Action> {
    if app.current_workspace().is_some() {
        return Some(Action::LoadGitLog);
    }
    None
}

pub(crate) fn detect_conflicts(app: &mut App) -> Option<Action> {
    if let Some(ws) = app.current_workspace()
        && ws.info.workspace_type != piki_core::WorkspaceType::Project
    {
        return Some(Action::DetectConflicts);
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

pub(crate) fn undo(_app: &mut App) -> Option<Action> {
    Some(Action::Undo)
}

// ── Workspaces & tabs ──

/// Pane-sensitive "next" cycling (workspace / tab / file, depending on focus).
pub(crate) fn cycle_next_by_pane(app: &mut App) -> Option<Action> {
    match app.active_pane {
        ActivePane::WorkspaceList => app.next_workspace(),
        ActivePane::MainPanel => return cycle_next_tab(app),
        ActivePane::GitStatus => app.next_file(),
    }
    None
}

/// Pane-sensitive "prev" cycling (workspace / tab / file, depending on focus).
pub(crate) fn cycle_prev_by_pane(app: &mut App) -> Option<Action> {
    match app.active_pane {
        ActivePane::WorkspaceList => app.prev_workspace(),
        ActivePane::MainPanel => return cycle_prev_tab(app),
        ActivePane::GitStatus => app.prev_file(),
    }
    None
}

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

// ── Quick git actions (GitStatus pane) ──

pub(crate) fn stage_quick(app: &mut App) -> Option<Action> {
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
    None
}

pub(crate) fn unstage_quick(app: &mut App) -> Option<Action> {
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
    None
}
