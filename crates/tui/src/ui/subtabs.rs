use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::Workspace;
use crate::theme::Theme;

/// Render the dynamic sub-tabs below the workspace tabs.
pub fn render(frame: &mut Frame, area: Rect, ws: &Workspace, theme: &Theme) {
    let titles: Vec<Line> = ws
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let label = tab
                .markdown_label
                .as_deref()
                .unwrap_or(tab.provider.label());
            let icon = if tab.markdown_content.is_some() {
                "◆"
            } else {
                tab.provider.icon()
            };
            let is_active = i == ws.active_tab;
            let base_style = if is_active {
                Style::default()
                    .fg(theme.subtabs.active)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(theme.subtabs.inactive)
            };

            let mut spans = vec![
                Span::styled(format!(" {} ", icon), base_style),
                Span::styled(label, base_style),
            ];
            if tab.closable {
                spans.push(Span::styled(
                    " ×",
                    Style::default().fg(theme.subtabs.close_marker),
                ));
            }
            spans.push(Span::raw(" "));
            Line::from(spans)
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(ws.active_tab)
        .style(Style::default())
        .divider(Span::styled(
            "│",
            Style::default().fg(theme.subtabs.divider),
        ));

    frame.render_widget(tabs, area);
}
