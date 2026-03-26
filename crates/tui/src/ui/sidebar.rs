use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, FileStatus, SidebarItem};

use super::layout::pane_border_style;

pub(super) fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
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

    let sidebar_items = app.sidebar_items();

    // Pre-compute visual position (0-based) for each workspace index in sidebar order.
    // This maps workspace_index → visual_position so 1-9 badges reflect display order.
    let mut ws_visual_pos: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut visual_counter = 0;
    for item in &sidebar_items {
        if let SidebarItem::Workspace { index } = item {
            ws_visual_pos.insert(*index, visual_counter);
            visual_counter += 1;
        }
    }

    // Compute item heights and scroll offset for mixed-height items
    let item_height = |item: &SidebarItem| -> usize {
        match item {
            SidebarItem::GroupHeader { .. } => 1,
            SidebarItem::Workspace { .. } => 3,
        }
    };
    let visible_height = area.height.saturating_sub(2) as usize;
    let mut scroll_offset = 0;
    if visible_height > 0 {
        // Sum heights up to and including selected row
        let mut height_to_selected: usize = 0;
        for (i, item) in sidebar_items.iter().enumerate() {
            height_to_selected += item_height(item);
            if i == app.selected_sidebar_row {
                break;
            }
        }
        if height_to_selected > visible_height {
            // Find first item to skip so selected fits
            let mut skip_height = height_to_selected - visible_height;
            for (i, item) in sidebar_items.iter().enumerate() {
                let h = item_height(item);
                if skip_height == 0 {
                    break;
                }
                scroll_offset = i + 1;
                skip_height = skip_height.saturating_sub(h);
            }
        }
    }

    let mut total_lines = 0;
    let items: Vec<ListItem> = sidebar_items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .filter(|(_, item)| {
            let h = item_height(item);
            if total_lines + h > visible_height {
                return false;
            }
            total_lines += h;
            true
        })
        .map(|(row, item)| {
            let is_selected = row == app.selected_sidebar_row;
            match item {
                SidebarItem::GroupHeader {
                    name,
                    count,
                    collapsed,
                } => {
                    let arrow = if *collapsed { "▸" } else { "▼" };
                    let line = Line::from(vec![
                        Span::raw(format!(" {} ", arrow)),
                        Span::styled(
                            format!("{} ({})", name, count),
                            Style::default()
                                .fg(theme.name_inactive)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    let style = if is_selected {
                        Style::default().bg(theme.selected_bg)
                    } else {
                        Style::default()
                    };
                    ListItem::new(vec![line]).style(style)
                }
                SidebarItem::Workspace { index } => {
                    let ws = &app.workspaces[*index];
                    let detail_color = if is_selected {
                        theme.detail_selected
                    } else {
                        theme.detail_normal
                    };

                    let marker = if *index == app.active_workspace {
                        "▶"
                    } else {
                        " "
                    };
                    // Show 1-9 badge matching visual order for quick-jump shortcuts
                    let visual_pos = ws_visual_pos.get(index).copied().unwrap_or(usize::MAX);
                    let number_badge = if visual_pos < 9 {
                        format!("{}", visual_pos + 1)
                    } else {
                        " ".to_string()
                    };
                    let line1 = Line::from(vec![
                        Span::styled(
                            number_badge.to_string(),
                            Style::default().fg(detail_color),
                        ),
                        Span::raw(format!("{} ", marker)),
                        Span::styled(
                            ws.name.clone(),
                            if *index == app.active_workspace {
                                Style::default()
                                    .fg(theme.name_active)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.name_inactive)
                            },
                        ),
                    ]);
                    let count_label = if ws.info.workspace_type == piki_core::WorkspaceType::Project
                    {
                        format!("{} services", ws.file_count())
                    } else {
                        format!("{} files", ws.file_count())
                    };
                    let line2 = Line::from(vec![
                        Span::raw("   "),
                        Span::styled(count_label, Style::default().fg(detail_color)),
                    ]);

                    let mut lines = vec![line1, line2];

                    // Line 3: description if available, otherwise branch name
                    let detail_text = if ws.info.description.is_empty() {
                        format!("⎇ {}", ws.info.branch)
                    } else {
                        ws.info.description.clone()
                    };
                    let max_len = area.width.saturating_sub(6) as usize;
                    let truncated = if detail_text.len() > max_len {
                        format!("{}…", &detail_text[..max_len.saturating_sub(1)])
                    } else {
                        detail_text
                    };
                    lines.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(truncated, Style::default().fg(detail_color)),
                    ]));

                    let style = if is_selected {
                        Style::default().bg(theme.selected_bg)
                    } else {
                        Style::default()
                    };

                    ListItem::new(lines).style(style)
                }
            }
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

pub(super) fn render_file_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::GitStatus;
    let border_style = pane_border_style(app, ActivePane::GitStatus);
    let theme = &app.theme.file_list;

    let is_project = app
        .current_workspace()
        .is_some_and(|ws| ws.info.workspace_type == piki_core::WorkspaceType::Project);

    if is_project {
        render_project_file_list(frame, area, app, is_active, border_style, theme);
    } else {
        render_git_file_list(frame, area, app, is_active, border_style, theme);
    }
}

fn render_project_file_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    is_active: bool,
    border_style: Style,
    theme: &crate::theme::FileListTheme,
) {
    let block = Block::default()
        .title(" SERVICES ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    let dirs = app
        .current_workspace()
        .map(|ws| &ws.sub_directories[..])
        .unwrap_or(&[]);

    if dirs.is_empty() {
        let text = Paragraph::new("  No sub-directories")
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let visible_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = if visible_height > 0 && app.selected_file >= visible_height {
        app.selected_file - visible_height + 1
    } else {
        0
    };

    let items: Vec<ListItem> = dirs
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(i, dir_name)| {
            let line = Line::from(vec![
                Span::styled("  📂 ", Style::default().fg(theme.file_path)),
                Span::styled(dir_name.as_str(), Style::default().fg(theme.file_path)),
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

fn render_git_file_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    is_active: bool,
    border_style: Style,
    theme: &crate::theme::FileListTheme,
) {
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

    let visible_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = if visible_height > 0 && app.selected_file >= visible_height {
        app.selected_file - visible_height + 1
    } else {
        0
    };

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
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
