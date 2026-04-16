use parking_lot::Mutex;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use tui_term::widget::{Cursor as PtyCursor, PseudoTerminal};

use crate::app::{Selection, TermSearchState};

/// Render the PTY terminal output in the given area.
/// Locks the parser briefly to read the vt100 screen state.
/// When `scroll_offset > 0`, shows scrollback history.
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    parser: &Arc<Mutex<vt100::Parser>>,
    border_style: Style,
    title: &str,
    scroll_offset: usize,
    selection: Option<&Selection>,
    selection_style: Style,
    search: Option<&TermSearchState>,
    scrollbar_color: Color,
) {
    let mut parser_guard = parser.lock();

    // Apply scrollback offset
    parser_guard.screen_mut().set_scrollback(scroll_offset);

    // Read back the clamped value (set_scrollback clamps to actual buffer size)
    let actual_offset = parser_guard.screen().scrollback();

    let mut pseudo_term = PseudoTerminal::new(parser_guard.screen()).block(
        Block::default()
            .title(format!(" {} ", title))
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    if actual_offset > 0 {
        pseudo_term = pseudo_term.cursor(PtyCursor::default().visibility(false));
    }
    frame.render_widget(pseudo_term, area);

    // Probe max scrollback while we hold the lock
    parser_guard.screen_mut().set_scrollback(usize::MAX);
    let max_scrollback = parser_guard.screen().scrollback();
    // Reset scrollback so the parser tracks live output correctly
    parser_guard.screen_mut().set_scrollback(0);
    drop(parser_guard);

    // Scrollbar for terminal scrollback (inverted: 0=live/bottom, max=top)
    let visible_height = area.height.saturating_sub(2) as usize;
    if max_scrollback > 0 {
        let total = max_scrollback + visible_height;
        let position = max_scrollback.saturating_sub(actual_offset);
        super::scrollbar::render_vertical(
            frame,
            area,
            position,
            total,
            visible_height,
            scrollbar_color,
        );
    }

    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    // Render selection highlight overlay — iterate only the selected rows
    if let Some(sel) = selection {
        let (start_row, start_col, end_row, end_col) = sel.normalized();
        let first_row = start_row.min(inner.height.saturating_sub(1));
        let last_row = end_row.min(inner.height.saturating_sub(1));
        let buf = frame.buffer_mut();
        for row in first_row..=last_row {
            let col_start = if row == start_row { start_col } else { 0 };
            let col_end = if row == end_row {
                end_col.min(inner.width.saturating_sub(1))
            } else {
                inner.width.saturating_sub(1)
            };
            for col in col_start..=col_end {
                let x = inner.x + col;
                let y = inner.y + row;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(selection_style);
                }
            }
        }
    }

    // Render search highlights and input bar
    if let Some(search) = search {
        let query_len = search.query.chars().count();
        if query_len > 0 {
            let match_style = Style::default().bg(Color::Yellow).fg(Color::Black);
            let current_style = Style::default().bg(Color::LightRed).fg(Color::Black);
            let buf = frame.buffer_mut();
            for (idx, &(row, col)) in search.matches.iter().enumerate() {
                let style = if idx == search.current_match {
                    current_style
                } else {
                    match_style
                };
                let r = row as u16;
                if r >= inner.height {
                    continue;
                }
                for offset in 0..query_len {
                    let c = (col + offset) as u16;
                    if c >= inner.width {
                        break;
                    }
                    let x = inner.x + c;
                    let y = inner.y + r;
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(style);
                    }
                }
            }
        }

        // Search input bar at the bottom of the terminal area
        let bar_y = area.y + area.height.saturating_sub(2);
        let bar_area = Rect::new(area.x + 1, bar_y, inner.width, 1);
        frame.render_widget(Clear, bar_area);

        let match_info = if search.matches.is_empty() {
            if search.query.is_empty() {
                String::new()
            } else {
                " (no matches)".to_string()
            }
        } else {
            format!(" {}/{}", search.current_match + 1, search.matches.len())
        };

        let bar = Paragraph::new(Line::from(vec![
            Span::styled(" / ", Style::default().fg(Color::Yellow)),
            Span::raw(&search.query),
            Span::styled(match_info, Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(bar, bar_area);
    }
}
