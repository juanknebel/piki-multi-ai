use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, DialogField, WorkspaceType};
use crate::dialog_state::DialogState;

pub(crate) fn render_new_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::NewWorkspace {
        ref name,
        name_cursor,
        ref dir,
        dir_cursor,
        ref desc,
        desc_cursor,
        ref prompt,
        prompt_cursor,
        ref kanban,
        kanban_cursor,
        ref group,
        group_cursor,
        ws_type,
        active_field,
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let hide_name = ws_type != WorkspaceType::Worktree;
    let popup_height = if hide_name { 17 } else { 19 };
    let popup = super::clear_popup(frame, area, popup_width.max(40), popup_height);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let type_active = active_field == DialogField::Type;
    let dir_active = active_field == DialogField::Directory;
    let desc_active = active_field == DialogField::Description;
    let prompt_active = active_field == DialogField::Prompt;
    let kanban_active = active_field == DialogField::KanbanPath;
    let group_active = active_field == DialogField::Group;

    let type_text = match ws_type {
        WorkspaceType::Simple => "[Simple]  Worktree   Project",
        WorkspaceType::Worktree => " Simple  [Worktree]  Project",
        WorkspaceType::Project => " Simple   Worktree  [Project]",
    };

    let mut lines: Vec<Line<'_>> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("  Type:   ", super::field_style(type_active, active_c, inactive_c)),
        Span::styled(type_text, super::field_style(type_active, active_c, inactive_c)),
    ]));
    lines.push(Line::from(""));

    if !hide_name {
        let name_active = active_field == DialogField::Name;
        lines.push(super::render_text_field(
            "  Name:   ",
            name,
            name_active,
            name_cursor,
            fmax,
            super::field_style(name_active, active_c, inactive_c),
        ));
        lines.push(Line::from(""));
    }

    lines.push(super::render_text_field(
        "  Dir:    ",
        dir,
        dir_active,
        dir_cursor,
        fmax,
        super::field_style(dir_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(super::render_text_field(
        "  Desc:   ",
        desc,
        desc_active,
        desc_cursor,
        fmax,
        super::field_style(desc_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(super::render_text_field(
        "  Prompt: ",
        prompt,
        prompt_active,
        prompt_cursor,
        fmax,
        super::field_style(prompt_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(super::render_text_field(
        "  Kanban: ",
        kanban,
        kanban_active,
        kanban_cursor,
        fmax,
        super::field_style(kanban_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(super::render_text_field(
        "  Group:  ",
        group,
        group_active,
        group_cursor,
        fmax,
        super::field_style(group_active, active_c, inactive_c),
    ));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![Span::styled(
        "  [Esc] Cancel",
        Style::default().fg(theme.new_ws_inactive),
    )]));

    let text = Paragraph::new(lines).block(super::popup_block("New Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_edit_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::EditWorkspace {
        ref kanban,
        kanban_cursor,
        ref prompt,
        prompt_cursor,
        ref group,
        group_cursor,
        active_field,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let popup = super::clear_popup(frame, area, popup_width.max(40), 13);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let kanban_active = active_field == DialogField::KanbanPath;
    let prompt_active = active_field == DialogField::Prompt;
    let group_active = active_field == DialogField::Group;

    let lines = vec![
        super::render_text_field(
            "  Kanban: ",
            kanban,
            kanban_active,
            kanban_cursor,
            fmax,
            super::field_style(kanban_active, active_c, inactive_c),
        ),
        Line::from(""),
        super::render_text_field(
            "  Prompt: ",
            prompt,
            prompt_active,
            prompt_cursor,
            fmax,
            super::field_style(prompt_active, active_c, inactive_c),
        ),
        Line::from(""),
        super::render_text_field(
            "  Group:  ",
            group,
            group_active,
            group_cursor,
            fmax,
            super::field_style(group_active, active_c, inactive_c),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ];

    let text = Paragraph::new(lines).block(super::popup_block("Edit Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_workspace_info_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let info_hscroll = match app.active_dialog {
        Some(DialogState::WorkspaceInfo { hscroll }) => hscroll,
        _ => 0,
    };

    let theme = &app.theme;
    let ws = &app.workspaces[app.selected_workspace];

    let label_style = Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(theme.help.border);
    let project_name = ws
        .source_repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ws.source_repo.to_string_lossy().to_string());

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" Branch:  ", label_style),
            Span::raw(ws.branch.clone()),
        ]),
        Line::from(vec![
            Span::styled(" Project: ", label_style),
            Span::raw(project_name),
        ]),
        Line::from(vec![
            Span::styled("          ", label_style),
            Span::raw(ws.source_repo.to_string_lossy().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Path:    ", label_style),
            Span::raw(ws.path.to_string_lossy().to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Kanban:  ", label_style),
            Span::raw(
                ws.kanban_path
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Type:    ", label_style),
            Span::raw(match ws.info.workspace_type {
                WorkspaceType::Simple => "Simple",
                WorkspaceType::Worktree => "Worktree",
                WorkspaceType::Project => "Project",
            }),
        ]),
        Line::from(vec![
            Span::styled(" Group:   ", label_style),
            Span::raw(ws.info.group.as_deref().unwrap_or("(none)").to_string()),
        ]),
        Line::from(""),
    ];

    if !ws.description.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " Description: ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.help.border),
        )]));
        lines.push(Line::from(format!("  {}", ws.description)));
        lines.push(Line::from(""));
    }

    if !ws.prompt.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " Prompt: ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.help.border),
        )]));
        // Wrap prompt text
        let max_width = 56usize;
        for chunk in ws.prompt.as_bytes().chunks(max_width) {
            let s = String::from_utf8_lossy(chunk);
            lines.push(Line::from(format!("  {}", s)));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        " Esc to close · h/l to scroll · mouse select to copy",
        Style::default().fg(Color::DarkGray),
    )));

    let height = (lines.len() as u16 + 2).min(area.height);
    let popup = super::clear_popup(frame, area, 70, height);

    let block = super::popup_block(&ws.name, theme.help.border);

    let text = Paragraph::new(lines).block(block).scroll((0, info_hscroll));
    frame.render_widget(text, popup);
}

pub(crate) fn render_confirm_delete_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let target = match app.active_dialog {
        Some(DialogState::ConfirmDelete { target }) => Some(target),
        _ => None,
    };

    let popup = super::clear_popup(frame, area, 50, 9);
    let theme = &app.theme.dialog;

    let ws = target.and_then(|idx| app.workspaces.get(idx));
    let ws_name = ws.map(|ws| ws.name.as_str()).unwrap_or("?");
    let is_worktree = ws
        .map(|ws| ws.info.workspace_type == WorkspaceType::Worktree)
        .unwrap_or(false);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if is_worktree {
                    "  Delete "
                } else {
                    "  Remove "
                },
                Style::default().fg(theme.delete_text),
            ),
            Span::styled(
                ws_name,
                Style::default()
                    .fg(theme.delete_name)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ?", Style::default().fg(theme.delete_text)),
        ]),
        Line::from(""),
    ];

    if !is_worktree {
        lines.push(Line::from(Span::styled(
            "  [y] Yes, remove from list",
            Style::default().fg(theme.delete_yes),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  [y] Yes, delete worktree and branch",
            Style::default().fg(theme.delete_yes),
        )));
        lines.push(Line::from(Span::styled(
            "  [n] No, keep worktree on disk",
            Style::default().fg(theme.delete_no),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  [Esc] Cancel",
        Style::default().fg(theme.delete_cancel),
    )));

    let text = Paragraph::new(lines).block(super::popup_block("Delete Workspace", theme.delete_border));
    frame.render_widget(text, popup);
}
