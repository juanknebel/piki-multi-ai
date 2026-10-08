//! The project editor dialog, opened from the sidebar tree: name, colour and
//! a member checklist. Pure render — its state lives in
//! [`DialogState::ProjectEdit`], built at open time. The project *list* is the
//! sidebar tree itself (`ui/sidebar.rs`), not a dialog.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::{DialogState, ProjectEditField};
use piki_core::projects::PROJECT_PALETTE_LEN;

pub(crate) fn render_projects_overlay(frame: &mut Frame, area: Rect, app: &App) {
    if matches!(app.active_dialog, Some(DialogState::ProjectEdit { .. })) {
        render_project_edit(frame, area, app);
    }
}

fn render_project_edit(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::ProjectEdit {
        ref editing_id,
        ref name,
        name_cursor,
        color,
        ref members,
        member_cursor,
        active_field,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let theme = &app.theme;
    let active_c = theme.dialog.new_ws_active;
    let inactive_c = theme.dialog.new_ws_inactive;

    // Fixed chrome: hint + blank + name + color + members header (5 lines)
    // plus borders (2); the member checklist gets whatever height is left,
    // clamped to the screen, and auto-scrolls around its cursor.
    let width = (area.width * 70 / 100).clamp(46, 76);
    let max_height = area.height.saturating_sub(2).max(9);
    let height = (7 + members.len().max(1) as u16).min(max_height);
    let popup = super::clear_popup(frame, area, width, height);

    let field = |f: ProjectEditField| super::field_style(active_field == f, active_c, inactive_c);

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            " [Tab] fields [←/→] colour [Space] toggle [Enter] save [Esc] back",
            Style::default().fg(theme.palette.fg3),
        )),
        Line::from(""),
    ];

    // Name
    let fmax = (popup.width as usize).saturating_sub(12);
    lines.push(super::render_text_field(
        "  Name:  ",
        name,
        active_field == ProjectEditField::Name,
        name_cursor,
        fmax,
        field(ProjectEditField::Name),
    ));

    // Color: the ten palette dots, the selected one bracketed.
    let mut color_spans: Vec<Span<'_>> =
        vec![Span::styled("  Color: ", field(ProjectEditField::Color))];
    for i in 0..PROJECT_PALETTE_LEN {
        let (open, close) = if i == color { ("[", "]") } else { (" ", " ") };
        color_spans.push(Span::styled(open, field(ProjectEditField::Color)));
        color_spans.push(Span::styled(
            "●",
            Style::default().fg(theme.projects.color(i)),
        ));
        color_spans.push(Span::styled(close, field(ProjectEditField::Color)));
    }
    lines.push(Line::from(color_spans));

    // Members: a checklist of the saved members plus every registered
    // workspace. No "add directory" input here (v1) — directories get added
    // from the desktop side, or by adopting one from the list overlay.
    lines.push(Line::from(Span::styled(
        "  Members:",
        field(ProjectEditField::Members),
    )));
    if members.is_empty() {
        lines.push(Line::from(Span::styled(
            "    (no workspaces registered)",
            Style::default().fg(theme.palette.fg3),
        )));
    } else {
        let members_active = active_field == ProjectEditField::Members;
        // Checklist rows that fit: inner height minus the fixed chrome
        // (hint + blank + name + color + members header), minus one more for
        // the [n/total] indicator when the list overflows.
        let avail = (popup.height.saturating_sub(2) as usize).saturating_sub(5);
        let overflow = members.len() > avail;
        let visible = if overflow {
            avail.saturating_sub(1).max(1)
        } else {
            avail.max(1)
        };
        let scroll = member_cursor
            .saturating_sub(visible.saturating_sub(1))
            .min(members.len().saturating_sub(visible));
        for (i, row) in members.iter().enumerate().skip(scroll).take(visible) {
            let cursor = if members_active && i == member_cursor {
                ">"
            } else {
                " "
            };
            let check = if row.checked { "[x]" } else { "[ ]" };
            let label_style = if row.is_workspace {
                Style::default().fg(theme.palette.fg1)
            } else {
                // Directory member: dimmed, kept so it can be unchecked.
                Style::default().fg(theme.palette.fg3)
            };
            // "  > [x] " prefix is 8 columns; fit the label to what's left.
            let avail = (popup.width as usize).saturating_sub(2 + 8);
            let label = if row.is_workspace {
                ellipsize_end(&row.label, avail)
            } else {
                ellipsize_start(&row.label, avail)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {cursor} {check} "),
                    field(ProjectEditField::Members),
                ),
                Span::styled(label, label_style),
            ]));
        }
        if overflow {
            // Overflow indicator mirrors the list overlay's [n/total].
            lines.push(Line::from(Span::styled(
                format!("    [{}/{}]", member_cursor + 1, members.len()),
                Style::default().fg(theme.palette.fg3),
            )));
        }
    }

    let title = if editing_id.is_some() {
        "Edit Project"
    } else {
        "New Project"
    };
    let block = super::popup_block(title, theme.dialog.new_ws_border);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

/// `s` capped to `max` columns (char-count approximation, as elsewhere in
/// the dialogs) with a trailing ellipsis — for names, whose head matters.
pub(crate) fn ellipsize_end(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Same cap but keeping the tail — for paths, whose last segments are the
/// distinctive part.
pub(crate) fn ellipsize_start(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let tail: String = s.chars().skip(count + 1 - max.max(1)).collect();
    format!("…{tail}")
}
