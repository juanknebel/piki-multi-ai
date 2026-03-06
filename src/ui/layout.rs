use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};

use crate::app::{ActivePane, App, AppMode, DialogField, FileStatus};
use crate::theme::Theme;

/// Compute the inner terminal area (minus borders) for a given total terminal size.
/// Replicates layout math to find the main content area dimensions.
pub fn compute_terminal_area(total: Rect) -> Rect {
    // Main vertical split: content + footer
    let [content_area, _footer] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(total);

    // Horizontal split: left sidebar + right main panel
    let [_left, right_area] =
        Layout::horizontal([Constraint::Percentage(20), Constraint::Percentage(80)])
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
pub fn render(frame: &mut Frame, app: &mut App) {
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
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(left_area);

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
        render_help_overlay(frame, area, &app.theme);
    }
    if app.mode == AppMode::ConfirmDelete {
        render_confirm_delete_dialog(frame, area, app);
    }
    if app.mode == AppMode::FuzzySearch {
        super::fuzzy::render(frame, area, app);
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
        super::subtabs::render(frame, area, ws.active_provider, &app.theme);
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
        let provider = ws.active_provider;
        if let Some(parser) = ws.pty_parsers.get(&provider) {
            super::terminal::render(
                frame,
                area,
                parser,
                border_style,
                provider.label(),
                ws.term_scroll,
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
    then press [g] to switch providers."#
            );
            let text = Paragraph::new(ascii_art)
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
                    let scroll_info = if ws.term_scroll > 0 {
                        format!(" | SCROLL -{}", ws.term_scroll)
                    } else {
                        String::new()
                    };
                    Span::styled(
                        format!(
                            " [{}] branch: {} | {} files | {}: {} | ws {}/{}{}",
                            mode_label,
                            ws.branch,
                            ws.file_count(),
                            ws.active_provider.label(),
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

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys = match app.mode {
        AppMode::FuzzySearch => vec![
            ("↑↓", "select"),
            ("Enter", "diff"),
            ("C-e", "editor"),
            ("C-v", "inline edit"),
            ("Esc", "close"),
        ],
        AppMode::InlineEdit => vec![("C-s", "save"), ("Esc", "close")],
        AppMode::NewWorkspace => vec![
            ("Tab", "switch field"),
            ("Enter", "create"),
            ("Esc", "cancel"),
        ],
        _ if app.interacting => vec![("C-g", "navigation mode")],
        _ => vec![
            ("hjkl", "navigate"),
            ("Enter", "interact"),
            ("n", "new"),
            ("d", "delete"),
            ("Tab", "switch ws"),
            ("/", "search"),
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
                    Style::default().fg(app.theme.footer.key),
                ),
                Span::styled(
                    format!("{} ", desc),
                    Style::default().fg(app.theme.footer.description),
                ),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
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
    let cursor_char = |active: bool| if active { "█" } else { "" };

    // Auto-scroll: show the tail of text when it exceeds available width
    let label_width = 10_u16; // "  Name: " = 8 + some padding
    let field_max = popup.width.saturating_sub(label_width + 2) as usize; // borders
    let visible_field = |text: &str, active: bool| -> String {
        let suffix = cursor_char(active);
        let full = format!("{}{}", text, suffix);
        if full.len() > field_max && field_max > 2 {
            format!("…{}", &full[full.len() - (field_max - 1)..])
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
                visible_field(&app.input_buffer, name_active),
                field_style(name_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Dir:    ", field_style(dir_active)),
            Span::styled(
                visible_field(&app.dir_input_buffer, dir_active),
                field_style(dir_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Desc:   ", field_style(desc_active)),
            Span::styled(
                visible_field(&app.desc_input_buffer, desc_active),
                field_style(desc_active),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Prompt: ", field_style(prompt_active)),
            Span::styled(
                visible_field(&app.prompt_input_buffer, prompt_active),
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

fn render_help_overlay(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup = centered_rect(50, 50, area);
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
        "    Ctrl+g        Back to navigation",
        "",
        "  Terminal pane (navigation mode)",
        "    Shift+K/J     Scroll up/down (3 lines)",
        "    PageUp/Down   Scroll by page",
        "    Mouse scroll  Scroll up/down",
        "",
        "  Terminal pane (interaction mode)",
        "    All keys sent to active provider",
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
        "    Ctrl+g        Close diff",
        "",
        "  Fuzzy search (/ or Ctrl+F)",
        "    Type          Filter files",
        "    ↑/↓           Select result",
        "    Enter         Open diff",
        "    Ctrl+E        Open in $EDITOR",
        "    Ctrl+V        Inline editor",
        "    Esc           Close",
        "",
        "  File list (interaction mode)",
        "    e             Open in $EDITOR",
        "    v             Inline editor",
        "",
        "  Inline editor",
        "    Ctrl+S        Save",
        "    Esc           Close",
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

    let text = Paragraph::new(help_text.join("\n")).block(block);
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
