//! Global app actions, one function per action.
//!
//! These are the bodies of the former navigation-mode arms, extracted so the
//! key-matching layer (the prefix dispatcher and pane routing) stays
//! a thin table. All helpers return `Option<Action>` so callers can uniformly
//! `return` their result.

use crate::action::Action;
use crate::app::{ActivePane, App, AppMode, DialogField, InputState};
use crate::dialog_state::{DialogState, EditWorkspaceField};
use crate::helpers::{resize_all_ptys, scrollback_max};

// ── Focus movement between panes ──
//
// Layout: WorkspaceList (top-left) and Agents (bottom-left) stack in the
// sidebar; MainPanel (right) spans both. Moves off the edge of that grid have no
// destination — say so instead of doing nothing, or the user can't tell an edge
// apart from an unbound key.

fn focus(app: &mut App, edge: &str, target: Option<ActivePane>) -> Option<Action> {
    match target {
        Some(pane) => app.active_pane = pane,
        None => app.set_toast(format!("No pane {edge}"), crate::app::ToastLevel::Info),
    }
    None
}

pub(crate) fn focus_left(app: &mut App) -> Option<Action> {
    let target = match app.active_pane {
        ActivePane::MainPanel => Some(ActivePane::WorkspaceList),
        // Both sidebar panes are already flush against the left edge.
        ActivePane::WorkspaceList | ActivePane::Agents => None,
    };
    focus(app, "to the left", target)
}

pub(crate) fn focus_right(app: &mut App) -> Option<Action> {
    let target = match app.active_pane {
        ActivePane::WorkspaceList | ActivePane::Agents => Some(ActivePane::MainPanel),
        ActivePane::MainPanel => None,
    };
    focus(app, "to the right", target)
}

pub(crate) fn focus_down(app: &mut App) -> Option<Action> {
    let target = match app.active_pane {
        ActivePane::WorkspaceList | ActivePane::MainPanel => Some(ActivePane::Agents),
        ActivePane::Agents => None,
    };
    focus(app, "below", target)
}

pub(crate) fn focus_up(app: &mut App) -> Option<Action> {
    let target = match app.active_pane {
        ActivePane::Agents | ActivePane::MainPanel => Some(ActivePane::WorkspaceList),
        ActivePane::WorkspaceList => None,
    };
    focus(app, "above", target)
}

// ── Dialogs and overlays ──

pub(crate) fn open_confirm_quit(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::ConfirmQuit);
    app.mode = AppMode::ConfirmQuit;
    None
}

pub(crate) fn open_help(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::Help {
        scroll: 0,
        filter: String::new(),
    });
    app.mode = AppMode::Help;
    None
}

pub(crate) fn open_about(app: &mut App) -> Option<Action> {
    app.active_dialog = Some(DialogState::About);
    app.mode = AppMode::About;
    None
}

pub(crate) fn open_dashboard(app: &mut App) -> Option<Action> {
    let indices = app.dashboard_indices();
    // Always open: workspaces part may be empty (shows nothing), external
    // agents section below always renders (either rows or “— No external … —”).
    let selected = indices
        .iter()
        .position(|&idx| idx == app.active_workspace)
        .unwrap_or(0);
    app.active_dialog = Some(DialogState::Dashboard {
        selected,
        scroll_offset: 0,
    });
    app.mode = AppMode::Dashboard;
    None
}

pub(crate) fn open_sessions(app: &mut App) -> Option<Action> {
    // Daemon pid is read here, at open time — renders must stay pure.
    let daemon_pid = std::fs::read_to_string(app.paths.session_pid_file())
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    app.active_dialog = Some(DialogState::Sessions {
        loading: app.session_daemon.is_some(),
        error: None,
        sessions: Vec::new(),
        selected: 0,
        scroll_offset: 0,
        daemon_pid,
    });
    app.mode = AppMode::Sessions;
    Some(Action::LoadSessions)
}

pub(crate) fn open_projects(app: &mut App) -> Option<Action> {
    // Loaded from storage here, at open time — renders must stay pure. The
    // list also reloads through this function after every save/delete and
    // when the edit sub-dialog backs out.
    let projects = app
        .storage
        .projects
        .as_ref()
        .map(|s| s.list_projects())
        .unwrap_or_default();
    app.active_dialog = Some(DialogState::Projects {
        projects,
        selected: 0,
        expanded: std::collections::HashSet::new(),
        scroll_offset: 0,
    });
    app.mode = AppMode::Projects;
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
        app.active_dialog = Some(DialogState::EditWorkspace {
            target: app.selected_workspace,
            kanban_cursor: kanban.chars().count(),
            kanban,
            prompt_cursor: prompt.chars().count(),
            prompt,
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
                app.active_dialog = Some(crate::dialog_state::DialogState::CreateWorktree {
                    parent_idx: app.selected_workspace,
                    mode: crate::dialog_state::CreateWorktreeMode::ChooseSource,
                    name: String::new(),
                    name_cursor: 0,
                    prompt_cursor: prompt.chars().count(),
                    prompt,
                    kanban_cursor: kanban.chars().count(),
                    kanban,
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

/// Severity at or above which an agent "needs you": waiting for permission,
/// or idle/done carrying news the user hasn't looked at. Mirrors the
/// desktop's `ATTENTION_SEVERITY` over `piki_core::cli_agent::status_severity`
/// (4 / 3) — and the states `ui::actionable_status_view` already surfaces as
/// a glyph in the sidebar, subtab bar and status bar.
const ATTENTION_SEVERITY: u8 = 3;

/// Order the agents needing attention: worst first, stable within a severity
/// so the Agents-pane order is the jump order. Pure — the App-reading half is
/// `attention_targets`.
fn order_attention_targets(mut rows: Vec<((usize, usize), u8)>) -> Vec<(usize, usize)> {
    // Stable, so same-severity rows keep the order `agent_rows()` produced.
    rows.sort_by_key(|(_, severity)| std::cmp::Reverse(*severity));
    rows.into_iter().map(|(key, _)| key).collect()
}

/// Where the jump lands: the worst agent needing attention — or, when the
/// user is already standing on one of them, the next one down the list
/// (cyclic), so pressing the chord again walks through everything that needs
/// them. `None` when nothing does. Pure.
fn pick_attention_target(
    targets: &[(usize, usize)],
    current: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    if targets.is_empty() {
        return None;
    }
    let here = current.and_then(|c| targets.iter().position(|t| *t == c));
    Some(match here {
        Some(i) => targets[(i + 1) % targets.len()],
        None => targets[0],
    })
}

/// The (workspace, tab) pairs whose agent needs the user, worst first.
fn attention_targets(app: &App) -> Vec<(usize, usize)> {
    let rows = app
        .agent_rows()
        .into_iter()
        .filter_map(|(ws_idx, tab_idx)| {
            let tab = app.workspaces.get(ws_idx)?.tabs.get(tab_idx)?;
            let (status, attention, _) = tab.cli_agent_snapshot()?;
            let severity = piki_core::cli_agent::status_severity(status, attention);
            (severity >= ATTENTION_SEVERITY).then_some(((ws_idx, tab_idx), severity))
        })
        .collect();
    order_attention_targets(rows)
}

/// Jump to the agent that needs the user (permission first, then unseen
/// news), cycling through them on repeated presses — the desktop's `Alt+A`.
/// Looking at the tab is what clears its marker (the event loop acknowledges
/// the visible tab), so a walk empties the list as it goes.
pub(crate) fn jump_to_attention(app: &mut App) -> Option<Action> {
    let targets = attention_targets(app);
    let current = app
        .current_workspace()
        .map(|ws| (app.active_workspace, ws.active_tab));
    match pick_attention_target(&targets, current) {
        Some(target) => super::interaction::jump_to_agent(app, target),
        None => app.set_toast("No agent needs you", crate::app::ToastLevel::Info),
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

/// Text currently selected in the active tab's terminal, if any. The
/// selection is a plain cell rectangle over the rendered screen, so it is
/// read back through the parser at the tab's scroll position — the same way
/// the mouse-release copy does.
fn terminal_selection_text(app: &App) -> Option<String> {
    let sel = app.selection.as_ref()?;
    let (sr, sc, er, ec) = sel.normalized();
    if sr == er && sc == ec {
        return None;
    }
    let tab = app.current_workspace()?.current_tab()?;
    let parser = tab.pty_parser.as_ref()?;
    let mut guard = parser.lock();
    guard.screen_mut().set_scrollback(tab.term_scroll);
    let text = guard.screen().contents_between(sr, sc, er, ec + 1);
    guard.screen_mut().set_scrollback(0);
    (!text.trim().is_empty()).then_some(text)
}

/// Put the terminal selection into the chat composer as a fenced block and
/// open the panel — the desktop's `Ctrl+Shift+I` with a terminal selection
/// (select, then the chord: two keys, composer ready).
pub(crate) fn add_chat_context(app: &mut App) -> Option<Action> {
    let Some(text) = terminal_selection_text(app) else {
        app.set_toast(
            "Select text in the terminal first (drag with the mouse)",
            crate::app::ToastLevel::Info,
        );
        return None;
    };
    let label = app
        .current_workspace()
        .and_then(|ws| ws.current_tab())
        .map(|tab| tab.display_label().to_string())
        .unwrap_or_else(|| "terminal".to_string());
    let block = crate::chat_context::fence_block(
        &crate::chat_context::ContextKind::Terminal,
        &label,
        &text,
    );
    push_chat_context(app, block, "Selection added to chat")
}

/// Append `block` to the chat composer, open the panel and say so. Shared by
/// every injection entry point.
pub(crate) fn push_chat_context(app: &mut App, block: String, toast: &str) -> Option<Action> {
    if block.is_empty() {
        return None;
    }
    app.chat_panel.input = crate::chat_context::append_to_draft(&app.chat_panel.input, &block);
    app.chat_panel.input_cursor = app.chat_panel.input.chars().count();
    let action = open_chat_panel(app);
    app.set_toast(toast.to_string(), crate::app::ToastLevel::Success);
    action
}

/// Toggle the global scratch-terminal overlay. Hiding it (from
/// `AppMode::ScratchTerminal`) just flips the flag and drops back to Normal —
/// the shell keeps running. Showing it needs the async spawn on first use,
/// so that path returns `Action::ShowScratchTerminal`.
pub(crate) fn toggle_scratch_terminal(app: &mut App) -> Option<Action> {
    if app.mode == AppMode::ScratchTerminal {
        app.scratch.visible = false;
        app.scratch.prefix_pending = false;
        app.scratch.selection = None;
        app.mode = AppMode::Normal;
        return None;
    }
    Some(Action::ShowScratchTerminal)
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
            app.set_toast(
                "Cannot close the initial shell tab",
                crate::app::ToastLevel::Error,
            );
        }
    }
    None
}

/// Open the destination picker for moving the active tab to another
/// workspace — the desktop's *Move to workspace…*.
///
/// Only a live terminal or agent tab can move: the process is re-parented
/// untouched (it keeps the cwd it started in), whereas Kanban / API / Code
/// Review / markdown tabs render from the workspace's own state and would be
/// meaningless elsewhere, and the lazygit tab is pinned to its repo. The
/// initial shell is refused for the same reason `close_tab` refuses it —
/// moving it away would leave its workspace with no tabs at all.
pub(crate) fn open_move_tab(app: &mut App) -> Option<Action> {
    let Some(ws) = app.current_workspace() else {
        app.set_toast("No active workspace", crate::app::ToastLevel::Info);
        return None;
    };
    let tab_idx = ws.active_tab;
    let tab = ws.tabs.get(tab_idx)?;
    let movable = tab.pty_session.is_some()
        && tab.markdown_content.is_none()
        && matches!(
            tab.provider,
            piki_core::AIProvider::Shell | piki_core::AIProvider::Custom(_)
        );
    if !movable {
        app.set_toast(
            "Only shell and agent tabs can be moved",
            crate::app::ToastLevel::Info,
        );
        return None;
    }
    if !tab.closable {
        app.set_toast(
            "Cannot move the initial shell tab",
            crate::app::ToastLevel::Info,
        );
        return None;
    }
    let targets: Vec<usize> = (0..app.workspaces.len())
        .filter(|i| *i != app.active_workspace)
        .collect();
    if targets.is_empty() {
        app.set_toast(
            "No other workspace to move this tab to",
            crate::app::ToastLevel::Info,
        );
        return None;
    }
    app.active_dialog = Some(DialogState::MoveTab {
        tab: tab_idx,
        targets,
        selected: 0,
    });
    app.mode = AppMode::MoveTab;
    None
}

/// Re-spawn the active tab's process after it exited — the desktop's
/// pane-header *Restart* button. Only an exited PTY tab can be restarted: a
/// live one has nothing to fix, and a Kanban / API / markdown tab has no
/// process at all.
pub(crate) fn request_restart_tab(app: &mut App) -> Option<Action> {
    let Some(ws) = app.current_workspace() else {
        app.set_toast("No active workspace", crate::app::ToastLevel::Info);
        return None;
    };
    let target = ws.active_tab;
    let pty = ws.tabs.get(target).and_then(|t| t.pty_session.as_ref());
    match pty {
        None => {
            app.set_toast(
                "This tab has no process to restart",
                crate::app::ToastLevel::Info,
            );
            None
        }
        Some(pty) if pty.peek_alive() => {
            app.set_toast(
                "This tab is still running — close it first",
                crate::app::ToastLevel::Info,
            );
            None
        }
        Some(_) => Some(Action::RestartTab(target)),
    }
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

/// Scrollback depth of the parser a clear rebuilds, mirroring what
/// `piki_multiplex::pty::session` gave the original one (1000 rows in
/// process, 5000 for a daemon-backed tab). `vt100::Parser` takes the capacity
/// at construction and never hands it back, so a clear has to restate it —
/// keep these two in step with the multiplexer if they ever change there.
const LOCAL_SCROLLBACK: usize = 1000;
const REMOTE_SCROLLBACK: usize = 5000;

/// Clear the active terminal — screen *and* scrollback — the desktop's
/// `Ctrl+Shift+K`. The process is untouched: this only drops what the
/// emulator is holding, so a full-screen program (an agent's TUI, vim)
/// repaints on its next output.
pub(crate) fn clear_terminal(app: &mut App) -> Option<Action> {
    let Some(ws) = app.workspaces.get_mut(app.active_workspace) else {
        app.set_toast("No active workspace", crate::app::ToastLevel::Info);
        return None;
    };
    let tab = ws.tabs.get_mut(ws.active_tab)?;
    let Some(parser) = tab.pty_parser.as_ref() else {
        app.set_toast(
            "This tab has no terminal to clear",
            crate::app::ToastLevel::Info,
        );
        return None;
    };
    let scrollback = if tab.pty_session.as_ref().is_some_and(|p| p.is_remote()) {
        REMOTE_SCROLLBACK
    } else {
        LOCAL_SCROLLBACK
    };
    {
        let mut guard = parser.lock();
        let (rows, cols) = guard.screen().size();
        // Same move the session layer makes when it replays a restore: a
        // fresh parser of the current size is the only way to drop the
        // scrollback (`CSI 3 J` is not implemented).
        *guard = vt100::Parser::new(rows, cols, scrollback);
    }
    tab.term_scroll = 0;
    // A selection addresses cells that no longer hold anything.
    app.selection = None;
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
    // Closed dead-Git-tab's daemon session, to remove after the borrow ends.
    let mut dead_session_id: Option<String> = None;
    {
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
            dead_session_id = ws.tabs[idx].session_id.clone();
            ws.close_tab(idx);
        }
    }
    if let Some(sid) = dead_session_id {
        crate::helpers::remove_session(app, &sid);
    }
    app.active_pane = ActivePane::MainPanel;
    Some(Action::SpawnTab(piki_core::AIProvider::Git))
}

// ── Layout ──

pub(crate) fn open_rename_tab(app: &mut App) -> Option<Action> {
    let Some(ws) = app.workspaces.get(app.active_workspace) else {
        app.set_toast("No active workspace", crate::app::ToastLevel::Info);
        return None;
    };
    let Some(tab) = ws.current_tab() else {
        app.set_toast("No active tab", crate::app::ToastLevel::Info);
        return None;
    };
    let initial = tab.display_label().to_string();
    let cursor = initial.chars().count();
    app.active_dialog = Some(DialogState::RenameTab {
        input: initial,
        cursor,
    });
    app.mode = AppMode::RenameTab;
    None
}

pub(crate) fn sidebar_shrink(app: &mut App) -> Option<Action> {
    app.sidebar_pct = app.sidebar_pct.saturating_sub(5).max(10);
    resize_all_ptys(app);
    app.save_layout_prefs();
    app.input_state = InputState::Resize;
    None
}

pub(crate) fn sidebar_grow(app: &mut App) -> Option<Action> {
    app.sidebar_pct = (app.sidebar_pct + 5).min(90);
    resize_all_ptys(app);
    app.save_layout_prefs();
    app.input_state = InputState::Resize;
    None
}

pub(crate) fn split_up(app: &mut App) -> Option<Action> {
    app.left_split_pct = (app.left_split_pct + 10).min(90);
    app.save_layout_prefs();
    app.input_state = InputState::Resize;
    None
}

pub(crate) fn split_down(app: &mut App) -> Option<Action> {
    app.left_split_pct = app.left_split_pct.saturating_sub(10).max(10);
    app.save_layout_prefs();
    app.input_state = InputState::Resize;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_app;

    /// A focus move: the pane it starts from, the mover, and what it should say
    /// when there is nowhere to go.
    type EdgeCase = (ActivePane, fn(&mut App) -> Option<Action>, &'static str);

    /// The four focus moves used to be silent no-ops at the edges of the pane
    /// grid, which reads exactly like an unbound key.
    #[test]
    fn focus_moves_off_the_edge_say_so() {
        let cases: [EdgeCase; 4] = [
            (ActivePane::Agents, focus_left, "No pane to the left"),
            (ActivePane::MainPanel, focus_right, "No pane to the right"),
            (ActivePane::Agents, focus_down, "No pane below"),
            (ActivePane::WorkspaceList, focus_up, "No pane above"),
        ];

        for (from, mv, expected) in cases {
            let mut app = test_app();
            app.active_pane = from;

            mv(&mut app);

            assert_eq!(app.active_pane, from, "focus should not have moved");
            assert_eq!(app.status_message.as_deref(), Some(expected));
        }
    }

    #[test]
    fn focus_moves_with_a_destination_are_silent() {
        let mut app = test_app();
        app.active_pane = ActivePane::MainPanel;

        focus_left(&mut app);

        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
        assert!(
            app.status_message.is_none(),
            "a successful move must not toast"
        );
    }

    /// Agents sits below the workspace list, so `focus_left` from it has no
    /// destination — but `focus_up` does.
    #[test]
    fn agents_pane_reaches_the_workspace_list_upward() {
        let mut app = test_app();
        app.active_pane = ActivePane::Agents;

        focus_up(&mut app);

        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    // ── Add context to the chat ──

    #[test]
    fn chat_context_without_a_selection_says_what_to_do() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        crate::test_support::add_terminal_tab(&mut app, 0);

        let action = add_chat_context(&mut app);

        assert!(action.is_none());
        assert!(app.chat_panel.input.is_empty());
        assert_eq!(
            app.status_message.as_deref(),
            Some("Select text in the terminal first (drag with the mouse)")
        );
    }

    #[test]
    fn chat_context_injects_the_selection_and_opens_the_panel() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        crate::test_support::add_terminal_tab(&mut app, 0);
        {
            let parser = app.workspaces[0].tabs[0].pty_parser.clone().unwrap();
            parser.lock().process(b"hello world\r\n");
        }
        let owner = app.selection_owner_key().expect("a tab to own it");
        let mut sel = crate::app::Selection::new(0, 0, owner);
        sel.end_col = 10;
        sel.active = false;
        app.selection = Some(sel);

        add_chat_context(&mut app);

        assert!(
            app.chat_panel.input.contains("hello world"),
            "composer: {:?}",
            app.chat_panel.input
        );
        assert!(app.chat_panel.input.starts_with("Terminal selection (tab "));
        assert_eq!(app.mode, AppMode::ChatPanel);
        assert_eq!(
            app.chat_panel.input_cursor,
            app.chat_panel.input.chars().count()
        );
    }

    /// A second block lands under the first, not glued to it.
    #[test]
    fn chat_context_appends_below_what_is_already_there() {
        let mut app = test_app();
        app.chat_panel.input = "look:".to_string();

        push_chat_context(&mut app, "File: a.rs\n```rs\nx\n```\n".to_string(), "ok");

        assert_eq!(app.chat_panel.input, "look:\n\nFile: a.rs\n```rs\nx\n```\n");
    }

    // ── Clear the terminal ──

    #[test]
    fn clear_terminal_empties_the_screen_and_resets_the_scroll() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        crate::test_support::add_terminal_tab(&mut app, 0);
        {
            let parser = app.workspaces[0].tabs[0].pty_parser.clone().unwrap();
            let mut guard = parser.lock();
            for i in 0..50 {
                guard.process(format!("line {i}\r\n").as_bytes());
            }
            assert!(!guard.screen().contents().trim().is_empty());
        }
        app.workspaces[0].tabs[0].term_scroll = 5;

        clear_terminal(&mut app);

        let parser = app.workspaces[0].tabs[0].pty_parser.clone().unwrap();
        assert!(
            parser.lock().screen().contents().trim().is_empty(),
            "the screen should be empty after a clear"
        );
        assert_eq!(app.workspaces[0].tabs[0].term_scroll, 0);
    }

    #[test]
    fn clear_terminal_on_a_tab_without_one_toasts() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        app.workspaces[0].add_tab(piki_core::AIProvider::Kanban, true, None);

        let action = clear_terminal(&mut app);

        assert!(action.is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("This tab has no terminal to clear")
        );
    }

    // ── Move a tab to another workspace ──

    #[test]
    fn move_tab_without_a_workspace_toasts() {
        let mut app = test_app();

        let action = open_move_tab(&mut app);

        assert!(action.is_none());
        assert_eq!(app.status_message.as_deref(), Some("No active workspace"));
        assert_eq!(app.mode, AppMode::Normal);
    }

    /// A Kanban / API / Code Review tab renders from the workspace's own
    /// state, so there is nothing to re-parent.
    #[test]
    fn move_tab_refuses_a_tab_with_no_process() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        crate::test_support::add_test_workspace(&mut app);
        app.workspaces[0].add_tab(piki_core::AIProvider::Kanban, true, None);

        let action = open_move_tab(&mut app);

        assert!(action.is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("Only shell and agent tabs can be moved")
        );
        assert_eq!(app.mode, AppMode::Normal);
    }

    // ── Restart an exited tab ──

    /// Kanban / API / markdown tabs have no process behind them, so the chord
    /// must say so instead of silently doing nothing.
    #[test]
    fn restart_without_a_process_toasts_and_produces_no_action() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        crate::test_support::add_terminal_tab(&mut app, 0); // no PTY in tests

        let action = request_restart_tab(&mut app);

        assert!(action.is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("This tab has no process to restart")
        );
    }

    #[test]
    fn restart_without_a_workspace_toasts() {
        let mut app = test_app();

        let action = request_restart_tab(&mut app);

        assert!(action.is_none());
        assert_eq!(app.status_message.as_deref(), Some("No active workspace"));
    }

    // ── Jump to the agent needing attention ──

    #[test]
    fn attention_order_is_worst_first_then_pane_order() {
        // (workspace, tab) keys with their severity, in Agents-pane order.
        let rows = vec![
            ((0, 1), 3), // unseen news
            ((1, 0), 4), // waiting for permission
            ((0, 2), 3), // unseen news, later in the pane
        ];

        assert_eq!(
            order_attention_targets(rows),
            vec![(1, 0), (0, 1), (0, 2)],
            "permission first, then pane order within the same severity"
        );
    }

    #[test]
    fn jump_starts_at_the_worst_agent_and_then_cycles() {
        let targets = [(1, 0), (0, 1), (0, 2)];

        // Standing somewhere else: land on the worst one.
        assert_eq!(pick_attention_target(&targets, None), Some((1, 0)));
        assert_eq!(pick_attention_target(&targets, Some((2, 5))), Some((1, 0)));

        // Standing on one of them: walk to the next, wrapping at the end.
        assert_eq!(pick_attention_target(&targets, Some((1, 0))), Some((0, 1)));
        assert_eq!(pick_attention_target(&targets, Some((0, 2))), Some((1, 0)));
    }

    #[test]
    fn nothing_needing_attention_has_no_target() {
        assert_eq!(pick_attention_target(&[], None), None);
        assert_eq!(pick_attention_target(&[], Some((0, 0))), None);
    }

    /// Without an agent reporting, the chord must say so rather than move the
    /// user somewhere at random.
    #[test]
    fn jump_with_no_agents_toasts_and_stays_put() {
        let mut app = test_app();
        crate::test_support::add_test_workspace(&mut app);
        let before = app.active_workspace;

        jump_to_attention(&mut app);

        assert_eq!(app.active_workspace, before);
        assert_eq!(app.status_message.as_deref(), Some("No agent needs you"));
    }
}
