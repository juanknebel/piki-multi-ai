use std::sync::Arc;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

/// A widget that renders borrowed `Text` without cloning, avoiding expensive
/// deep copies of `Vec<Line<Vec<Span<String>>>>` on every frame.
struct BorrowedDiff<'a> {
    text: &'a Text<'static>,
    block: Block<'a>,
    scroll: u16,
}

impl Widget for BorrowedDiff<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = self.block.inner(area);
        self.block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let lines = &self.text.lines;
        let skip = self.scroll as usize;
        for (row_offset, line) in lines
            .iter()
            .skip(skip)
            .take(inner.height as usize)
            .enumerate()
        {
            let y = inner.y + row_offset as u16;
            let mut x = inner.x;
            for span in &line.spans {
                let available = inner.width.saturating_sub(x - inner.x) as usize;
                if available == 0 {
                    break;
                }
                let content = span.content.as_ref();
                // Truncate at a char boundary to avoid panicking on
                // multi-byte UTF-8 (e.g. box-drawing '─' is 3 bytes).
                let (byte_end, char_count) = content
                    .char_indices()
                    .take(available)
                    .fold((0, 0u16), |(_, count), (i, c)| {
                        (i + c.len_utf8(), count + 1)
                    });
                let display = &content[..byte_end];
                buf.set_string(x, y, display, span.style);
                x += char_count;
            }
        }
    }
}

/// Render the side-by-side diff view in the given area.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    content: &Option<Arc<Text<'static>>>,
    scroll: u16,
    file_path: &str,
    border_style: Style,
    empty_text_color: Color,
) {
    if let Some(text) = content {
        let title = format!(" DIFF: {} ", file_path);
        let widget = BorrowedDiff {
            text,
            block: Block::default()
                .title(title)
                .title_style(border_style)
                .borders(Borders::ALL)
                .border_style(border_style),
            scroll,
        };
        frame.render_widget(widget, area);
        let visible_height = area.height.saturating_sub(2) as usize;
        super::scrollbar::render_vertical(
            frame,
            area,
            scroll as usize,
            text.lines.len(),
            visible_height,
            Color::DarkGray,
        );
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
