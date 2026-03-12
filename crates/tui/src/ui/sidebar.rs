use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, FileStatus};

use super::layout::pane_border_style;

pub(super) fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::WorkspaceList;
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);
    let theme = &app.theme.workspace_list;

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.workspaces.is_empty() {
        let text = Paragraph::new("  Press [n] to create")
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = app
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let is_selected = i == app.selected_workspace && is_active;
            let detail_color = if is_selected {
                theme.detail_selected
            } else {
                theme.detail_normal
            };

            let marker = if i == app.active_workspace {
                "▶"
            } else {
                " "
            };
            let status_icon = match ws.status {
                crate::app::WorkspaceStatus::Idle => "●",
                crate::app::WorkspaceStatus::Busy => "◐",
                crate::app::WorkspaceStatus::Done => "✓",
                crate::app::WorkspaceStatus::Error(_) => "✗",
            };
            let line1 = Line::from(vec![
                Span::raw(format!(" {} ", marker)),
                Span::styled(
                    ws.name.clone(),
                    if i == app.active_workspace {
                        Style::default()
                            .fg(theme.name_active)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.name_inactive)
                    },
                ),
            ]);
            let line2 = Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!(
                        "{} {} | {} files",
                        status_icon,
                        ws.status_label(),
                        ws.file_count()
                    ),
                    Style::default().fg(detail_color),
                ),
            ]);

            let mut lines = vec![line1, line2];

            // Show parent project (pre-computed in WorkspaceInfo)
            let project_name = &ws.source_repo_display;
            let max_proj = area.width.saturating_sub(6) as usize;
            let proj_text = if project_name.len() > max_proj {
                format!("⌂ {}…", &project_name[..max_proj.saturating_sub(1)])
            } else {
                format!("⌂ {}", project_name)
            };
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(proj_text, Style::default().fg(detail_color)),
            ]));

            let style = if is_selected {
                Style::default().bg(theme.selected_bg)
            } else {
                Style::default()
            };

            ListItem::new(lines).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

pub(super) fn render_file_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::GitStatus;
    let border_style = pane_border_style(app, ActivePane::GitStatus);
    let theme = &app.theme.file_list;

    let ahead_title = app
        .current_workspace()
        .and_then(|ws| ws.ahead_behind)
        .and_then(|(ahead, behind)| {
            if ahead > 0 && behind > 0 {
                Some(format!(" ↑{} ↓{} ", ahead, behind))
            } else if ahead > 0 {
                Some(format!(" ↑{} to push ", ahead))
            } else if behind > 0 {
                Some(format!(" ↓{} behind ", behind))
            } else {
                None
            }
        });

    let mut block = Block::default()
        .title(" STATUS ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    if let Some(ref title) = ahead_title {
        block = block.title_bottom(Line::from(Span::styled(
            title.as_str(),
            Style::default().fg(theme.modified),
        )));
    }

    let files = app
        .current_workspace()
        .map(|ws| &ws.changed_files[..])
        .unwrap_or(&[]);

    if files.is_empty() {
        let text = Paragraph::new("  No files changed")
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (label, color) = match f.status {
                FileStatus::Modified => ("M", theme.modified),
                FileStatus::Added => ("A", theme.added),
                FileStatus::Deleted => ("D", theme.deleted),
                FileStatus::Renamed => ("R", theme.renamed),
                FileStatus::Untracked => ("?", theme.untracked),
                FileStatus::Conflicted => ("C", theme.conflicted),
                FileStatus::Staged => ("S", theme.staged),
                FileStatus::StagedModified => ("SM", theme.staged_modified),
            };
            let line = Line::from(vec![
                Span::styled(format!("  {} ", label), Style::default().fg(color)),
                Span::styled(&f.path, Style::default().fg(theme.file_path)),
            ]);
            let style = if i == app.selected_file && is_active {
                Style::default().bg(theme.selected_bg)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
