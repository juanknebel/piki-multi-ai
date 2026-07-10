use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::Workspace;
use crate::theme::Theme;

/// Render the tab bar as herdr-style solid blocks: the active tab gets the
/// accent background, inactive tabs a subtle one, one-space gaps in between
/// and a trailing `+` button. Per-tab column layout must stay in sync with
/// `helpers::subtab_index_at`.
pub fn render(frame: &mut Frame, area: Rect, ws: &Workspace, theme: &Theme) {
    let mut spans: Vec<Span> = Vec::new();

    for (i, tab) in ws.tabs.iter().enumerate() {
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
                .fg(theme.subtabs.active_fg)
                .bg(theme.subtabs.active)
        } else {
            Style::default()
                .fg(theme.subtabs.inactive)
                .bg(theme.subtabs.inactive_bg)
        };

        spans.push(Span::styled(format!(" {} ", icon), base_style));
        spans.push(Span::styled(
            label.to_string(),
            if is_active {
                base_style.add_modifier(Modifier::BOLD)
            } else {
                base_style
            },
        ));
        if let Some((status, attention, _)) = tab.cli_agent_snapshot()
            && let Some((glyph, color)) = crate::ui::actionable_status_view(theme, status, attention)
        {
            // Only actionable states reach the tab bar (activity stays in the
            // Agents pane); the glyph keeps its semantic color even on the
            // active accent block.
            spans.push(Span::styled(format!(" {}", glyph), base_style.fg(color)));
        }
        if tab.closable {
            spans.push(Span::styled(" ×", base_style.add_modifier(Modifier::DIM)));
        }
        spans.push(Span::styled(" ", base_style));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        " + ",
        Style::default().fg(theme.subtabs.inactive),
    ));

    let bar = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(bar, area);
}
