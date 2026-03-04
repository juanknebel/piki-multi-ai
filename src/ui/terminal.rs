use std::sync::{Arc, Mutex};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use tui_term::widget::PseudoTerminal;

/// Render the PTY terminal output in the given area.
/// Locks the parser briefly to read the vt100 screen state.
pub fn render(frame: &mut Frame, area: Rect, parser: &Arc<Mutex<vt100::Parser>>, border_style: Style, title: &str) {
    let parser_guard = parser.lock().unwrap();
    let pseudo_term = PseudoTerminal::new(parser_guard.screen())
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .title_style(border_style)
                .borders(Borders::ALL)
                .border_style(border_style),
        );
    frame.render_widget(pseudo_term, area);
}
