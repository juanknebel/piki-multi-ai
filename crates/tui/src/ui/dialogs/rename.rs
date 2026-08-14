use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::DialogState;
use ratatui::Frame;
use ratatui::layout::Rect;

pub(crate) fn render_rename_tab_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::RenameTab { ref input, cursor }) = app.active_dialog else {
        return;
    };

    let popup = super::clear_popup(frame, area, 60, 7);
    let border = app.theme.dialog.new_ws_active;
    let block = super::popup_block(" Rename Tab ", border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let input_width = inner.width.saturating_sub(4) as usize;
    let visible = super::visible_field(input, true, cursor, input_width);

    let label_style = Style::default().fg(app.theme.dialog.new_ws_inactive);
    let input_style = Style::default().fg(app.theme.dialog.new_ws_active);

    let lines = vec![
        Line::from(Span::styled("New title (empty to clear):", label_style)),
        Line::from(Span::styled(visible, input_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(app.theme.footer.key)),
            Span::styled(" save  ", Style::default().fg(app.theme.footer.description)),
            Span::styled("Esc", Style::default().fg(app.theme.footer.key)),
            Span::styled(" cancel", Style::default().fg(app.theme.footer.description)),
        ]),
    ];
    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}
