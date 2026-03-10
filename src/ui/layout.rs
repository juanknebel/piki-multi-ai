use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};

use crate::app::{ActivePane, App, AppMode, DialogField, FileStatus};


/// Compute the inner terminal area (minus borders) for a given total terminal size.
/// Replicates layout math to find the main content area dimensions.
pub fn compute_terminal_area_with(total: Rect, sidebar_pct: u16) -> Rect {
    // Main vertical split: header + content + footer (use max footer height for conservative estimate)
    let [_header, content_area, _footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(2)]).areas(total);

    // Horizontal split: left sidebar + right main panel
    let [_left, right_area] =
        Layout::horizontal([Constraint::Percentage(sidebar_pct), Constraint::Percentage(100 - sidebar_pct)])
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
fn pane_border_style(app: &App, pane: ActivePane) -> Style {
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

/// Render the main application layout
/// Calculate how many lines the footer needs based on content width.
fn compute_footer_height(app: &App, total_width: u16) -> u16 {
    let keys = footer_keys(app);
    let total: usize = keys
        .iter()
        .map(|(key, desc)| format!(" [{}] {} ", key, desc).len())
        .sum();
    if total as u16 <= total_width { 1 } else { 2 }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Pre-calculate footer height based on content width
    let footer_height = compute_footer_height(app, area.width);

    // Main vertical split: header + content + footer
    let [header_area, content_area, footer_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(footer_height)]).areas(area);

    // Horizontal split: left sidebar + right main panel
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(app.sidebar_pct), Constraint::Percentage(100 - app.sidebar_pct)])
            .areas(content_area);

    // Left panel: workspaces (top) + files (bottom)
    let [ws_area, files_area] =
        Layout::vertical([Constraint::Percentage(app.left_split_pct), Constraint::Percentage(100 - app.left_split_pct)]).areas(left_area);

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
    render_sysinfo_bar(frame, header_area, app);

    // Left top: workspace list
    render_workspace_list(frame, ws_area, app);

    // Left bottom: changed files
    render_file_list(frame, files_area, app);

    // Right top: workspace tabs
    render_tab_bar(frame, tabs_area, app);

    // Right: AI provider sub-tabs
    render_subtabs(frame, subtabs_area, app);

    // Cache terminal inner area for mouse coordinate translation
    let terminal_inner = Rect::new(
        main_area.x + 1,
        main_area.y + 1,
        main_area.width.saturating_sub(2),
        main_area.height.saturating_sub(2),
    );
    app.terminal_inner_area = Some(terminal_inner);

    // Right center: main content (PTY or Diff)
    render_main_content(frame, main_area, app);

    // Right bottom: status bar
    render_status_bar(frame, status_area, app);

    // Footer: keybindings
    render_footer(frame, footer_area, app);

    // Overlays
    if app.mode == AppMode::Diff {
        render_diff_overlay(frame, area, app);
    }
    if app.mode == AppMode::NewWorkspace {
        render_new_workspace_dialog(frame, area, app);
    }
    if app.mode == AppMode::EditWorkspace {
        render_edit_workspace_dialog(frame, area, app);
    }
    if app.mode == AppMode::Help {
        render_help_overlay(frame, area, app);
    }
    if app.mode == AppMode::ConfirmDelete {
        render_confirm_delete_dialog(frame, area, app);
    }
    if app.mode == AppMode::CommitMessage {
        render_commit_dialog(frame, area, app);
    }
    if app.mode == AppMode::ConfirmMerge {
        render_confirm_merge_dialog(frame, area, app);
    }
    if app.mode == AppMode::FuzzySearch {
        super::fuzzy::render(frame, area, app);
    }
    if app.mode == AppMode::NewTab {
        render_new_tab_dialog(frame, area);
    }
    if app.mode == AppMode::About {
        render_about_overlay(frame, area, app);
    }
    if app.mode == AppMode::WorkspaceInfo {
        render_workspace_info_overlay(frame, area, app);
    }
    if app.mode == AppMode::ConfirmQuit {
        render_confirm_quit_dialog(frame, area, app);
    }
}

fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::WorkspaceList;
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);
    let theme = &app.theme.workspace_list;

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.workspaces.is_empty() {
        let text = Paragraph::new("  Press [n] to create")
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = app
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let is_selected = i == app.selected_workspace && is_active;
            let detail_color = if is_selected {
                theme.detail_selected
            } else {
                theme.detail_normal
            };

            let marker = if i == app.active_workspace {
                "▶"
            } else {
                " "
            };
            let status_icon = match ws.status {
                crate::app::WorkspaceStatus::Idle => "●",
                crate::app::WorkspaceStatus::Busy => "◐",
                crate::app::WorkspaceStatus::Done => "✓",
                crate::app::WorkspaceStatus::Error(_) => "✗",
            };
            let line1 = Line::from(vec![
                Span::raw(format!(" {} ", marker)),
                Span::styled(
                    ws.name.clone(),
                    if i == app.active_workspace {
                        Style::default()
                            .fg(theme.name_active)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.name_inactive)
                    },
                ),
            ]);
            let line2 = Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!(
                        "{} {} | {} files",
                        status_icon,
                        ws.status_label(),
                        ws.file_count()
                    ),
                    Style::default().fg(detail_color),
                ),
            ]);

            let mut lines = vec![line1, line2];

            // Show parent project
            let project_name = ws
                .source_repo
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| ws.source_repo.to_string_lossy().to_string());
            let max_proj = area.width.saturating_sub(6) as usize;
            let proj_display = if project_name.len() > max_proj {
                format!("{}…", &project_name[..max_proj.saturating_sub(1)])
            } else {
                project_name
            };
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!("⌂ {}", proj_display),
                    Style::default().fg(detail_color),
                ),
            ]));


            let style = if is_selected {
                Style::default().bg(theme.selected_bg)
            } else {
                Style::default()
            };

            ListItem::new(lines).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_file_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::GitStatus;
    let border_style = pane_border_style(app, ActivePane::GitStatus);
    let theme = &app.theme.file_list;

    let ahead_title = app
        .current_workspace()
        .and_then(|ws| ws.ahead_behind)
        .and_then(|(ahead, behind)| {
            if ahead > 0 && behind > 0 {
                Some(format!(" ↑{} ↓{} ", ahead, behind))
            } else if ahead > 0 {
                Some(format!(" ↑{} to push ", ahead))
            } else if behind > 0 {
                Some(format!(" ↓{} behind ", behind))
            } else {
                None
            }
        });

    let mut block = Block::default()
        .title(" STATUS ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    if let Some(ref title) = ahead_title {
        block = block.title_bottom(Line::from(Span::styled(
            title.as_str(),
            Style::default().fg(theme.modified),
        )));
    }

    let files = app
        .current_workspace()
        .map(|ws| &ws.changed_files[..])
        .unwrap_or(&[]);

    if files.is_empty() {
        let text = Paragraph::new("  No files changed")
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (label, color) = match f.status {
                FileStatus::Modified => ("M", theme.modified),
                FileStatus::Added => ("A", theme.added),
                FileStatus::Deleted => ("D", theme.deleted),
                FileStatus::Renamed => ("R", theme.renamed),
                FileStatus::Untracked => ("?", theme.untracked),
                FileStatus::Conflicted => ("C", theme.conflicted),
                FileStatus::Staged => ("S", theme.staged),
                FileStatus::StagedModified => ("SM", theme.staged_modified),
            };
            let line = Line::from(vec![
                Span::styled(format!("  {} ", label), Style::default().fg(color)),
                Span::styled(&f.path, Style::default().fg(theme.file_path)),
            ]);
            let style = if i == app.selected_file && is_active {
                Style::default().bg(theme.selected_bg)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    if app.workspaces.is_empty() {
        let block = Block::default()
            .title(" No workspaces ")
            .borders(Borders::BOTTOM);
        frame.render_widget(block, area);
        return;
    }

    let titles: Vec<Line> = app
        .workspaces
        .iter()
        .map(|ws| Line::from(format!(" {} ", ws.name)))
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(app.active_workspace)
        .highlight_style(
            Style::default()
                .fg(app.theme.tabs.active)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(app.theme.tabs.inactive))
        .divider("|");

    frame.render_widget(tabs, area);
}

fn render_subtabs(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(ws) = app.current_workspace() {
        super::subtabs::render(frame, area, ws, &app.theme);
    } else {
        let block = Block::default().borders(Borders::BOTTOM);
        frame.render_widget(block, area);
    }
}

fn render_main_content(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.mode == AppMode::InlineEdit {
        super::editor::render(frame, area, app);
        return;
    }

    let border_style = pane_border_style(app, ActivePane::MainPanel);

    let selection = app.selection.take();
    let selection_style = Style::default()
        .bg(app.theme.selection.bg)
        .fg(app.theme.selection.fg);
    if let Some(ws) = app.current_workspace() {
        if let Some(tab) = ws.current_tab() {
            // Markdown tab
            if let (Some(content), Some(label)) =
                (&tab.markdown_content, &tab.markdown_label)
            {
                let content = content.clone();
                let label = label.clone();
                let scroll = tab.markdown_scroll;
                super::markdown::render(
                    frame,
                    area,
                    &content,
                    &label,
                    scroll,
                    border_style,
                );
                return;
            }

            let provider = tab.provider;

            if provider == crate::app::AIProvider::Kanban {
                if let Some(ws) = app.workspaces.get(app.active_workspace) {
                    if let Some(kanban_app) = &ws.kanban_app {
                        let block = Block::default()
                            .borders(Borders::ALL)
                            .border_style(border_style);
                        let inner_area = block.inner(area);
                        frame.render_widget(block, area);
                        flow::ui::render(frame, kanban_app, Some(inner_area));
                    }
                }
                return;
            }

            if let Some(ref parser) = tab.pty_parser {
                super::terminal::render(
                    frame,
                    area,
                    parser,
                    border_style,
                    provider.label(),
                    tab.term_scroll,
                    selection.as_ref(),
                    selection_style,
                );
            } else {
                // Provider CLI not found — show fun ASCII art
                let block = Block::default()
                    .title(format!(" {} ", provider.label()))
                    .title_style(border_style)
                    .borders(Borders::ALL)
                    .border_style(border_style);
                let cmd = provider.command();
                let ascii_art = format!(
                    r#"
        ___________________
       /                   \
      |   Command not found |
      |   `{cmd}` is not    |
      |   installed :-(     |
       \___________________/
              \
               \    _(o o)_
                \  / \.-./ \
                  ##  | |  ##
                     _| |_
                    (_____)

    Install `{cmd}` and add it to your PATH
    then press [t] to open a new tab."#
                );
                let text = Paragraph::new(ascii_art)
                    .style(Style::default().fg(app.theme.general.muted_text))
                    .block(block);
                frame.render_widget(text, area);
            }
        } else {
            // No tabs yet
            let block = Block::default()
                .title(" Terminal ")
                .title_style(border_style)
                .borders(Borders::ALL)
                .border_style(border_style);
            let text = Paragraph::new("  Press [t] to open a new tab")
                .style(Style::default().fg(app.theme.general.muted_text))
                .block(block);
            frame.render_widget(text, area);
        }
    } else {
        let block = Block::default()
            .title(" piki-multi-ai ")
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style);
        let text = Paragraph::new(
            "  Welcome to piki-multi-ai\n\n  Press [n] to create a new workspace\n  Press [?] for help\n  Press [q] to quit",
        )
        .style(Style::default().fg(app.theme.general.welcome_text))
        .block(block);
        frame.render_widget(text, area);
    }
    app.selection = selection;
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme.status_bar;
    let content = if let Some(msg) = &app.status_message {
        Span::styled(
            format!(" {} ", msg),
            Style::default().bg(theme.error_bg).fg(theme.error_fg),
        )
    } else {
        match app.mode {
            AppMode::Diff => Span::styled(
                format!(
                    " DIFF: {} | [C-g] back | [↑↓] scroll | [n/p] file",
                    app.diff_file_path.as_deref().unwrap_or("?")
                ),
                Style::default().bg(theme.diff_bg).fg(theme.diff_fg),
            ),
            AppMode::FuzzySearch => Span::styled(
                " SEARCH | type to filter | Enter = diff | C-e = editor | Esc = close",
                Style::default().bg(theme.navigate_bg).fg(theme.mode_fg),
            ),
            AppMode::InlineEdit => Span::styled(
                format!(
                    " EDIT: {} | C-s = save | Esc = close",
                    app.editing_file
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".to_string())
                ),
                Style::default().bg(theme.interact_bg).fg(theme.mode_fg),
            ),
            _ => {
                let mode_label = if app.interacting {
                    "INTERACT"
                } else {
                    "NAVIGATE"
                };
                let mode_color = if app.interacting {
                    theme.interact_bg
                } else {
                    theme.navigate_bg
                };
                if let Some(ws) = app.current_workspace() {
                    let tab_scroll = ws.current_tab().map(|t| t.term_scroll).unwrap_or(0);
                    let scroll_info = if tab_scroll > 0 {
                        format!(" | SCROLL -{}", tab_scroll)
                    } else {
                        String::new()
                    };
                    let sync_info = match ws.ahead_behind {
                        Some((ahead, behind)) if ahead > 0 && behind > 0 => {
                            format!(" | ↑{} ↓{}", ahead, behind)
                        }
                        Some((ahead, 0)) if ahead > 0 => format!(" | ↑{} unpushed", ahead),
                        Some((0, behind)) if behind > 0 => format!(" | ↓{} behind", behind),
                        _ => String::new(),
                    };
                    let tab_label = ws.current_tab().map(|t| t.provider.label()).unwrap_or("—");
                    Span::styled(
                        format!(
                            " [{}] branch: {} | {} files{} | {}: {} | ws {}/{}{}",
                            mode_label,
                            ws.branch,
                            ws.file_count(),
                            sync_info,
                            tab_label,
                            ws.status_label(),
                            app.active_workspace + 1,
                            app.workspaces.len(),
                            scroll_info,
                        ),
                        Style::default().bg(mode_color).fg(theme.mode_fg),
                    )
                } else {
                    Span::styled(
                        format!(" [{}] No active workspace", mode_label),
                        Style::default().bg(mode_color).fg(theme.mode_fg),
                    )
                }
            }
        }
    };

    let bar = Paragraph::new(Line::from(content));
    frame.render_widget(bar, area);
}

fn render_sysinfo_bar(frame: &mut Frame, area: Rect, app: &App) {
    let text = if let Ok(info) = app.sysinfo.lock() {
        info.format()
    } else {
        String::new()
    };

    let spans: Vec<Span> = text
        .split(" | ")
        .flat_map(|part| {
            if let Some((label, value)) = part.split_once(' ') {
                vec![
                    Span::styled(
                        format!(" [{}] ", label),
                        Style::default().fg(app.theme.footer.key),
                    ),
                    Span::styled(
                        format!("{} ", value),
                        Style::default().fg(app.theme.footer.description),
                    ),
                ]
            } else {
                vec![Span::styled(
                    format!(" {} ", part),
                    Style::default().fg(app.theme.footer.description),
                )]
            }
        })
        .collect();

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

fn footer_keys(app: &App) -> Vec<(String, String)> {
    let cfg = &app.config;
    match app.mode {
        AppMode::FuzzySearch => vec![
            (format!("{}/{}", cfg.get_binding("fuzzy", "up"), cfg.get_binding("fuzzy", "down")), "select".to_string()),
            (cfg.get_binding("fuzzy", "diff"), "diff".to_string()),
            (cfg.get_binding("fuzzy", "editor"), "editor".to_string()),
            (cfg.get_binding("fuzzy", "inline_edit"), "inline edit".to_string()),
            (cfg.get_binding("fuzzy", "markdown"), "markdown".to_string()),
            (cfg.get_binding("fuzzy", "mdr"), "mdr".to_string()),
            (cfg.get_binding("fuzzy", "exit"), "close".to_string()),
        ],
        AppMode::InlineEdit => vec![
            (cfg.get_binding("editor", "save"), "save".to_string()),
            (cfg.get_binding("editor", "exit"), "close".to_string()),
        ],
        AppMode::NewWorkspace => vec![
            (cfg.get_binding("new_workspace", "switch_field"), "switch field".to_string()),
            (cfg.get_binding("new_workspace", "create"), "create".to_string()),
            (cfg.get_binding("new_workspace", "exit"), "cancel".to_string()),
        ],
        AppMode::CommitMessage => vec![
            (cfg.get_binding("commit", "commit"), "commit".to_string()),
            (cfg.get_binding("commit", "exit"), "cancel".to_string()),
        ],
        AppMode::ConfirmMerge => vec![
            (cfg.get_binding("merge", "merge"), "merge".to_string()),
            (cfg.get_binding("merge", "rebase"), "rebase".to_string()),
            (cfg.get_binding("merge", "exit"), "cancel".to_string()),
        ],
        AppMode::EditWorkspace => vec![
            (cfg.get_binding("new_workspace", "switch_field"), "switch field".to_string()),
            ("enter".to_string(), "save".to_string()),
            ("esc".to_string(), "cancel".to_string()),
        ],
        AppMode::NewTab => vec![
            ("1-5".to_string(), "select".to_string()),
            (cfg.get_binding("new_tab", "exit"), "cancel".to_string()),
        ],
        AppMode::Diff => vec![
            (format!("{}/{}", cfg.get_binding("diff", "up"), cfg.get_binding("diff", "down")), "scroll".to_string()),
            (format!("{}/{}", cfg.get_binding("diff", "page_up"), cfg.get_binding("diff", "page_down")), "page".to_string()),
            (format!("{}/{}", cfg.get_binding("diff", "scroll_top"), cfg.get_binding("diff", "scroll_bottom")), "top/bottom".to_string()),
            (format!("{}/{}", cfg.get_binding("diff", "next_file"), cfg.get_binding("diff", "prev_file")), "next/prev file".to_string()),
            (cfg.get_binding("diff", "exit"), "close".to_string()),
        ],
        AppMode::ConfirmQuit => vec![
            ("Y".to_string(), "quit".to_string()),
            ("N".to_string(), "cancel".to_string()),
        ],
        _ if app.interacting => {
            if app.active_pane == ActivePane::GitStatus {
                vec![
                    (format!("{}/{}", cfg.get_binding("file_list", "up"), cfg.get_binding("file_list", "down")), "select".to_string()),
                    (cfg.get_binding("file_list", "diff"), "diff".to_string()),
                    (cfg.get_binding("file_list", "stage"), "stage".to_string()),
                    (cfg.get_binding("file_list", "unstage"), "unstage".to_string()),
                    (cfg.get_binding("file_list", "edit_external"), "editor".to_string()),
                    (cfg.get_binding("interaction", "exit_interaction"), "back".to_string()),
                ]
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.markdown_content.is_some())
            {
                vec![
                    (format!("{}/{}", cfg.get_binding("markdown", "up"), cfg.get_binding("markdown", "down")), "scroll".to_string()),
                    (format!("{}/{}", cfg.get_binding("markdown", "page_up"), cfg.get_binding("markdown", "page_down")), "page".to_string()),
                    (format!("{}/{}", cfg.get_binding("markdown", "scroll_top"), cfg.get_binding("markdown", "scroll_bottom")), "top/bottom".to_string()),
                    (cfg.get_binding("interaction", "exit_interaction"), "back".to_string()),
                ]
            } else {
                vec![(cfg.get_binding("interaction", "exit_interaction"), "navigation mode".to_string())]
            }
        }
        _ => {
            let nav = format!("{}{}{}{}", cfg.get_binding("navigation", "up"), cfg.get_binding("navigation", "down"), cfg.get_binding("navigation", "left"), cfg.get_binding("navigation", "right"));
            let mut keys = vec![
                (nav, "navigate".to_string()),
                (cfg.get_binding("navigation", "enter_pane"), "interact".to_string()),
            ];
            match app.active_pane {
                ActivePane::WorkspaceList => {
                    keys.push((cfg.get_binding("navigation", "new_workspace"), "new ws".to_string()));
                    keys.push((cfg.get_binding("navigation", "clone_workspace"), "clone ws".to_string()));
                    keys.push((cfg.get_binding("navigation", "edit_workspace"), "edit ws".to_string()));
                    keys.push((cfg.get_binding("navigation", "delete_workspace"), "delete ws".to_string()));
                    keys.push((cfg.get_binding("navigation", "next_workspace"), "switch ws".to_string()));
                }
                ActivePane::GitStatus => {
                    keys.push((cfg.get_binding("navigation", "fuzzy_search"), "search".to_string()));
                    keys.push((cfg.get_binding("navigation", "commit"), "commit".to_string()));
                    keys.push((cfg.get_binding("navigation", "push"), "push".to_string()));
                    keys.push((cfg.get_binding("navigation", "merge"), "merge".to_string()));
                }
                ActivePane::MainPanel => {
                    keys.push((cfg.get_binding("navigation", "new_tab"), "new tab".to_string()));
                    keys.push((cfg.get_binding("navigation", "close_tab"), "close tab".to_string()));
                    keys.push((format!("{}/{}", cfg.get_binding("navigation", "next_tab"), cfg.get_binding("navigation", "prev_tab")), "next/prev tab".to_string()));
                }
            }
            keys.push((format!("{}/{}", cfg.get_binding("navigation", "sidebar_shrink"), cfg.get_binding("navigation", "sidebar_grow")), "resize".to_string()));
            keys.push((cfg.get_binding("navigation", "help"), "help".to_string()));
            keys.push((cfg.get_binding("navigation", "quit"), "quit".to_string()));
            keys
        },
    }
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys = footer_keys(app);

    let make_spans = |items: &[(String, String)], theme: &crate::theme::Theme| -> Vec<Span<'static>> {
        items
            .iter()
            .flat_map(|(key, desc)| {
                vec![
                    Span::styled(
                        format!(" [{}] ", key),
                        Style::default().fg(theme.footer.key),
                    ),
                    Span::styled(
                        format!("{} ", desc),
                        Style::default().fg(theme.footer.description),
                    ),
                ]
            })
            .collect()
    };

    // Calculate total width of all spans
    let total_width: usize = keys
        .iter()
        .map(|(key, desc)| format!(" [{}] {} ", key, desc).len())
        .sum();

    if total_width as u16 <= area.width || area.height < 2 {
        // Single line
        let spans = make_spans(&keys, &app.theme);
        let footer = Paragraph::new(Line::from(spans));
        frame.render_widget(footer, area);
    } else {
        // Split into two lines: find the split point closest to half
        let mut acc = 0usize;
        let half = total_width / 2;
        let mut split_at = keys.len();
        for (i, (key, desc)) in keys.iter().enumerate() {
            acc += format!(" [{}] {} ", key, desc).len();
            if acc >= half {
                split_at = i + 1;
                break;
            }
        }
        let line1 = Line::from(make_spans(&keys[..split_at], &app.theme));
        let line2 = Line::from(make_spans(&keys[split_at..], &app.theme));
        let footer = Paragraph::new(vec![line1, line2]);
        frame.render_widget(footer, area);
    }
}

fn render_diff_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width * 90 / 100;
    let height = area.height * 85 / 100;
    let popup = centered_rect(width, height, area);

    frame.render_widget(ratatui::widgets::Clear, popup);

    let file_path = app.diff_file_path.as_deref().unwrap_or("?");
    let border_style = Style::default().fg(app.theme.diff.border);
    super::diff::render(
        frame,
        popup,
        &app.diff_content,
        app.diff_scroll,
        file_path,
        border_style,
        app.theme.diff.empty_text,
    );
}

fn render_new_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width * 70 / 100;
    let popup = centered_rect(popup_width.max(40), 17, area);
    let theme = &app.theme.dialog;

    // Clear background
    frame.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" New Workspace ")
        .title_style(Style::default().fg(theme.new_ws_border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.new_ws_border));

    let field_style = |active: bool| {
        if active {
            Style::default().fg(theme.new_ws_active)
        } else {
            Style::default().fg(theme.new_ws_inactive)
        }
    };
    // Auto-scroll: show the tail of text when it exceeds available width
    let label_width = 10_u16; // "  Name: " = 8 + some padding
    let field_max = popup.width.saturating_sub(label_width + 2) as usize; // borders
    let visible_field = |text: &str, active: bool, cursor: usize| -> String {
        if !active {
            if text.len() > field_max && field_max > 2 {
                return format!("…{}", &text[text.len() - (field_max - 1)..]);
            }
            return text.to_string();
        }
        // Insert cursor block at position
        let before: String = text.chars().take(cursor).collect();
        let after: String = text.chars().skip(cursor).collect();
        let full = format!("{}█{}", before, after);
        if full.chars().count() > field_max && field_max > 2 {
            // Scroll to keep cursor visible
            let chars: Vec<char> = full.chars().collect();
            let cursor_display = before.chars().count(); // cursor block is at this index
            let start = if cursor_display + 2 > field_max {
                cursor_display + 2 - field_max
            } else {
                0
            };
            let visible: String = chars[start..chars.len().min(start + field_max - 1)].iter().collect();
            format!("…{}", visible)
        } else {
            full
        }
    };

    let name_active = app.active_dialog_field == DialogField::Name;
    let dir_active = app.active_dialog_field == DialogField::Directory;
    let desc_active = app.active_dialog_field == DialogField::Description;
    let prompt_active = app.active_dialog_field == DialogField::Prompt;
    let kanban_active = app.active_dialog_field == DialogField::KanbanPath;

    let lines = vec![
        Line::from(vec![
            Span::styled("  Name:   ", field_style(name_active)),
            Span::styled(
                visible_field(&app.input_buffer, name_active, app.input_cursor),
                field_style(name_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Dir:    ", field_style(dir_active)),
            Span::styled(
                visible_field(&app.dir_input_buffer, dir_active, app.dir_input_cursor),
                field_style(dir_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Desc:   ", field_style(desc_active)),
            Span::styled(
                visible_field(&app.desc_input_buffer, desc_active, app.desc_input_cursor),
                field_style(desc_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Prompt: ", field_style(prompt_active)),
            Span::styled(
                visible_field(&app.prompt_input_buffer, prompt_active, app.prompt_input_cursor),
                field_style(prompt_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Kanban: ", field_style(kanban_active)),
            Span::styled(
                visible_field(&app.kanban_input_buffer, kanban_active, app.kanban_input_cursor),
                field_style(kanban_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_edit_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width * 70 / 100;
    let popup = centered_rect(popup_width.max(40), 11, area);
    let theme = &app.theme.dialog;

    frame.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" Edit Workspace ")
        .title_style(Style::default().fg(theme.new_ws_border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.new_ws_border));

    let field_style = |active: bool| {
        if active {
            Style::default().fg(theme.new_ws_active)
        } else {
            Style::default().fg(theme.new_ws_inactive)
        }
    };

    let label_width = 10_u16;
    let field_max = popup.width.saturating_sub(label_width + 2) as usize;
    let visible_field = |text: &str, active: bool, cursor: usize| -> String {
        if !active {
            if text.len() > field_max && field_max > 2 {
                return format!("…{}", &text[text.len() - (field_max - 1)..]);
            }
            return text.to_string();
        }
        let before: String = text.chars().take(cursor).collect();
        let after: String = text.chars().skip(cursor).collect();
        let full = format!("{}█{}", before, after);
        if full.chars().count() > field_max && field_max > 2 {
            let chars: Vec<char> = full.chars().collect();
            let cursor_display = before.chars().count();
            let start = if cursor_display + 2 > field_max {
                cursor_display + 2 - field_max
            } else {
                0
            };
            let visible: String = chars[start..chars.len().min(start + field_max - 1)].iter().collect();
            format!("…{}", visible)
        } else {
            full
        }
    };

    let kanban_active = app.active_dialog_field == DialogField::KanbanPath;
    let prompt_active = app.active_dialog_field == DialogField::Prompt;

    let lines = vec![
        Line::from(vec![
            Span::styled("  Kanban: ", field_style(kanban_active)),
            Span::styled(
                visible_field(&app.kanban_input_buffer, kanban_active, app.kanban_input_cursor),
                field_style(kanban_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Prompt: ", field_style(prompt_active)),
            Span::styled(
                visible_field(&app.prompt_input_buffer, prompt_active, app.prompt_input_cursor),
                field_style(prompt_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let cfg = &app.config;
    let popup = centered_rect(55, 62, area);
    frame.render_widget(ratatui::widgets::Clear, popup);

    let help_text = vec![
        "".to_string(),
        "  Navigation mode (yellow border)".to_string(),
        format!("    {:<13} Move between panes", format!("{}/{}/{}/{}", cfg.get_binding("navigation", "up"), cfg.get_binding("navigation", "down"), cfg.get_binding("navigation", "left"), cfg.get_binding("navigation", "right"))),
        format!("    {:<13} Interact with pane", cfg.get_binding("navigation", "enter_pane")),
        format!("    {:<13} New workspace", cfg.get_binding("navigation", "new_workspace")),
        format!("    {:<13} Clone workspace", cfg.get_binding("navigation", "clone_workspace")),
        format!("    {:<13} Edit workspace", cfg.get_binding("navigation", "edit_workspace")),
        format!("    {:<13} Delete workspace", cfg.get_binding("navigation", "delete_workspace")),
        format!("    {:<13} Next/Prev workspace", format!("{}/{}", cfg.get_binding("navigation", "next_workspace"), cfg.get_binding("navigation", "prev_workspace"))),
        format!("    {:<13} Go to workspace N", "1-9"),
        format!("    {:<13} Next/Prev tab", format!("{}/{}", cfg.get_binding("navigation", "next_tab"), cfg.get_binding("navigation", "prev_tab"))),
        format!("    {:<13} New tab", cfg.get_binding("navigation", "new_tab")),
        format!("    {:<13} Close tab", cfg.get_binding("navigation", "close_tab")),
        format!("    {:<13} Toggle help", cfg.get_binding("navigation", "help")),
        format!("    {:<13} About", cfg.get_binding("navigation", "about")),
        format!("    {:<13} Quit", cfg.get_binding("navigation", "quit")),
        "".to_string(),
        "  Interaction mode (green border)".to_string(),
        format!("    {:<13} Back to navigation", cfg.get_binding("interaction", "exit_interaction")),
        "".to_string(),
        "  Terminal pane (navigation mode)".to_string(),
        format!("    {:<13} Scroll up/down (3 lines)", format!("{}/{}", cfg.get_binding("navigation", "scroll_up"), cfg.get_binding("navigation", "scroll_down"))),
        format!("    {:<13} Scroll by page", format!("{}/{}", cfg.get_binding("navigation", "page_up"), cfg.get_binding("navigation", "page_down"))),
        "    Mouse scroll  Scroll up/down".to_string(),
        "".to_string(),
        "  Terminal pane (interaction mode)".to_string(),
        "    All keys sent to active tab".to_string(),
        "".to_string(),
        "  File list pane".to_string(),
        format!("    {:<13} Select file", format!("{}/{}", cfg.get_binding("file_list", "up"), cfg.get_binding("file_list", "down"))),
        format!("    {:<13} Open diff", cfg.get_binding("file_list", "diff")),
        "".to_string(),
        "  Workspace list pane (interaction mode)".to_string(),
        format!("    {:<13} Select workspace", format!("{}/{}", cfg.get_binding("workspace_list", "up"), cfg.get_binding("workspace_list", "down"))),
        format!("    {:<13} Switch to workspace", cfg.get_binding("workspace_list", "select")),
        format!("    {:<13} Delete workspace", cfg.get_binding("workspace_list", "delete")),
        format!("    {:<13} Back to navigation", cfg.get_binding("interaction", "exit_interaction")),
        "".to_string(),
        "  Diff view".to_string(),
        format!("    {:<13} Scroll", format!("{}/{}", cfg.get_binding("diff", "up"), cfg.get_binding("diff", "down"))),
        format!("    {:<13} Page down/up", format!("{}/{}", cfg.get_binding("diff", "page_up"), cfg.get_binding("diff", "page_down"))),
        format!("    {:<13} Top/Bottom", format!("{}/{}", cfg.get_binding("diff", "scroll_top"), cfg.get_binding("diff", "scroll_bottom"))),
        format!("    {:<13} Next/Prev file", format!("{}/{}", cfg.get_binding("diff", "next_file"), cfg.get_binding("diff", "prev_file"))),
        format!("    {:<13} Close diff", cfg.get_binding("diff", "exit")),
        "".to_string(),
        format!("  Fuzzy search ({} or {})", cfg.get_binding("navigation", "fuzzy_search"), cfg.get_binding("navigation", "fuzzy_search_alt")).to_string(),
        "    Type          Filter files".to_string(),
        format!("    {:<13} Select result", format!("{}/{}", cfg.get_binding("fuzzy", "up"), cfg.get_binding("fuzzy", "down"))),
        format!("    {:<13} Open diff", cfg.get_binding("fuzzy", "diff")),
        format!("    {:<13} Open in $EDITOR", cfg.get_binding("fuzzy", "editor")),
        format!("    {:<13} Inline editor", cfg.get_binding("fuzzy", "inline_edit")),
        format!("    {:<13} Open markdown viewer", cfg.get_binding("fuzzy", "markdown")),
        format!("    {:<13} Open in mdr (external)", cfg.get_binding("fuzzy", "mdr")),
        format!("    {:<13} Close", cfg.get_binding("fuzzy", "exit")),
        "".to_string(),
        "  File list (interaction mode)".to_string(),
        format!("    {:<13} Open in $EDITOR", cfg.get_binding("file_list", "edit_external")),
        format!("    {:<13} Inline editor", cfg.get_binding("file_list", "edit_inline")),
        format!("    {:<13} Stage file (git add)", cfg.get_binding("file_list", "stage")),
        format!("    {:<13} Unstage file (git reset)", cfg.get_binding("file_list", "unstage")),
        "".to_string(),
        "  Git operations".to_string(),
        format!("    {:<13} Commit (opens dialog)", cfg.get_binding("navigation", "commit")),
        format!("    {:<13} Push", cfg.get_binding("navigation", "push")),
        "".to_string(),
        "  Inline editor".to_string(),
        format!("    {:<13} Save", cfg.get_binding("editor", "save")),
        format!("    {:<13} Close", cfg.get_binding("editor", "exit")),
        "".to_string(),
        "  Pane resize".to_string(),
        format!("    {:<13} Resize sidebar width", format!("{} / {}", cfg.get_binding("navigation", "sidebar_shrink"), cfg.get_binding("navigation", "sidebar_grow"))),
        format!("    {:<13} Resize workspace/file split", format!("{} / {}", cfg.get_binding("navigation", "split_up"), cfg.get_binding("navigation", "split_down"))),
        "    Mouse drag    Drag pane borders to resize".to_string(),
        "".to_string(),
        "  Clipboard".to_string(),
        "    Mouse drag    Select text in terminal".to_string(),
        format!("    {:<13} Copy visible terminal content", cfg.get_binding("interaction", "copy")),
        format!("    {:<13} Paste from clipboard (terminal)", cfg.get_binding("interaction", "paste")),
    ];

    let block = Block::default()
        .title(" Help ")
        .title_style(Style::default().fg(theme.help.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.help.border));

    let total_lines = help_text.len() as u16;
    let inner_height = popup.height.saturating_sub(2); // borders
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = app.help_scroll.min(max_scroll);

    let scroll_indicator = if max_scroll > 0 {
        format!(" [{}/{} ↑{}/{}↓] ", scroll + 1, max_scroll + 1, cfg.get_binding("help", "up"), cfg.get_binding("help", "down"))
    } else {
        String::new()
    };

    let block = block.title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(help_text.join("\n"))
        .block(block)
        .scroll((scroll, 0));
    frame.render_widget(text, popup);
}

fn render_about_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let popup = centered_rect(50, 20, area);
    frame.render_widget(ratatui::widgets::Clear, popup);

    let version = env!("CARGO_PKG_VERSION");

    let version_line = format!("piki-multi-ai v{version}");
    let about_lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(""),
        Line::from("██████╗ ██╗██╗  ██╗██╗"),
        Line::from("██╔══██╗██║██║ ██╔╝██║"),
        Line::from("██████╔╝██║█████╔╝ ██║"),
        Line::from("██╔═══╝ ██║██╔═██╗ ██║"),
        Line::from("██║     ██║██║  ██╗██║"),
        Line::from("╚═╝     ╚═╝╚═╝  ╚═╝╚═╝"),
        Line::from(""),
        Line::from(version_line),
        Line::from(""),
        Line::from("Author: Juan Knebel"),
        Line::from("Contact: juanknebel@gmail.com"),
        Line::from("Web: github.com/juanknebel/piki-multi-ai"),
        Line::from("License: GPL-2.0"),
        Line::from(""),
        Line::from("Press Esc to close"),
    ];

    let block = Block::default()
        .title(" About ")
        .title_style(Style::default().fg(theme.help.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.help.border));

    let text = Paragraph::new(about_lines)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(text, popup);
}

fn render_workspace_info_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let ws = &app.workspaces[app.selected_workspace];

    let label_style = Style::default().add_modifier(Modifier::BOLD).fg(theme.help.border);
    let project_name = ws
        .source_repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ws.source_repo.to_string_lossy().to_string());

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" Branch:  ", label_style),
            Span::raw(ws.branch.clone()),
        ]),
        Line::from(vec![
            Span::styled(" Project: ", label_style),
            Span::raw(project_name),
        ]),
        Line::from(vec![
            Span::styled("          ", label_style),
            Span::raw(ws.source_repo.to_string_lossy().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Path:    ", label_style),
            Span::raw(ws.path.to_string_lossy().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Kanban:  ", label_style),
            Span::raw(ws.kanban_path.clone().unwrap_or_else(|| "default".to_string())),
        ]),
        Line::from(""),
    ];

    if !ws.description.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(" Description: ", Style::default().add_modifier(Modifier::BOLD).fg(theme.help.border)),
        ]));
        lines.push(Line::from(format!("  {}", ws.description)));
        lines.push(Line::from(""));
    }

    if !ws.prompt.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(" Prompt: ", Style::default().add_modifier(Modifier::BOLD).fg(theme.help.border)),
        ]));
        // Wrap prompt text
        let max_width = 56usize;
        for chunk in ws.prompt.as_bytes().chunks(max_width) {
            let s = String::from_utf8_lossy(chunk);
            lines.push(Line::from(format!("  {}", s)));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(
        Span::styled(" Esc to close · h/l to scroll · mouse select to copy", Style::default().fg(Color::DarkGray)),
    ));

    let height = (lines.len() as u16 + 2).min(area.height);
    let popup = centered_rect(70, height, area);
    frame.render_widget(ratatui::widgets::Clear, popup);

    let title = format!(" {} ", ws.name);
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(theme.help.border).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.help.border));

    let text = Paragraph::new(lines)
        .block(block)
        .scroll((0, app.info_hscroll));
    frame.render_widget(text, popup);
}

/// Helper to create a centered rect with fixed width (chars) and height (lines)
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn render_confirm_delete_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(50, 9, area);
    frame.render_widget(ratatui::widgets::Clear, popup);
    let theme = &app.theme.dialog;

    let ws_name = app
        .delete_target
        .and_then(|idx| app.workspaces.get(idx))
        .map(|ws| ws.name.as_str())
        .unwrap_or("?");

    let block = Block::default()
        .title(" Delete Workspace ")
        .title_style(Style::default().fg(theme.delete_border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.delete_border));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Delete ", Style::default().fg(theme.delete_text)),
            Span::styled(
                ws_name,
                Style::default()
                    .fg(theme.delete_name)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ?", Style::default().fg(theme.delete_text)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [y] Yes, delete worktree and branch",
            Style::default().fg(theme.delete_yes),
        )),
        Line::from(Span::styled(
            "  [n] No, keep worktree on disk",
            Style::default().fg(theme.delete_no),
        )),
        Line::from(Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.delete_cancel),
        )),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_confirm_quit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(40, 7, area);
    frame.render_widget(ratatui::widgets::Clear, popup);
    let theme = &app.theme.dialog;

    let block = Block::default()
        .title(" Quit ")
        .title_style(Style::default().fg(theme.delete_border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.delete_border));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Are you sure you want to quit?",
            Style::default().fg(theme.delete_text),
        )).centered(),
        Line::from(""),
        Line::from(Span::styled(
            "[Y] Yes    [N] No",
            Style::default().fg(theme.delete_cancel),
        )).centered(),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_commit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width * 60 / 100;
    let popup = centered_rect(popup_width.max(40), 7, area);
    let theme = &app.theme.dialog;

    frame.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" Commit ")
        .title_style(Style::default().fg(theme.new_ws_border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.new_ws_border));

    let field_max = popup.width.saturating_sub(14) as usize;
    let cursor = "█";
    let full = format!("{}{}", app.commit_msg_buffer, cursor);
    let visible = if full.len() > field_max && field_max > 2 {
        format!("…{}", &full[full.len() - (field_max - 1)..])
    } else {
        full
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Message: ", Style::default().fg(theme.new_ws_active)),
            Span::styled(visible, Style::default().fg(theme.new_ws_active)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter] Commit  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_confirm_merge_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(50, 9, area);
    frame.render_widget(ratatui::widgets::Clear, popup);
    let theme = &app.theme.dialog;

    let branch_name = app
        .current_workspace()
        .map(|ws| ws.branch.as_str())
        .unwrap_or("?");

    let block = Block::default()
        .title(" Merge ")
        .title_style(Style::default().fg(theme.new_ws_border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.new_ws_border));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Merge ", Style::default().fg(theme.new_ws_active)),
            Span::styled(
                branch_name,
                Style::default()
                    .fg(theme.new_ws_active)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" into main?", Style::default().fg(theme.new_ws_active)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [m] Merge commit",
            Style::default().fg(theme.new_ws_inactive),
        )),
        Line::from(Span::styled(
            "  [r] Rebase + fast-forward",
            Style::default().fg(theme.new_ws_inactive),
        )),
        Line::from(Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_new_tab_dialog(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(40, 11, area);
    frame.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" New Tab ")
        .title_style(Style::default().fg(ratatui::style::Color::Cyan))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ratatui::style::Color::Cyan));

    let lines = vec![
        Line::from(""),
        Line::from("  Select provider:"),
        Line::from(""),
        Line::from("  [1] Claude Code"),
        Line::from("  [2] Gemini"),
        Line::from("  [3] Codex"),
        Line::from("  [4] Shell"),
        Line::from("  [5] Kanban Board"),
        Line::from(""),
        Line::from("  [Esc] Cancel"),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}
