use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, SidebarItem};
use piki_core::WorkspaceType;

use super::layout::pane_border_style;

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
fn sidebar_item_height(items: &[SidebarItem], idx: usize) -> usize {
    match &items[idx] {
        SidebarItem::GroupHeader { .. } => 1,
        SidebarItem::Workspace { .. } => {
            if idx > 0 && matches!(items[idx - 1], SidebarItem::Workspace { .. }) {
                4 // 1 separator + 3 content
            } else {
                3
            }
        }
    }
}

pub(super) fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);
    let theme = &app.theme.workspace_list;

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(border_style)
        .borders(Borders::ALL)
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

    // Compute scroll offset for mixed-height items
    let visible_height = area.height.saturating_sub(2) as usize;
    let mut scroll_offset = 0;
    if visible_height > 0 {
        // Sum heights up to and including selected row
        let mut height_to_selected: usize = 0;
        for i in 0..=app
            .selected_sidebar_row
            .min(sidebar_items.len().saturating_sub(1))
        {
            height_to_selected += sidebar_item_height(&sidebar_items, i);
            if i == app.selected_sidebar_row {
                break;
            }
        }
        if height_to_selected > visible_height {
            // Find first item to skip so selected fits
            let mut skip_height = height_to_selected - visible_height;
            for i in 0..sidebar_items.len() {
                let h = sidebar_item_height(&sidebar_items, i);
                if skip_height == 0 {
                    break;
                }
                scroll_offset = i + 1;
                skip_height = skip_height.saturating_sub(h);
            }
        }
    }

    // Build the separator line (full inner width of '─' chars)
    let inner_width = area.width.saturating_sub(2) as usize;
    let separator_str: String = "─".repeat(inner_width);

    // Track workspace visual index for alternating backgrounds
    let mut ws_visual_idx: usize = 0;

    let mut total_lines = 0;
    let items: Vec<ListItem> = sidebar_items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .filter(|(i, _)| {
            let h = sidebar_item_height(&sidebar_items, *i);
            if total_lines + h > visible_height {
                return false;
            }
            total_lines += h;
            true
        })
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
                        Style::default().bg(theme.selected_bg)
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

                    let mut line1_spans = vec![
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
                        line1_spans.push(Span::styled(
                            " ●",
                            Style::default()
                                .fg(ratatui::style::Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                    let line1 = Line::from(line1_spans);
                    let count_label = format!("{} files", ws.file_count());
                    let line2 = Line::from(vec![
                        Span::raw("   "),
                        Span::styled(count_label, Style::default().fg(detail_color)),
                    ]);

                    // Line 3: description if available, otherwise branch name
                    let detail_text = if ws.info.description.is_empty() {
                        format!("⎇ {}", ws.info.branch)
                    } else {
                        ws.info.description.clone()
                    };
                    let max_len = area.width.saturating_sub(6) as usize;
                    let truncated = if detail_text.len() > max_len {
                        format!("{}…", &detail_text[..max_len.saturating_sub(1)])
                    } else {
                        detail_text
                    };
                    let line3 = Line::from(vec![
                        Span::raw("   "),
                        Span::styled(truncated, Style::default().fg(detail_color)),
                    ]);

                    // Build lines: optionally prepend separator between consecutive workspaces
                    let mut lines = Vec::new();
                    let has_separator =
                        row > 0 && matches!(sidebar_items[row - 1], SidebarItem::Workspace { .. });
                    if has_separator {
                        lines.push(Line::from(Span::styled(
                            separator_str.as_str(),
                            Style::default().fg(theme.separator),
                        )));
                    }
                    lines.push(line1);
                    lines.push(line2);
                    lines.push(line3);

                    let style = if is_selected {
                        Style::default().bg(theme.selected_bg)
                    } else if current_ws_idx % 2 == 1 {
                        Style::default().bg(theme.alt_bg)
                    } else {
                        Style::default()
                    };

                    ListItem::new(lines).style(style)
                }
            }
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);

    let total_visual_height: usize = (0..sidebar_items.len())
        .map(|i| sidebar_item_height(&sidebar_items, i))
        .sum();
    let scroll_pos: usize = (0..scroll_offset)
        .map(|i| sidebar_item_height(&sidebar_items, i))
        .sum();
    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_pos,
        total_visual_height,
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
        .title_style(border_style)
        .borders(Borders::ALL)
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
                Some((status, _)) => crate::ui::cli_agent_status_view(status),
                None => crate::ui::agent_tab_indicator(tab),
            };

            let row_bg = if is_active && row_idx == selected {
                Style::default().bg(theme.selected_bg)
            } else {
                Style::default()
            };
            let mut spans = vec![
                Span::styled(format!(" {glyph} "), row_bg.fg(status_color)),
                Span::styled(ws.name.clone(), row_bg.fg(theme.file_path)),
                Span::styled(" · ", row_bg.fg(theme.empty_text)),
                Span::styled(tab.provider.label().to_string(), row_bg.fg(theme.file_path)),
                Span::styled(format!(" {status_label}"), row_bg.fg(status_color)),
            ];
            if ws.has_idle_notification {
                spans.push(Span::styled(" ●", row_bg.fg(ratatui::style::Color::Yellow)));
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
