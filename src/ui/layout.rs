use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
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

    // Store layout rects for mouse-based resize detection
    app.sidebar_x = left_area.x + left_area.width;
    app.left_split_y = ws_area.y + ws_area.height;
    app.left_area_rect = left_area;

    // Right panel: tabs + sub-tabs + content + status bar
    let [tabs_area, subtabs_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(right_area);

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
            if !ws.description.is_empty() {
                let max_desc = area.width.saturating_sub(6) as usize;
                let desc = if ws.description.len() > max_desc {
                    format!("{}…", &ws.description[..max_desc.saturating_sub(1)])
                } else {
                    ws.description.clone()
                };
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        desc,
                        Style::default()
                            .fg(detail_color)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }

            // Show worktree path
            let path_str = ws.path.to_string_lossy();
            let max_path = area.width.saturating_sub(6) as usize;
            let path_display = if path_str.len() > max_path {
                format!(
                    "…{}",
                    &path_str[path_str.len() - max_path.saturating_sub(1)..]
                )
            } else {
                path_str.to_string()
            };
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(path_display, Style::default().fg(detail_color)),
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
    let is_active = app.active_pane == ActivePane::FileList;
    let border_style = pane_border_style(app, ActivePane::FileList);
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

fn footer_keys(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.mode {
        AppMode::FuzzySearch => vec![
            ("↑↓", "select"),
            ("Enter", "diff"),
            ("C-e", "editor"),
            ("C-v", "inline edit"),
            ("C-o", "markdown"),
            ("A-m", "mdr"),
            ("Esc", "close"),
        ],
        AppMode::InlineEdit => vec![("C-s", "save"), ("Esc", "close")],
        AppMode::NewWorkspace => vec![
            ("Tab", "switch field"),
            ("Enter", "create"),
            ("Esc", "cancel"),
        ],
        AppMode::CommitMessage => vec![("Enter", "commit"), ("Esc", "cancel")],
        AppMode::ConfirmMerge => vec![("m", "merge"), ("r", "rebase"), ("Esc", "cancel")],
        AppMode::NewTab => vec![
            ("1", "Claude"),
            ("2", "Gemini"),
            ("3", "Codex"),
            ("4", "Shell"),
            ("Esc", "cancel"),
        ],
        AppMode::Diff => vec![
            ("j/k", "scroll"),
            ("C-d/u", "page"),
            ("g/G", "top/bottom"),
            ("n/p", "next/prev file"),
            ("Esc", "close"),
        ],
        _ if app.interacting => {
            if app.active_pane == ActivePane::FileList {
                vec![
                    ("j/k", "select"),
                    ("Enter", "diff"),
                    ("s", "stage"),
                    ("u", "unstage"),
                    ("e", "editor"),
                    ("C-g", "back"),
                ]
            } else if app
                .current_workspace()
                .and_then(|ws| ws.current_tab())
                .is_some_and(|tab| tab.markdown_content.is_some())
            {
                vec![
                    ("j/k", "scroll"),
                    ("C-d/u", "page"),
                    ("g/G", "top/bottom"),
                    ("C-g", "back"),
                ]
            } else {
                vec![("C-g", "navigation mode")]
            }
        }
        _ => vec![
            ("hjkl", "navigate"),
            ("Enter", "interact"),
            ("n", "new ws"),
            ("d", "delete ws"),
            ("t", "new tab"),
            ("w", "close tab"),
            ("g/G", "next/prev tab"),
            ("c", "commit"),
            ("P", "push"),
            ("M", "merge"),
            ("Tab", "switch ws"),
            ("/", "search"),
            ("</>", "resize"),
            ("?", "help"),
            ("q", "quit"),
        ],
    }
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys = footer_keys(app);

    let make_spans = |items: &[(&str, &str)], theme: &crate::theme::Theme| -> Vec<Span<'static>> {
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
    let popup = centered_rect(popup_width.max(40), 15, area);
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
    let popup = centered_rect(55, 58, area);
    frame.render_widget(ratatui::widgets::Clear, popup);

    let help_text = vec![
        "",
        "  Navigation mode (yellow border)",
        "    h/j/k/l      Move between panes",
        "    Enter         Interact with pane",
        "    n             New workspace",
        "    d             Delete workspace",
        "    Tab/S-Tab     Next/Prev workspace",
        "    1-9           Go to workspace N",
        "    g/G           Next/Prev tab",
        "    t             New tab",
        "    w             Close tab",
        "    ?             Toggle help",
        "    a             About",
        "    q             Quit",
        "",
        "  Interaction mode (green border)",
        "    Ctrl+g        Back to navigation",
        "",
        "  Terminal pane (navigation mode)",
        "    Shift+K/J     Scroll up/down (3 lines)",
        "    PageUp/Down   Scroll by page",
        "    Mouse scroll  Scroll up/down",
        "",
        "  Terminal pane (interaction mode)",
        "    All keys sent to active tab",
        "",
        "  File list pane",
        "    j/k           Select file",
        "    Enter         Open diff",
        "",
        "  Workspace list pane (interaction mode)",
        "    j/k           Select workspace",
        "    Enter         Switch to workspace",
        "    d             Delete workspace",
        "    Ctrl+g        Back to navigation",
        "",
        "  Diff view",
        "    j/k           Scroll",
        "    Ctrl+d/u      Page down/up",
        "    g/G           Top/Bottom",
        "    n/p           Next/Prev file",
        "    Esc           Close diff",
        "",
        "  Fuzzy search (/ or Ctrl+F)",
        "    Type          Filter files",
        "    ↑/↓           Select result",
        "    Enter         Open diff",
        "    Ctrl+E        Open in $EDITOR",
        "    Ctrl+V        Inline editor",
        "    Ctrl+O        Open markdown viewer",
        "    Alt+M         Open in mdr (external)",
        "    Esc           Close",
        "",
        "  File list (interaction mode)",
        "    e             Open in $EDITOR",
        "    v             Inline editor",
        "    s             Stage file (git add)",
        "    u             Unstage file (git reset)",
        "",
        "  Git operations",
        "    c             Commit (opens dialog)",
        "    P             Push",
        "",
        "  Inline editor",
        "    Ctrl+S        Save",
        "    Esc           Close",
        "",
        "  Pane resize",
        "    < / >         Resize sidebar width",
        "    + / -         Resize workspace/file split",
        "    Mouse drag    Drag pane borders to resize",
        "",
        "  Clipboard",
        "    Mouse drag    Select text in terminal",
        "    Ctrl+Shift+C  Copy visible terminal content",
        "    Ctrl+Shift+V  Paste from clipboard (terminal)",
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
        format!(" [{}/{} ↑j/k↓] ", scroll + 1, max_scroll + 1)
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
    let popup = centered_rect(40, 10, area);
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
        Line::from(""),
        Line::from("  [Esc] Cancel"),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}
