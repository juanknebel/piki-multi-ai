use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::DialogState;

/// Destination picker for moving a tab to another workspace. The list is
/// every workspace but the current one, so it can be long: the popup is
/// clamped to the screen and auto-scrolls to keep the selection in view.
pub(crate) fn render_move_tab_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::MoveTab {
        tab,
        ref targets,
        selected,
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 60 / 100;
    let max_popup_height = area.height.saturating_sub(4).max(10);
    let visible_rows = targets.len().clamp(1, 10) as u16;
    let height = (6u16.saturating_add(visible_rows)).min(max_popup_height);
    let popup = super::clear_popup(frame, area, popup_width.max(40), height);
    let theme = &app.theme.dialog;
    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;

    let tab_label = app
        .current_workspace()
        .and_then(|ws| ws.tabs.get(tab))
        .map(|t| t.display_label().to_string())
        .unwrap_or_else(|| "tab".to_string());

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(vec![
            Span::styled("  Move ", Style::default().fg(inactive_c)),
            Span::styled(tab_label, Style::default().fg(active_c)),
            Span::styled(" to:", Style::default().fg(inactive_c)),
        ]),
        Line::from(""),
    ];
    let mut selected_line_idx = 0usize;
    for (row, ws_idx) in targets.iter().enumerate() {
        let Some(ws) = app.workspaces.get(*ws_idx) else {
            continue;
        };
        let is_selected = row == selected;
        if is_selected {
            selected_line_idx = lines.len();
        }
        let prefix = if is_selected { "  > " } else { "    " };
        let style = if is_selected {
            Style::default().fg(active_c).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(inactive_c)
        };
        let branch = ws
            .branch
            .as_deref()
            .map(|b| format!(" ⎇ {b}"))
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("{prefix}{}{branch}", ws.info.name),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  [j/k] move  [Enter] move tab  [Esc] cancel",
        Style::default().fg(inactive_c),
    )]));

    let mut block = super::popup_block("Move Tab to Workspace", theme.new_ws_border);

    let total_lines = lines.len() as u16;
    let inner_height = popup.height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = (selected_line_idx as u16)
        .saturating_sub(inner_height.saturating_sub(1))
        .min(max_scroll);
    if max_scroll > 0 {
        block = block.title_bottom(
            Line::from(format!(" [{}/{}] ", selected + 1, targets.len())).right_aligned(),
        );
    }

    let text = Paragraph::new(lines).block(block).scroll((scroll, 0));
    frame.render_widget(text, popup);
}
