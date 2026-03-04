use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Render the side-by-side diff view in the given area.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    content: &Option<Text<'static>>,
    scroll: u16,
    file_path: &str,
    border_style: Style,
    empty_text_color: Color,
) {
    if let Some(text) = content {
        let title = format!(" DIFF: {} ", file_path);
        let paragraph = Paragraph::new(text.clone())
            .block(
                Block::default()
                    .title(title)
                    .title_style(border_style)
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .scroll((scroll, 0));
        frame.render_widget(paragraph, area);
    } else {
        let paragraph = Paragraph::new("  Select a file and press Enter to view diff")
            .style(Style::default().fg(empty_text_color))
            .block(
                Block::default()
                    .title(" DIFF ")
                    .title_style(border_style)
                    .borders(Borders::ALL)
                    .border_style(border_style),
            );
        frame.render_widget(paragraph, area);
    }
}
