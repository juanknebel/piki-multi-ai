use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, FileStatus, SidebarItem};
use piki_core::WorkspaceType;

use super::layout::pane_border_style;

/// Icon prefix for each workspace type.
fn workspace_type_icon(ws_type: WorkspaceType) -> &'static str {
    match ws_type {
        WorkspaceType::Worktree => "⎇ ",
        WorkspaceType::Project => "▣ ",
        WorkspaceType::Simple => "○ ",
    }
}

/// Returns the visual height (in lines) of a sidebar item at the given index.
/// Workspace items that follow another workspace get an extra separator line.
fn sidebar_item_height(items: &[SidebarItem], idx: usize) -> usize {
    match &items[idx] {
        SidebarItem::GroupHeader { .. } => 1,
        SidebarItem::Workspace { .. } => {
            if idx > 0 && matches!(items[idx - 1], SidebarItem::Workspace { .. }) {
                4 // 1 separator + 3 content
            } else {
                3
            }
        }
    }
}

pub(super) fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);
    let theme = &app.theme.workspace_list;

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.workspaces.is_empty() {
        let key_style = Style::default().fg(app.theme.footer.key);
        let desc_style = Style::default().fg(theme.empty_text);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(" [n]", key_style),
                Span::styled(" New workspace", desc_style),
            ]),
        ];
        let text = Paragraph::new(lines).block(block);
        frame.render_widget(text, area);
        return;
    }

    let sidebar_items = app.sidebar_items();

    // Pre-compute visual position (0-based) for each workspace index in sidebar order.
    // This maps workspace_index → visual_position so 1-9 badges reflect display order.
    let mut ws_visual_pos: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut visual_counter = 0;
    for item in &sidebar_items {
        if let SidebarItem::Workspace { index } = item {
            ws_visual_pos.insert(*index, visual_counter);
            visual_counter += 1;
        }
    }

    // Compute scroll offset for mixed-height items
    let visible_height = area.height.saturating_sub(2) as usize;
    let mut scroll_offset = 0;
    if visible_height > 0 {
        // Sum heights up to and including selected row
        let mut height_to_selected: usize = 0;
        for i in 0..=app.selected_sidebar_row.min(sidebar_items.len().saturating_sub(1)) {
            height_to_selected += sidebar_item_height(&sidebar_items, i);
            if i == app.selected_sidebar_row {
                break;
            }
        }
        if height_to_selected > visible_height {
            // Find first item to skip so selected fits
            let mut skip_height = height_to_selected - visible_height;
            for i in 0..sidebar_items.len() {
                let h = sidebar_item_height(&sidebar_items, i);
                if skip_height == 0 {
                    break;
                }
                scroll_offset = i + 1;
                skip_height = skip_height.saturating_sub(h);
            }
        }
    }

    // Build the separator line (full inner width of '─' chars)
    let inner_width = area.width.saturating_sub(2) as usize;
    let separator_str: String = "─".repeat(inner_width);

    // Track workspace visual index for alternating backgrounds
    let mut ws_visual_idx: usize = 0;

    let mut total_lines = 0;
    let items: Vec<ListItem> = sidebar_items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .filter(|(i, _)| {
            let h = sidebar_item_height(&sidebar_items, *i);
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
                    let header_style = Style::default()
                        .fg(theme.name_inactive)
                        .bg(theme.group_header_bg)
                        .add_modifier(Modifier::BOLD);
                    let line = Line::from(vec![
                        Span::styled(format!(" {} ", arrow), header_style),
                        Span::styled(
                            format!("{} ({})", name.to_uppercase(), count),
                            header_style,
                        ),
                    ]);
                    let style = if is_selected {
                        Style::default().bg(theme.selected_bg)
                    } else {
                        Style::default().bg(theme.group_header_bg)
                    };
                    ListItem::new(vec![line]).style(style)
                }
                SidebarItem::Workspace { index } => {
                    let ws = &app.workspaces[*index];
                    let current_ws_idx = ws_visual_idx;
                    ws_visual_idx += 1;

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

                    let type_icon = workspace_type_icon(ws.info.workspace_type);

                    let line1 = Line::from(vec![
                        Span::styled(number_badge.to_string(), Style::default().fg(detail_color)),
                        Span::raw(format!("{} ", marker)),
                        Span::styled(
                            type_icon,
                            Style::default().fg(detail_color),
                        ),
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
                    let count_label = if ws.info.workspace_type == WorkspaceType::Project {
                        format!("{} services", ws.file_count())
                    } else {
                        format!("{} files", ws.file_count())
                    };
                    let line2 = Line::from(vec![
                        Span::raw("   "),
                        Span::styled(count_label, Style::default().fg(detail_color)),
                    ]);

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
                    let line3 = Line::from(vec![
                        Span::raw("   "),
                        Span::styled(truncated, Style::default().fg(detail_color)),
                    ]);

                    // Build lines: optionally prepend separator between consecutive workspaces
                    let mut lines = Vec::new();
                    let has_separator =
                        row > 0 && matches!(sidebar_items[row - 1], SidebarItem::Workspace { .. });
                    if has_separator {
                        lines.push(Line::from(Span::styled(
                            separator_str.as_str(),
                            Style::default().fg(theme.separator),
                        )));
                    }
                    lines.push(line1);
                    lines.push(line2);
                    lines.push(line3);

                    let style = if is_selected {
                        Style::default().bg(theme.selected_bg)
                    } else if current_ws_idx % 2 == 1 {
                        Style::default().bg(theme.alt_bg)
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

    let total_visual_height: usize = (0..sidebar_items.len())
        .map(|i| sidebar_item_height(&sidebar_items, i))
        .sum();
    let scroll_pos: usize = (0..scroll_offset)
        .map(|i| sidebar_item_height(&sidebar_items, i))
        .sum();
    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_pos,
        total_visual_height,
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
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
        let text = Paragraph::new("  No services found")
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
    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        dirs.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
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

    let sel_count = app.selection_count();
    let title = if sel_count > 0 {
        format!(" STATUS ({} selected) ", sel_count)
    } else {
        " STATUS ".to_string()
    };
    let mut block = Block::default()
        .title(title)
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
        let text = Paragraph::new(Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(Color::Green)),
            Span::styled(
                "Working tree clean",
                Style::default().fg(theme.empty_text),
            ),
        ]))
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
            let is_multi_selected = app.is_file_selected(&f.path);
            let select_marker = if is_multi_selected { ">" } else { " " };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {}{} ", select_marker, label),
                    Style::default().fg(color),
                ),
                Span::styled(&f.path, Style::default().fg(theme.file_path)),
            ]);
            let style = if i == app.selected_file && is_active {
                Style::default().bg(theme.selected_bg)
            } else if is_multi_selected {
                Style::default().bg(theme.multi_select_bg)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        files.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
}
