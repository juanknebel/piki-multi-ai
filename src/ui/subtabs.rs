use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::AIProvider;
use crate::theme::Theme;

/// Render the AI provider sub-tabs below the workspace tabs.
pub fn render(frame: &mut Frame, area: Rect, active_provider: AIProvider, theme: &Theme) {
    let titles: Vec<Line> = AIProvider::all()
        .iter()
        .map(|p| Line::from(format!(" {} ", p.label())))
        .collect();

    let active_idx = AIProvider::all()
        .iter()
        .position(|p| *p == active_provider)
        .unwrap_or(0);

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(active_idx)
        .highlight_style(
            Style::default()
                .fg(theme.subtabs.active)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(theme.subtabs.inactive))
        .divider("|");

    frame.render_widget(tabs, area);
}
