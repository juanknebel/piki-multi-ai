use std::sync::{Arc, Mutex};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use tui_term::widget::{Cursor as PtyCursor, PseudoTerminal};

use crate::app::Selection;

/// Render the PTY terminal output in the given area.
/// Locks the parser briefly to read the vt100 screen state.
/// When `scroll_offset > 0`, shows scrollback history.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    parser: &Arc<Mutex<vt100::Parser>>,
    border_style: Style,
    title: &str,
    scroll_offset: usize,
    selection: Option<&Selection>,
    selection_style: Style,
) {
    let mut parser_guard = parser.lock().unwrap();

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

    // Reset scrollback so the parser tracks live output correctly
    parser_guard.screen_mut().set_scrollback(0);
    drop(parser_guard);

    // Render selection highlight overlay
    if let Some(sel) = selection {
        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        let buf = frame.buffer_mut();
        for row in 0..inner.height {
            for col in 0..inner.width {
                if sel.contains(row, col) {
                    let x = inner.x + col;
                    let y = inner.y + row;
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(selection_style);
                    }
                }
            }
        }
    }
}
