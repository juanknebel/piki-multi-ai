use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
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
            Line::from(format!(" {}{} ", tab.provider.label(), close_marker))
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
