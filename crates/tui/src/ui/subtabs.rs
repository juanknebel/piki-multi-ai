use std::time::SystemTime;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::Workspace;
use crate::theme::Theme;

/// Render the dynamic sub-tabs below the workspace tabs.
pub fn render(frame: &mut Frame, area: Rect, ws: &Workspace, theme: &Theme) {
    let titles: Vec<Line> = ws
        .tabs
        .iter()
        .map(|tab| {
            let close_marker = if tab.closable { " ×" } else { "" };
            let label = tab
                .markdown_label
                .as_deref()
                .unwrap_or(tab.provider.label());
            
            let mut style = Style::default();
            if tab.pomodoro_state.as_ref().is_some_and(|s| s.alert) {
                let millis = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                if (millis / 500) % 2 == 0 {
                    style = style.bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD);
                } else {
                    style = style.bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD);
                }
            }

            Line::from(Span::styled(format!(" {}{} ", label, close_marker), style))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(ws.active_tab)
        .highlight_style(
            Style::default()
                .fg(theme.subtabs.active)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(theme.subtabs.inactive))
        .divider("|");

    frame.render_widget(tabs, area);
}
