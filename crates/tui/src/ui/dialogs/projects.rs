//! Projects overlay (`prefix ctrl-p`): cross-repo groups of workspaces and
//! directories, each with a colour. Pure render — the project list lives in
//! [`DialogState::Projects`], loaded from storage at open time; the edit
//! sub-dialog renders from [`DialogState::ProjectEdit`]. Member rows resolve
//! dynamically against the registered workspace list, so a directory adopted
//! as a workspace upgrades its row with no stored state.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::{DialogState, ProjectEditField, ProjectRow, project_rows};
use piki_core::projects::PROJECT_PALETTE_LEN;

pub(crate) fn render_projects_overlay(frame: &mut Frame, area: Rect, app: &App) {
    match &app.active_dialog {
        Some(DialogState::Projects { .. }) => render_projects_list(frame, area, app),
        Some(DialogState::ProjectEdit { .. }) => render_project_edit(frame, area, app),
        _ => {}
    }
}

fn render_projects_list(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::Projects {
        ref projects,
        selected,
        ref expanded,
        scroll_offset,
    }) = app.active_dialog
    else {
        return;
    };

    let theme = &app.theme;
    let width = area.width * 80 / 100;
    let height = area.height * 70 / 100;
    let popup = super::clear_popup(frame, area, width.max(50), height.max(9));

    let inner_height = popup.height.saturating_sub(2) as usize; // borders
    let visible_rows = inner_height.saturating_sub(2).max(1); // hints + blank

    let muted = Style::default().fg(theme.palette.fg3);
    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            " [Enter] expand/jump [n] new [e] edit [d] delete [Esc] close",
            muted,
        )),
        Line::from(""),
    ];

    let rows = project_rows(projects, expanded);
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            " no projects — press n to create one",
            muted,
        )));
    } else {
        let mut rendered: Vec<Line<'_>> = Vec::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            let bg = if i == selected {
                theme.workspace_list.selected_bg
            } else {
                Color::Reset
            };
            match *row {
                ProjectRow::Project(pi) => {
                    let p = &projects[pi];
                    let open = p.id.is_some_and(|id| expanded.contains(&id));
                    let marker = if open { "▾" } else { "▸" };
                    let count = p.members.len();
                    let count_text = if count == 1 {
                        "1 member".to_string()
                    } else {
                        format!("{count} members")
                    };
                    rendered.push(Line::from(vec![
                        Span::styled(
                            format!(" {marker} "),
                            Style::default().fg(theme.palette.fg2).bg(bg),
                        ),
                        Span::styled(
                            "● ",
                            Style::default()
                                .fg(theme.projects.color(p.clamped_color()))
                                .bg(bg),
                        ),
                        Span::styled(
                            format!("{:<28}", p.name),
                            Style::default()
                                .fg(theme.palette.fg0)
                                .bg(bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(count_text, Style::default().fg(theme.palette.fg3).bg(bg)),
                    ]));
                }
                ProjectRow::Member(pi, mi) => {
                    let path = &projects[pi].members[mi].path;
                    // Resolve dynamically: a registered workspace renders by
                    // name (Enter jumps); anything else is a plain directory,
                    // shown dimmed by path (Enter adopts it).
                    let ws = app.workspaces.iter().find(|w| w.info.path == *path);
                    let (text, style) = match ws {
                        Some(w) => (
                            w.info.name.clone(),
                            Style::default().fg(theme.palette.fg1).bg(bg),
                        ),
                        None => (
                            path.to_string_lossy().into_owned(),
                            Style::default().fg(theme.palette.fg3).bg(bg),
                        ),
                    };
                    rendered.push(Line::from(vec![
                        Span::styled("     ", Style::default().bg(bg)),
                        Span::styled(text, style),
                    ]));
                }
            }
        }
        // Auto-scroll: start from the handler's offset but always keep the
        // selected row inside the window (mandatory for unbounded lists).
        let mut scroll = scroll_offset;
        if selected < scroll {
            scroll = selected;
        }
        if selected >= scroll + visible_rows {
            scroll = selected + 1 - visible_rows;
        }
        scroll = scroll.min(rendered.len().saturating_sub(visible_rows));
        lines.extend(rendered.into_iter().skip(scroll).take(visible_rows));
    }

    let counter = if rows.is_empty() {
        String::new()
    } else {
        format!(" [{}/{}] ", selected + 1, rows.len())
    };
    let block = super::popup_block("Projects", theme.help.border)
        .title_bottom(Line::from(counter).right_aligned());
    frame.render_widget(Paragraph::new(lines).block(block), popup);
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
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {cursor} {check} "),
                    field(ProjectEditField::Members),
                ),
                Span::styled(row.label.clone(), label_style),
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
