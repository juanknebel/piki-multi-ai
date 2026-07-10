use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, SidebarItem};
use piki_core::WorkspaceType;

use super::layout::{pane_border_style, pane_title_style};

/// Icon prefix for each workspace type.
fn workspace_type_icon(ws_type: WorkspaceType) -> &'static str {
    match ws_type {
        WorkspaceType::Worktree => "⎇ ",
        WorkspaceType::Project => "▣ ",
        WorkspaceType::Simple => "○ ",
    }
}

/// Returns the visual height (in lines) of a sidebar item at the given index.
/// Workspace items that follow another workspace get an extra separator line.
pub(super) fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);
    let theme = &app.theme.workspace_list;
    // Selection has two temperatures: the iris wash where the focus is, a
    // neutral raised surface where it is not — you never lose your place.
    let sel_bg = if app.active_pane == ActivePane::WorkspaceList {
        theme.selected_bg
    } else {
        app.theme.palette.bg2
    };

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(pane_title_style(app, ActivePane::WorkspaceList))
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    if app.workspaces.is_empty() {
        let key_style = Style::default().fg(app.theme.footer.key);
        let desc_style = Style::default().fg(theme.empty_text);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!(" [{}]", app.config.get_binding("app", "new_workspace")),
                    key_style,
                ),
                Span::styled(" New workspace", desc_style),
            ]),
        ];
        let text = Paragraph::new(lines).block(block);
        frame.render_widget(text, area);
        return;
    }

    let sidebar_items = app.sidebar_items();

    // All rows are one line tall; scroll follows the selection.
    let visible_height = area.height.saturating_sub(2) as usize;
    let selected = app
        .selected_sidebar_row
        .min(sidebar_items.len().saturating_sub(1));
    let scroll_offset = if visible_height > 0 && selected >= visible_height {
        selected + 1 - visible_height
    } else {
        0
    };

    let inner_width = area.width.saturating_sub(2) as usize;
    let mut ws_visual_idx: usize = 0;

    let items: Vec<ListItem> = sidebar_items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(row, item)| {
            let is_selected = row == app.selected_sidebar_row;
            match item {
                SidebarItem::GroupHeader {
                    name,
                    count,
                    collapsed,
                } => {
                    let arrow = if *collapsed { "▸" } else { "▼" };
                    let header_style = Style::default()
                        .fg(theme.name_inactive)
                        .bg(theme.group_header_bg)
                        .add_modifier(Modifier::BOLD);
                    let line = Line::from(vec![
                        Span::styled(format!(" {} ", arrow), header_style),
                        Span::styled(format!("{} ({})", name.to_uppercase(), count), header_style),
                    ]);
                    let style = if is_selected {
                        Style::default().bg(sel_bg)
                    } else {
                        Style::default().bg(theme.group_header_bg)
                    };
                    ListItem::new(vec![line]).style(style)
                }
                SidebarItem::Workspace { index } => {
                    let ws = &app.workspaces[*index];
                    let current_ws_idx = ws_visual_idx;
                    ws_visual_idx += 1;

                    let detail_color = if is_selected {
                        theme.detail_selected
                    } else {
                        theme.detail_normal
                    };

                    let marker = if *index == app.active_workspace {
                        "▶"
                    } else {
                        " "
                    };
                    let type_icon = workspace_type_icon(ws.info.workspace_type);

                    let mut left: Vec<Span> = vec![
                        Span::raw(format!(" {} ", marker)),
                        Span::styled(type_icon, Style::default().fg(detail_color)),
                        Span::styled(
                            ws.name.clone(),
                            if *index == app.active_workspace {
                                Style::default()
                                    .fg(theme.name_active)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.name_inactive)
                            },
                        ),
                    ];
                    if ws.has_idle_notification {
                        left.push(Span::styled(
                            " ●",
                            Style::default()
                                .fg(app.theme.status.needs_you)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }

                    // Right-aligned metadata, shown only when it says something:
                    // actionable agent status, changed-file count, ahead/behind.
                    // Activity (running) stays in the Agents pane.
                    let mut right: Vec<Span> = Vec::new();
                    if let Some((status, attention)) = ws.agent_status_rollup()
                        && let Some((glyph, color)) =
                            crate::ui::actionable_status_view(&app.theme, status, attention)
                    {
                        right.push(Span::styled(
                            glyph.to_string(),
                            Style::default().fg(color),
                        ));
                    }
                    let changed = ws.file_count();
                    if changed > 0 {
                        if !right.is_empty() {
                            right.push(Span::raw(" "));
                        }
                        right.push(Span::styled(
                            format!("{}∆", changed),
                            Style::default().fg(detail_color),
                        ));
                    }
                    if let Some((ahead, behind)) = ws.ahead_behind
                        && (ahead > 0 || behind > 0)
                    {
                        if !right.is_empty() {
                            right.push(Span::raw(" "));
                        }
                        let mut ab = String::new();
                        if ahead > 0 {
                            ab.push_str(&format!("↑{}", ahead));
                        }
                        if behind > 0 {
                            if ahead > 0 {
                                ab.push(' ');
                            }
                            ab.push_str(&format!("↓{}", behind));
                        }
                        right.push(Span::styled(ab, Style::default().fg(detail_color)));
                    }

                    // The name wins over metadata when width is tight
                    let left_w: usize = left.iter().map(|sp| sp.width()).sum();
                    let right_w: usize = right.iter().map(|sp| sp.width()).sum();
                    let mut spans = left;
                    if right_w > 0 && left_w + right_w + 2 <= inner_width {
                        spans.push(Span::raw(" ".repeat(inner_width - left_w - right_w - 1)));
                        spans.extend(right);
                    }

                    let style = if is_selected {
                        Style::default().bg(sel_bg)
                    } else if current_ws_idx % 2 == 1 {
                        Style::default().bg(theme.alt_bg)
                    } else {
                        Style::default()
                    };
                    ListItem::new(vec![Line::from(spans)]).style(style)
                }
            }
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);

    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        sidebar_items.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
}

/// Bottom-left pane: active AI agents across ALL workspaces.
/// One row per (workspace, tab) running a Custom provider; Enter/click jumps
/// to that workspace+tab. Status comes from the OSC 777 channel when present.
pub(super) fn render_agents_pane(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::Agents;
    let border_style = pane_border_style(app, ActivePane::Agents);
    let theme = &app.theme.file_list;

    let block = Block::default()
        .title(" AGENTS ")
        .title_style(pane_title_style(app, ActivePane::Agents))
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    let rows = app.agent_rows();
    if rows.is_empty() {
        let hint = format!(
            "  No agents running\n  [{}] new agent tab",
            app.config.get_binding("app", "new_tab")
        );
        let text = Paragraph::new(hint)
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let selected = app.selected_agent_row.min(rows.len() - 1);
    let visible_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = if selected >= visible_height {
        selected + 1 - visible_height
    } else {
        0
    };

    let items: Vec<ListItem> = rows
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(vis_idx, &(wi, ti))| {
            let row_idx = vis_idx + scroll_offset;
            let ws = &app.workspaces[wi];
            let tab = &ws.tabs[ti];

            let (glyph, status_label, status_color) = match tab.cli_agent_snapshot() {
                Some((status, attention, _)) => {
                    crate::ui::cli_agent_status_view(app, status, attention)
                }
                None => crate::ui::agent_tab_indicator(app, tab),
            };
            // A non-Custom tab only lists here because its cli-agent channel
            // reported — a `claude` run manually inside that tab.
            let label = if matches!(tab.provider, piki_core::AIProvider::Custom(_)) {
                tab.provider.label().to_string()
            } else {
                format!("Claude ({})", tab.provider.label())
            };

            // Selection cools to a neutral surface when the pane loses focus
            // but never disappears.
            let row_bg = if row_idx == selected {
                Style::default().bg(if is_active {
                    theme.selected_bg
                } else {
                    app.theme.palette.bg2
                })
            } else {
                Style::default()
            };
            let mut spans = vec![
                Span::styled(format!(" {glyph} "), row_bg.fg(status_color)),
                Span::styled(ws.name.clone(), row_bg.fg(theme.file_path)),
                Span::styled(" · ", row_bg.fg(theme.empty_text)),
                Span::styled(label, row_bg.fg(theme.file_path)),
                Span::styled(format!(" {status_label}"), row_bg.fg(status_color)),
            ];
            if ws.has_idle_notification {
                spans.push(Span::styled(" ●", row_bg.fg(app.theme.status.needs_you)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);

    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        rows.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
}
