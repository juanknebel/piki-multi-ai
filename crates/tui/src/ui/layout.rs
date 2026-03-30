use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;

use crate::app::{ActivePane, App, AppMode};

/// Compute the inner terminal area (minus borders) for a given total terminal size.
/// Replicates layout math to find the main content area dimensions.
pub fn compute_terminal_area_with(total: Rect, sidebar_pct: u16) -> Rect {
    // Main vertical split: header + content + footer (use max footer height for conservative estimate)
    let [_header, content_area, _footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(total);

    // Horizontal split: left sidebar + right main panel
    let [_left, right_area] = Layout::horizontal([
        Constraint::Percentage(sidebar_pct),
        Constraint::Percentage(100 - sidebar_pct),
    ])
    .areas(content_area);

    // Right panel: sub-tabs + content + status bar
    let [_subtabs, main_area, _status] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(right_area);

    // Subtract borders (2 for top/bottom, 2 for left/right)
    Rect::new(
        main_area.x + 1,
        main_area.y + 1,
        main_area.width.saturating_sub(2),
        main_area.height.saturating_sub(2),
    )
}

/// Border style for a pane: green if interacting, yellow if selected, white otherwise
pub(super) fn pane_border_style(app: &App, pane: ActivePane) -> Style {
    if app.active_pane == pane {
        if app.interacting {
            Style::default().fg(app.theme.border.active_interact)
        } else {
            Style::default().fg(app.theme.border.active_navigate)
        }
    } else {
        Style::default().fg(app.theme.border.inactive)
    }
}

/// Calculate how many lines the footer needs based on content width.
fn compute_footer_height_from_keys(keys: &[(String, &str)], total_width: u16) -> u16 {
    let total: usize = keys
        .iter()
        .map(|(key, desc)| key.len() + desc.len() + 5)
        .sum();
    if total as u16 <= total_width { 1 } else { 2 }
}

/// Check if the active tab is a CodeReview tab with loaded state
fn is_code_review_active(app: &App) -> bool {
    app.current_workspace()
        .and_then(|ws| ws.current_tab())
        .is_some_and(|tab| tab.provider == piki_core::AIProvider::CodeReview)
        && app
            .current_workspace()
            .is_some_and(|ws| ws.code_review.is_some())
}

/// Render the main application layout
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Full-screen code review layout bypass
    if is_code_review_active(app) {
        super::code_review::render_fullscreen(frame, area, app);
        if app.mode == AppMode::SubmitReview {
            super::code_review::render_submit_overlay(frame, area, app);
        }
        return;
    }

    // Compute footer keys — use cache when mode/interacting/pane haven't changed
    let has_markdown = app
        .current_workspace()
        .and_then(|ws| ws.current_tab())
        .is_some_and(|tab| tab.markdown_content.is_some());
    let api_footer_state: u8 = match app
        .current_workspace()
        .and_then(|ws| ws.current_tab())
        .and_then(|tab| tab.api_state.as_ref())
    {
        Some(api) if api.search.is_some() => 2,
        Some(_) => 1,
        None => 0,
    };
    let new_tab_menu: u8 = match app.active_dialog {
        Some(crate::dialog_state::DialogState::NewTab { menu }) => match menu {
            crate::dialog_state::NewTabMenu::Main => 1,
            crate::dialog_state::NewTabMenu::Agents => 2,
            crate::dialog_state::NewTabMenu::Tools => 3,
        },
        _ => 0,
    };
    let sel_count = app.selection_count();
    let cache_key = (
        app.mode.clone(),
        app.interacting,
        app.active_pane,
        has_markdown,
        api_footer_state,
        new_tab_menu,
        sel_count,
    );
    let keys = if let Some((ref m, i, p, md, api, ntm, sc, ref cached)) = app.footer_cache {
        if *m == cache_key.0
            && i == cache_key.1
            && p == cache_key.2
            && md == cache_key.3
            && api == cache_key.4
            && ntm == cache_key.5
            && sc == cache_key.6
        {
            cached.clone()
        } else {
            let k = super::statusbar::footer_keys(app);
            app.footer_cache = Some((
                cache_key.0,
                cache_key.1,
                cache_key.2,
                cache_key.3,
                cache_key.4,
                cache_key.5,
                cache_key.6,
                k.clone(),
            ));
            k
        }
    } else {
        let k = super::statusbar::footer_keys(app);
        app.footer_cache = Some((
            cache_key.0,
            cache_key.1,
            cache_key.2,
            cache_key.3,
            cache_key.4,
            cache_key.5,
            cache_key.6,
            k.clone(),
        ));
        k
    };
    let footer_height = compute_footer_height_from_keys(&keys, area.width);

    // Main vertical split: header + content + footer
    let [header_area, content_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    // Horizontal split: left sidebar + right main panel
    let [left_area, right_area] = Layout::horizontal([
        Constraint::Percentage(app.sidebar_pct),
        Constraint::Percentage(100 - app.sidebar_pct),
    ])
    .areas(content_area);

    // Left panel: workspaces (top) + files (bottom)
    let [ws_area, files_area] = Layout::vertical([
        Constraint::Percentage(app.left_split_pct),
        Constraint::Percentage(100 - app.left_split_pct),
    ])
    .areas(left_area);

    // Store layout rects for mouse hit-testing
    app.sidebar_x = left_area.x + left_area.width;
    app.left_split_y = ws_area.y + ws_area.height;
    app.left_area_rect = left_area;
    app.ws_list_area = ws_area;
    app.file_list_area = files_area;

    // Right panel: sub-tabs + content + status bar (store for mouse hit-testing)
    let [subtabs_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(right_area);

    app.tabs_area = Rect::default();
    app.subtabs_area = subtabs_area;
    app.main_content_area = main_area;

    // --- Render panels ---

    // Top header: system info
    super::statusbar::render_sysinfo_bar(frame, header_area, app);

    // Left top: workspace list
    super::sidebar::render_workspace_list(frame, ws_area, app);

    // Left bottom: changed files
    super::sidebar::render_file_list(frame, files_area, app);

    // Right: AI provider sub-tabs
    super::panels::render_subtabs(frame, subtabs_area, app);

    // Cache terminal inner area for mouse coordinate translation
    let terminal_inner = Rect::new(
        main_area.x + 1,
        main_area.y + 1,
        main_area.width.saturating_sub(2),
        main_area.height.saturating_sub(2),
    );
    app.terminal_inner_area = Some(terminal_inner);

    // Right center: main content (PTY or Diff)
    super::panels::render_main_content(frame, main_area, app);

    // Right bottom: status bar
    super::statusbar::render_status_bar(frame, status_area, app);

    // Footer: keybindings
    super::statusbar::render_footer_from_keys(frame, footer_area, &keys, &app.theme);

    // Overlays — match ensures exhaustive coverage of all AppMode variants
    match app.mode {
        AppMode::Normal => {}
        AppMode::Diff => super::dialogs::render_diff_overlay(frame, area, app),
        AppMode::NewWorkspace => super::dialogs::render_new_workspace_dialog(frame, area, app),
        AppMode::EditWorkspace => super::dialogs::render_edit_workspace_dialog(frame, area, app),
        AppMode::Help => super::dialogs::render_help_overlay(frame, area, app),
        AppMode::ConfirmDelete => super::dialogs::render_confirm_delete_dialog(frame, area, app),
        AppMode::CommitMessage => super::dialogs::render_commit_dialog(frame, area, app),
        AppMode::ConfirmMerge => super::dialogs::render_confirm_merge_dialog(frame, area, app),
        AppMode::FuzzySearch => super::fuzzy::render(frame, area, app),
        AppMode::NewTab => super::dialogs::render_new_tab_dialog(frame, area, app),
        AppMode::About => super::dialogs::render_about_overlay(frame, area, app),
        AppMode::WorkspaceInfo => super::dialogs::render_workspace_info_overlay(frame, area, app),
        AppMode::ConfirmCloseTab => {
            super::dialogs::render_confirm_close_tab_dialog(frame, area, app)
        }
        AppMode::ConfirmQuit => super::dialogs::render_confirm_quit_dialog(frame, area, app),
        AppMode::Dashboard => super::dialogs::render_dashboard_overlay(frame, area, app),
        AppMode::Logs => super::dialogs::render_logs_overlay(frame, area, app),
        AppMode::CommandPalette => super::command_palette::render(frame, area, app),
        AppMode::WorkspaceSwitcher => super::workspace_switcher::render(frame, area, app),
        AppMode::GitStash => super::dialogs::render_git_stash_overlay(frame, area, app),
        AppMode::GitLog => super::dialogs::render_git_log_overlay(frame, area, app),
        AppMode::ConflictResolution => {
            super::dialogs::render_conflict_resolution_overlay(frame, area, app)
        }
        AppMode::DispatchAgent => {
            super::dialogs::render_dispatch_agent_dialog(frame, area, app)
        }
        AppMode::ManageAgents => {
            super::dialogs::render_manage_agents_dialog(frame, area, app)
        }
        AppMode::EditAgent => {
            super::dialogs::render_edit_agent_dialog(frame, area, app)
        }
        AppMode::EditAgentRole => {
            super::dialogs::render_edit_agent_role_dialog(frame, area, app)
        }
        AppMode::InlineEdit => {}   // handled by main content render
        AppMode::SubmitReview => {} // handled by full-screen code review bypass above
    }
}
