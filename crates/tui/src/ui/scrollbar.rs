use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

/// Render a thin vertical scrollbar on the right edge of `area`.
/// Only renders when content overflows the viewport.
pub fn render_vertical(
    frame: &mut Frame,
    area: Rect,
    position: usize,
    total_content: usize,
    visible_height: usize,
    thumb_color: Color,
) {
    if total_content <= visible_height || visible_height == 0 {
        return;
    }

    let mut state = ScrollbarState::new(total_content)
        .position(position)
        .viewport_content_length(visible_height);

    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol("▐")
            .thumb_style(Style::default().fg(thumb_color)),
        area,
        &mut state,
    );
}
