use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Clear;

use crate::app::App;

/// Render the global scratch-terminal overlay: a centered floating box
/// holding the live shell, drawn on top of everything else. No-op unless
/// `app.scratch` has a parser (i.e. its shell spawned).
pub(crate) fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(parser) = app.scratch.pty_parser.clone() else {
        app.scratch_inner_area = None;
        return;
    };

    let width = (area.width * 85 / 100).max(40).min(area.width);
    let height = (area.height * 75 / 100).max(10).min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup = Rect::new(x, y, width, height);

    // Inner area (inside the 1-col rounded border) — the mouse hit-test box
    // and the size the shell should render at.
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    app.scratch_inner_area = Some(inner);

    let inner_rows = inner.height.max(1);
    let inner_cols = inner.width.max(1);
    let cur = {
        let g = parser.lock();
        (g.screen().size().0, g.screen().size().1)
    };
    if cur != (inner_rows, inner_cols)
        && let Some(ref pty) = app.scratch.pty_session
    {
        let _ = pty.resize(inner_rows, inner_cols);
    }

    frame.render_widget(Clear, popup);

    let border = Style::default().fg(app.theme.border.active);
    let title_style = border.add_modifier(Modifier::BOLD);
    let selection_style = Style::default()
        .bg(app.theme.selection.bg)
        .fg(app.theme.selection.fg);
    let hide = app
        .config
        .prefix_chord("scratch_terminal")
        .map(|c| format!("{} {c}", app.config.prefix_display()))
        .unwrap_or_default();

    super::terminal::render(
        frame,
        popup,
        &parser,
        border,
        title_style,
        &format!("Terminal · ~   ({hide} to hide)"),
        app.scratch.term_scroll,
        app.scratch.selection.as_ref(),
        selection_style,
        None,
        app.theme.general.scrollbar_thumb,
        &app.theme.palette,
    );
}
