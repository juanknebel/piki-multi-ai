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

    // Right panel: tabs + sub-tabs + content + status bar
    let [_tabs, _subtabs, main_area, _status] = Layout::vertical([
        Constraint::Length(3),
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

/// Render the main application layout
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Compute footer keys once per frame, use for both height and rendering
    let keys = super::statusbar::footer_keys(app);
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

    // Right panel: tabs + sub-tabs + content + status bar (store for mouse hit-testing)
    let [tabs_area, subtabs_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(right_area);

    app.tabs_area = tabs_area;
    app.subtabs_area = subtabs_area;
    app.main_content_area = main_area;

    // --- Render panels ---

    // Top header: system info
    super::statusbar::render_sysinfo_bar(frame, header_area, app);

    // Left top: workspace list
    super::sidebar::render_workspace_list(frame, ws_area, app);

    // Left bottom: changed files
    super::sidebar::render_file_list(frame, files_area, app);

    // Right top: workspace tabs
    super::panels::render_tab_bar(frame, tabs_area, app);

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

    // Overlays
    if app.mode == AppMode::Diff {
        super::dialogs::render_diff_overlay(frame, area, app);
    }
    if app.mode == AppMode::NewWorkspace {
        super::dialogs::render_new_workspace_dialog(frame, area, app);
    }
    if app.mode == AppMode::EditWorkspace {
        super::dialogs::render_edit_workspace_dialog(frame, area, app);
    }
    if app.mode == AppMode::Help {
        super::dialogs::render_help_overlay(frame, area, app);
    }
    if app.mode == AppMode::ConfirmDelete {
        super::dialogs::render_confirm_delete_dialog(frame, area, app);
    }
    if app.mode == AppMode::CommitMessage {
        super::dialogs::render_commit_dialog(frame, area, app);
    }
    if app.mode == AppMode::ConfirmMerge {
        super::dialogs::render_confirm_merge_dialog(frame, area, app);
    }
    if app.mode == AppMode::FuzzySearch {
        super::fuzzy::render(frame, area, app);
    }
    if app.mode == AppMode::NewTab {
        super::dialogs::render_new_tab_dialog(frame, area);
    }
    if app.mode == AppMode::About {
        super::dialogs::render_about_overlay(frame, area, app);
    }
    if app.mode == AppMode::WorkspaceInfo {
        super::dialogs::render_workspace_info_overlay(frame, area, app);
    }
    if app.mode == AppMode::ConfirmCloseTab {
        super::dialogs::render_confirm_close_tab_dialog(frame, area, app);
    }
    if app.mode == AppMode::ConfirmQuit {
        super::dialogs::render_confirm_quit_dialog(frame, area, app);
    }
}
