use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};

use crate::app::{ActivePane, App, AppMode, DialogField, FileStatus};

/// Border style for a pane: green if interacting, yellow if selected, white otherwise
fn pane_border_style(app: &App, pane: ActivePane) -> Style {
    if app.active_pane == pane {
        if app.interacting {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Yellow)
        }
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Render the main application layout
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main vertical split: content + footer
    let [content_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    // Horizontal split: left sidebar + right main panel
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(20), Constraint::Percentage(80)])
            .areas(content_area);

    // Left panel: workspaces (top) + files (bottom)
    let [ws_area, files_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(left_area);

    // Right panel: tabs + sub-tabs + content + status bar
    let [tabs_area, subtabs_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(right_area);

    // --- Render panels ---

    // Left top: workspace list
    render_workspace_list(frame, ws_area, app);

    // Left bottom: changed files
    render_file_list(frame, files_area, app);

    // Right top: workspace tabs
    render_tab_bar(frame, tabs_area, app);

    // Right: AI provider sub-tabs
    render_subtabs(frame, subtabs_area, app);

    // Right center: main content (PTY or Diff)
    render_main_content(frame, main_area, app);

    // Right bottom: status bar
    render_status_bar(frame, status_area, app);

    // Footer: keybindings
    render_footer(frame, footer_area, app);

    // Overlays
    if app.mode == AppMode::NewWorkspace {
        render_new_workspace_dialog(frame, area, app);
    }
    if app.mode == AppMode::Help {
        render_help_overlay(frame, area);
    }
}

fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::WorkspaceList;
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.workspaces.is_empty() {
        let text = Paragraph::new("  Press [n] to create")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = app
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let marker = if i == app.active_workspace { "▶" } else { " " };
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
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ),
            ]);
            let line2 = Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!("{} {} | {} files", status_icon, ws.status_label(), ws.file_count()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let style = if i == app.selected_workspace && is_active {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(vec![line1, line2]).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_file_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::FileList;
    let border_style = pane_border_style(app, ActivePane::FileList);

    let block = Block::default()
        .title(" STATUS ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    let files = app
        .current_workspace()
        .map(|ws| &ws.changed_files[..])
        .unwrap_or(&[]);

    if files.is_empty() {
        let text = Paragraph::new("  No files changed")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (label, color) = match f.status {
                FileStatus::Modified => ("M", Color::Yellow),
                FileStatus::Added => ("A", Color::Green),
                FileStatus::Deleted => ("D", Color::Red),
                FileStatus::Renamed => ("R", Color::Cyan),
                FileStatus::Untracked => ("?", Color::DarkGray),
                FileStatus::Conflicted => ("C", Color::Magenta),
                FileStatus::Staged => ("S", Color::Green),
                FileStatus::StagedModified => ("SM", Color::Yellow),
            };
            let line = Line::from(vec![
                Span::styled(format!("  {} ", label), Style::default().fg(color)),
                Span::styled(&f.path, Style::default().fg(Color::White)),
            ]);
            let style = if i == app.selected_file && is_active {
                Style::default().bg(Color::DarkGray)
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
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(Color::DarkGray))
        .divider("|");

    frame.render_widget(tabs, area);
}

fn render_subtabs(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(ws) = app.current_workspace() {
        super::subtabs::render(frame, area, ws.active_provider);
    } else {
        let block = Block::default().borders(Borders::BOTTOM);
        frame.render_widget(block, area);
    }
}

fn render_main_content(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = pane_border_style(app, ActivePane::MainPanel);

    match app.mode {
        AppMode::Diff => {
            let file_path = app.diff_file_path.as_deref().unwrap_or("?");
            super::diff::render(frame, area, &app.diff_content, app.diff_scroll, file_path, border_style);
        }
        _ => {
            if let Some(ws) = app.current_workspace() {
                let provider = ws.active_provider;
                if let Some(parser) = ws.pty_parsers.get(&provider) {
                    super::terminal::render(frame, area, parser, border_style, provider.label());
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
    then press [g] to switch providers."#
                    );
                    let text = Paragraph::new(ascii_art)
                        .style(Style::default().fg(Color::DarkGray))
                        .block(block);
                    frame.render_widget(text, area);
                }
            } else {
                let block = Block::default()
                    .title(" agent-multi ")
                    .title_style(border_style)
                    .borders(Borders::ALL)
                    .border_style(border_style);
                let text = Paragraph::new(
                    "  Welcome to agent-multi\n\n  Press [n] to create a new workspace\n  Press [?] for help\n  Press [q] to quit",
                )
                .style(Style::default().fg(Color::Gray))
                .block(block);
                frame.render_widget(text, area);
            }
        }
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(msg) = &app.status_message {
        Span::styled(
            format!(" {} ", msg),
            Style::default().bg(Color::Red).fg(Color::White),
        )
    } else {
        match app.mode {
            AppMode::Diff => Span::styled(
                format!(
                    " DIFF: {} | [Esc] back | [↑↓] scroll | [n/p] file",
                    app.diff_file_path.as_deref().unwrap_or("?")
                ),
                Style::default().bg(Color::DarkGray).fg(Color::White),
            ),
            _ => {
                let mode_label = if app.interacting { "INTERACT" } else { "NAVIGATE" };
                let mode_color = if app.interacting { Color::Green } else { Color::Yellow };
                if let Some(ws) = app.current_workspace() {
                    Span::styled(
                        format!(
                            " [{}] branch: {} | {} files | {}: {} | ws {}/{}",
                            mode_label,
                            ws.branch,
                            ws.file_count(),
                            ws.active_provider.label(),
                            ws.status_label(),
                            app.active_workspace + 1,
                            app.workspaces.len(),
                        ),
                        Style::default().bg(mode_color).fg(Color::Black),
                    )
                } else {
                    Span::styled(
                        format!(" [{}] No active workspace", mode_label),
                        Style::default().bg(mode_color).fg(Color::Black),
                    )
                }
            }
        }
    };

    let bar = Paragraph::new(Line::from(content));
    frame.render_widget(bar, area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys = match app.mode {
        AppMode::NewWorkspace => vec![
            ("Tab", "switch field"),
            ("Enter", "create"),
            ("Esc", "cancel"),
        ],
        _ if app.interacting => vec![
            ("Esc", "navigation mode"),
        ],
        _ => vec![
            ("hjkl", "navigate"),
            ("Enter", "interact"),
            ("n", "new"),
            ("d", "delete"),
            ("Tab", "switch ws"),
            ("g", "switch AI"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };

    let spans: Vec<Span> = keys
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!(" [{}] ", key),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(format!("{} ", desc), Style::default().fg(Color::Gray)),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

fn render_new_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(50, 7, area);

    // Clear background
    frame.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" New Workspace ")
        .title_style(Style::default().fg(Color::Yellow))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let name_active = app.active_dialog_field == DialogField::Name;
    let dir_active = app.active_dialog_field == DialogField::Directory;

    let name_style = if name_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let dir_style = if dir_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let name_cursor = if name_active { "█" } else { "" };
    let dir_cursor = if dir_active { "█" } else { "" };

    let lines = vec![
        Line::from(vec![
            Span::styled("  Name: ", name_style),
            Span::styled(format!("{}{}", app.input_buffer, name_cursor), name_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Dir:  ", dir_style),
            Span::styled(format!("{}{}", app.dir_input_buffer, dir_cursor), dir_style),
        ]),
    ];

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(50, 30, area);
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
        "    g             Cycle AI provider",
        "    ?             Toggle help",
        "    q             Quit",
        "",
        "  Interaction mode (green border)",
        "    Esc           Back to navigation",
        "",
        "  Terminal pane",
        "    All keys sent to Claude Code",
        "",
        "  File list pane",
        "    j/k           Select file",
        "    Enter         Open diff",
        "",
        "  Diff view",
        "    j/k           Scroll",
        "    Ctrl+d/u      Page down/up",
        "    g/G           Top/Bottom",
        "    n/p           Next/Prev file",
        "    Esc           Close diff",
    ];

    let block = Block::default()
        .title(" Help ")
        .title_style(Style::default().fg(Color::Cyan))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let text = Paragraph::new(help_text.join("\n")).block(block);
    frame.render_widget(text, popup);
}

/// Helper to create a centered rect with fixed width (chars) and height (lines)
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(
        x,
        y,
        width.min(area.width),
        height.min(area.height),
    )
}
