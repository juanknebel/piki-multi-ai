use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::DialogState;

pub(crate) fn render_dashboard_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (selected, scroll_offset) = match app.active_dialog {
        Some(DialogState::Dashboard {
            selected,
            scroll_offset,
        }) => (selected, scroll_offset),
        _ => (0, 0),
    };

    let theme = &app.theme;
    let width = area.width * 85 / 100;
    let height = area.height * 80 / 100;
    let popup = super::clear_popup(frame, area, width.max(40), height.max(10));

    let inner_height = popup.height.saturating_sub(2); // borders
    let narrow = popup.width < 70;

    // Footer hint = 1 line
    let visible_rows = inner_height.saturating_sub(1) as usize;

    // Build all visual lines, then slice by scroll_offset
    let mut body_lines: Vec<Line<'_>> = Vec::new();

    let total = app.workspaces.len();
    for i in 0..total {
        let ws = &app.workspaces[i];
        let is_active = i == app.active_workspace;
        let is_selected = i == selected;

        // Marker
        let marker = if is_active { "▸ " } else { "  " };

        // Name
        let name: String = ws.name.chars().take(24).collect();

        // Status
        let status_label = ws.status_label();
        let status_color = match &ws.status {
            piki_core::WorkspaceStatus::Idle => Color::DarkGray,
            piki_core::WorkspaceStatus::Busy => Color::Yellow,
            piki_core::WorkspaceStatus::Done => Color::Green,
            piki_core::WorkspaceStatus::Error(_) => Color::Red,
        };

        // Row style
        let base_fg = if is_active {
            theme.workspace_list.name_active
        } else {
            Color::White
        };
        let bg = if is_selected {
            theme.workspace_list.selected_bg
        } else {
            Color::Reset
        };
        let row_style = Style::default().fg(base_fg).bg(bg);
        let status_style = Style::default().fg(status_color).bg(bg);
        let muted = Style::default().fg(Color::DarkGray).bg(bg);

        // Workspace header line
        if narrow {
            let files = ws.file_count();
            body_lines.push(Line::from(vec![
                Span::styled(marker, row_style),
                Span::styled(name, row_style.add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {} ", status_label), status_style),
                Span::styled(format!("{}f", files), muted),
            ]));
        } else {
            let branch: String = ws.branch.chars().take(20).collect();
            let files = ws.file_count();
            let ab = match ws.ahead_behind {
                Some((ahead, behind)) if ahead > 0 || behind > 0 => {
                    format!(" ↑{} ↓{}", ahead, behind)
                }
                _ => String::new(),
            };
            body_lines.push(Line::from(vec![
                Span::styled(marker, row_style),
                Span::styled(
                    format!("{:<24}", name),
                    row_style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(branch, muted),
                Span::styled(format!("  {} ", status_label), status_style),
                Span::styled(format!("{}f", files), muted),
                Span::styled(ab, muted),
            ]));
        }

        // Tab lines (indented under workspace)
        if ws.tabs.is_empty() {
            body_lines.push(Line::from(Span::styled(
                "     (no tabs)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (ti, tab) in ws.tabs.iter().enumerate() {
                let label = tab
                    .markdown_label
                    .as_deref()
                    .unwrap_or(tab.provider.label());

                let alive = tab.pty_session.as_ref().is_some_and(|p| p.peek_alive());
                let (indicator, ind_color) = if tab.markdown_content.is_some() {
                    ("md", Color::Cyan)
                } else if alive {
                    ("●", Color::Green)
                } else if tab.pty_session.is_some() {
                    ("○", Color::DarkGray)
                } else {
                    ("—", Color::DarkGray)
                };

                let is_active_tab = ti == ws.active_tab;
                let tab_fg = if is_active_tab {
                    Color::White
                } else {
                    Color::DarkGray
                };
                let arrow = if is_active_tab { "→ " } else { "  " };

                body_lines.push(Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(arrow, Style::default().fg(tab_fg)),
                    Span::styled(label, Style::default().fg(tab_fg)),
                    Span::raw(" "),
                    Span::styled(indicator, Style::default().fg(ind_color)),
                ]));
            }
        }
    }

    let total_lines = body_lines.len();
    let scroll = scroll_offset.min(total_lines.saturating_sub(visible_rows));
    let end = total_lines.min(scroll + visible_rows);

    let mut lines: Vec<Line<'_>> = Vec::new();
    for line in body_lines.into_iter().skip(scroll).take(end - scroll) {
        lines.push(line);
    }

    // Footer hint
    lines.push(Line::from(Span::styled(
        " j/k navigate  Enter switch  Esc close",
        Style::default().fg(Color::DarkGray),
    )));

    // Scroll indicator in title
    let scroll_indicator = if total > 0 {
        format!(" [{}/{}] ", selected + 1, total)
    } else {
        String::new()
    };

    let block = super::popup_block("Dashboard", theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}
