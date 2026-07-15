use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, DialogField, NewWorkspaceSource, WorkspaceType};
use crate::dialog_state::{CreateWorktreeField, CreateWorktreeMode, DialogState, EditWorkspaceField};

pub(crate) fn render_new_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::NewWorkspace {
        ref dir,
        dir_cursor,
        ref destination,
        destination_cursor,
        ref desc,
        desc_cursor,
        ref prompt,
        prompt_cursor,
        ref kanban,
        kanban_cursor,
        source,
        active_field,
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    // Layout: Source, Directory/URL, [Destination (GitHub only)],
    // Desc, Prompt, Kanban → 5 fields for Local (14 lines), 6 for
    // GitHub (16 lines including the extra "Clone into" row).
    let is_github = source == NewWorkspaceSource::GitHub;
    let popup_height: u16 = if is_github { 16 } else { 14 };
    let popup = super::clear_popup(frame, area, popup_width.max(40), popup_height);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let source_active = active_field == DialogField::Source;
    let dir_active = active_field == DialogField::Directory;
    let destination_active = active_field == DialogField::Destination;
    let desc_active = active_field == DialogField::Description;
    let prompt_active = active_field == DialogField::Prompt;
    let kanban_active = active_field == DialogField::KanbanPath;

    let source_text = match source {
        NewWorkspaceSource::Local => "[Local folder]  GitHub URL",
        NewWorkspaceSource::GitHub => " Local folder  [GitHub URL]",
    };
    let dir_label = match source {
        NewWorkspaceSource::Local => "  Folder:  ",
        NewWorkspaceSource::GitHub => "  URL:     ",
    };

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(vec![
            Span::styled(
                "  Source:  ",
                super::field_style(source_active, active_c, inactive_c),
            ),
            Span::styled(
                source_text,
                super::field_style(source_active, active_c, inactive_c),
            ),
        ]),
        Line::from(""),
        super::render_text_field(
            dir_label,
            dir,
            dir_active,
            dir_cursor,
            fmax,
            super::field_style(dir_active, active_c, inactive_c),
        ),
        Line::from(""),
    ];

    if is_github {
        lines.push(super::render_text_field(
            "  Clone:  ",
            destination,
            destination_active,
            destination_cursor,
            fmax,
            super::field_style(destination_active, active_c, inactive_c),
        ));
        lines.push(Line::from(""));
    }

    lines.extend([
        super::render_text_field(
            "  Desc:   ",
            desc,
            desc_active,
            desc_cursor,
            fmax,
            super::field_style(desc_active, active_c, inactive_c),
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
            "  Kanban: ",
            kanban,
            kanban_active,
            kanban_cursor,
            fmax,
            super::field_style(kanban_active, active_c, inactive_c),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [Esc] cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ]);

    let text = Paragraph::new(lines).block(super::popup_block("New Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_edit_workspace_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::EditWorkspace {
        ref kanban,
        kanban_cursor,
        ref prompt,
        prompt_cursor,
        active_field,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let popup = super::clear_popup(frame, area, popup_width.max(40), 10);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let kanban_active = active_field == EditWorkspaceField::KanbanPath;
    let prompt_active = active_field == EditWorkspaceField::Prompt;

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
        Line::from(vec![Span::styled(
            "  [Esc] cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ];

    let text = Paragraph::new(lines).block(super::popup_block("Edit Workspace", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_create_worktree_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::CreateWorktree { mode, .. }) = &app.active_dialog else {
        return;
    };

    match mode {
        CreateWorktreeMode::ChooseSource => render_create_worktree_choose_source(frame, area, app),
        CreateWorktreeMode::CreateNew => render_create_worktree_create_new(frame, area, app),
        CreateWorktreeMode::LoadExisting => render_create_worktree_load_existing(frame, area, app),
    }
}

fn render_create_worktree_choose_source(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::CreateWorktree { existing_selected, .. }) = &app.active_dialog else {
        return;
    };

    let popup_width = area.width * 60 / 100;
    let popup = super::clear_popup(frame, area, popup_width.max(40), 8);
    let theme = &app.theme.dialog;
    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;

    let rows = ["Create new worktree", "Load existing worktree"];
    let mut lines: Vec<Line<'_>> = vec![Line::from("")];
    for (idx, label) in rows.iter().enumerate() {
        let is_selected = idx == *existing_selected;
        let prefix = if is_selected { "  > " } else { "    " };
        let style = if is_selected {
            Style::default().fg(active_c).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(inactive_c)
        };
        lines.push(Line::from(Span::styled(format!("{prefix}{label}"), style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  [j/k] move  [Enter] select  [Esc] cancel",
        Style::default().fg(inactive_c),
    )]));

    let text =
        Paragraph::new(lines).block(super::popup_block("Create Worktree", theme.new_ws_border));
    frame.render_widget(text, popup);
}

fn render_create_worktree_create_new(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::CreateWorktree {
        parent_idx,
        ref name,
        name_cursor,
        ref prompt,
        prompt_cursor,
        ref kanban,
        kanban_cursor,
        active_field,
        ..
    }) = app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let popup = super::clear_popup(frame, area, popup_width.max(40), 12);
    let theme = &app.theme.dialog;

    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;
    let label_width = 10_u16;
    let fmax = popup.width.saturating_sub(label_width + 2) as usize;

    let name_active = active_field == CreateWorktreeField::Name;
    let prompt_active = active_field == CreateWorktreeField::Prompt;
    let kanban_active = active_field == CreateWorktreeField::KanbanPath;

    let parent_label = app
        .workspaces
        .get(parent_idx)
        .map(|w| w.info.source_repo_display.as_str())
        .unwrap_or("?");

    let lines = vec![
        Line::from(vec![
            Span::styled(
                "  Parent: ",
                Style::default().fg(inactive_c),
            ),
            Span::raw(parent_label.to_string()),
        ]),
        Line::from(""),
        super::render_text_field(
            "  Name:   ",
            name,
            name_active,
            name_cursor,
            fmax,
            super::field_style(name_active, active_c, inactive_c),
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
            "  Kanban: ",
            kanban,
            kanban_active,
            kanban_cursor,
            fmax,
            super::field_style(kanban_active, active_c, inactive_c),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [Esc] cancel",
            Style::default().fg(theme.new_ws_inactive),
        )]),
    ];

    let text =
        Paragraph::new(lines).block(super::popup_block("Create Worktree", theme.new_ws_border));
    frame.render_widget(text, popup);
}

fn render_create_worktree_load_existing(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::CreateWorktree {
        existing,
        existing_selected,
        existing_loading,
        ..
    }) = &app.active_dialog
    else {
        return;
    };

    let popup_width = area.width * 70 / 100;
    let height = 8u16.saturating_add(existing.len().min(10) as u16);
    let popup = super::clear_popup(frame, area, popup_width.max(40), height);
    let theme = &app.theme.dialog;
    let active_c = theme.new_ws_active;
    let inactive_c = theme.new_ws_inactive;

    let mut lines: Vec<Line<'_>> = vec![Line::from("")];
    if *existing_loading {
        lines.push(Line::from(Span::styled(
            "  Scanning worktrees...",
            Style::default().fg(inactive_c),
        )));
    } else if existing.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No other worktrees found for this repository.",
            Style::default().fg(inactive_c),
        )));
    } else {
        for (idx, wt) in existing.iter().enumerate() {
            let is_selected = idx == *existing_selected;
            let prefix = if is_selected { "  > " } else { "    " };
            let style = if is_selected {
                Style::default().fg(active_c).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(inactive_c)
            };
            let label = format!("{} ({})", wt.path.display(), wt.branch);
            lines.push(Line::from(Span::styled(format!("{prefix}{label}"), style)));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  [j/k] move  [Enter] load  [Esc] cancel",
        Style::default().fg(inactive_c),
    )]));

    let text =
        Paragraph::new(lines).block(super::popup_block("Load Existing Worktree", theme.new_ws_border));
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
        " [h/l] scroll  [i/Esc] close  [mouse] select to copy",
        Style::default().fg(theme.palette.fg3),
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
        "  [Esc] cancel",
        Style::default().fg(theme.delete_cancel),
    )));

    let text = Paragraph::new(lines).block(super::popup_block("Delete Workspace", theme.delete_border));
    frame.render_widget(text, popup);
}
