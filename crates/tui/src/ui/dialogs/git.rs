use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::dialog_state::DialogState;

pub(crate) fn render_commit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let commit_buffer = match app.active_dialog {
        Some(DialogState::CommitMessage { ref buffer }) => buffer.as_str(),
        _ => "",
    };

    let popup_width = area.width * 60 / 100;
    let popup = super::clear_popup(frame, area, popup_width.max(40), 7);
    let theme = &app.theme.dialog;

    let field_max = popup.width.saturating_sub(14) as usize;
    let cursor = "█";
    let full = format!("{}{}", commit_buffer, cursor);
    let visible = if full.len() > field_max && field_max > 2 {
        format!("…{}", &full[full.len() - (field_max - 1)..])
    } else {
        full
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Message: ", Style::default().fg(theme.new_ws_active)),
            Span::styled(visible, Style::default().fg(theme.new_ws_active)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter] Commit  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )),
    ];

    let text = Paragraph::new(lines).block(super::popup_block("Commit", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_confirm_merge_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let popup = super::clear_popup(frame, area, 50, 9);
    let theme = &app.theme.dialog;

    let branch_name = app
        .current_workspace()
        .map(|ws| ws.branch.as_str())
        .unwrap_or("?");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Merge ", Style::default().fg(theme.new_ws_active)),
            Span::styled(
                branch_name,
                Style::default()
                    .fg(theme.new_ws_active)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" into main?", Style::default().fg(theme.new_ws_active)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [m] Merge commit",
            Style::default().fg(theme.new_ws_inactive),
        )),
        Line::from(Span::styled(
            "  [r] Rebase + fast-forward",
            Style::default().fg(theme.new_ws_inactive),
        )),
        Line::from(Span::styled(
            "  [Esc] Cancel",
            Style::default().fg(theme.new_ws_inactive),
        )),
    ];

    let text = Paragraph::new(lines).block(super::popup_block("Merge", theme.new_ws_border));
    frame.render_widget(text, popup);
}

pub(crate) fn render_git_stash_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (entries, selected, _scroll, input_mode, input_buffer, input_cursor) =
        match app.active_dialog {
            Some(DialogState::GitStash {
                ref entries,
                selected,
                scroll,
                input_mode,
                ref input_buffer,
                input_cursor,
            }) => (
                entries,
                selected,
                scroll,
                input_mode,
                input_buffer,
                input_cursor,
            ),
            _ => return,
        };

    let theme = &app.theme;
    let width = area.width * 75 / 100;
    let height = area.height * 70 / 100;
    let popup = super::clear_popup(frame, area, width.max(40), height.max(10));

    let inner_height = popup.height.saturating_sub(2) as usize; // borders
    // Reserve 2 lines for footer hint (and input if active)
    let footer_lines = if input_mode { 3 } else { 2 };
    let visible_rows = inner_height.saturating_sub(footer_lines);

    let mut lines: Vec<Line<'_>> = Vec::new();

    if entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                "  No stash entries",
                Style::default().fg(Color::DarkGray),
            ))
            .centered(),
        );
        lines.push(Line::from(""));
    } else {
        // Compute scroll to keep selected visible
        let scroll_offset = if selected >= visible_rows {
            selected + 1 - visible_rows
        } else {
            0
        };

        for (i, (stash_ref, message)) in entries.iter().enumerate() {
            if i < scroll_offset {
                continue;
            }
            if lines.len() >= visible_rows {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let bg = if is_selected {
                theme.workspace_list.selected_bg
            } else {
                Color::Reset
            };
            let ref_style = Style::default()
                .fg(Color::Yellow)
                .bg(bg)
                .add_modifier(Modifier::BOLD);
            let msg_style = Style::default().fg(Color::White).bg(bg);
            let marker_style = Style::default().fg(Color::Cyan).bg(bg);

            // Truncate message to fit
            let max_msg_len = popup.width.saturating_sub(20) as usize;
            let msg_display: String = if message.len() > max_msg_len {
                format!("{}...", &message[..max_msg_len.saturating_sub(3)])
            } else {
                message.clone()
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", marker), marker_style),
                Span::styled(format!("{:<12}", stash_ref), ref_style),
                Span::styled(format!(" {}", msg_display), msg_style),
            ]));
        }
    }

    // Pad remaining visible rows
    while lines.len() < visible_rows {
        lines.push(Line::from(""));
    }

    // Input line for stash save
    if input_mode {
        let label_style = Style::default().fg(Color::Cyan);
        let input_display = super::visible_field(
            input_buffer,
            true,
            input_cursor,
            popup.width.saturating_sub(14) as usize,
        );
        lines.push(Line::from(vec![
            Span::styled("  Message: ", label_style),
            Span::styled(input_display, Style::default().fg(Color::White)),
        ]));
    }

    // Footer hint
    let hint = if input_mode {
        " Enter confirm  Esc cancel"
    } else {
        " s save  p pop  a apply  d drop  Enter show  Esc close"
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));

    // Title with count
    let title_text = if entries.is_empty() {
        " Git Stash ".to_string()
    } else {
        format!(" Git Stash [{}/{}] ", selected + 1, entries.len())
    };

    let block = Block::default()
        .title(title_text)
        .title_style(Style::default().fg(theme.help.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.help.border));

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}

pub(crate) fn render_git_log_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (log_lines, selected, scroll) = match app.active_dialog {
        Some(DialogState::GitLog {
            ref lines,
            selected,
            scroll,
        }) => (lines, selected, scroll),
        _ => return,
    };

    let theme = &app.theme;
    let width = (area.width * 85 / 100).max(40).min(area.width);
    let height = (area.height * 80 / 100).max(10).min(area.height);
    let popup = super::clear_popup(frame, area, width, height);

    let inner_height = popup.height.saturating_sub(2); // borders
    if inner_height < 3 {
        return;
    }

    let footer_height: u16 = 1;
    let body_height = (inner_height - footer_height) as usize;

    // Auto-scroll to keep selected visible
    let actual_scroll = if selected < scroll {
        selected
    } else if selected >= scroll + body_height {
        selected - body_height + 1
    } else {
        scroll
    };

    let total = log_lines.len();

    // Build visible lines
    let mut display_lines: Vec<Line<'_>> = Vec::new();
    for (i, entry) in log_lines
        .iter()
        .skip(actual_scroll)
        .take(body_height)
        .enumerate()
    {
        let is_selected = actual_scroll + i == selected;
        let bg = if is_selected {
            theme.workspace_list.selected_bg
        } else {
            Color::Reset
        };
        let style = Style::default().bg(bg);

        // Truncate line to fit popup width (leave 2 for borders + 1 for padding)
        let max_chars = (popup.width.saturating_sub(3)) as usize;
        let truncated: String = entry.raw_line.chars().take(max_chars).collect();

        display_lines.push(Line::from(Span::styled(
            format!(" {:<width$}", truncated, width = max_chars),
            style,
        )));
    }

    // Pad remaining space
    while display_lines.len() < body_height {
        display_lines.push(Line::from(""));
    }

    // Footer hint
    display_lines.push(Line::from(vec![
        Span::styled(
            " [j/k]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" navigate  "),
        Span::styled(
            "[enter]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" view diff  "),
        Span::styled(
            "[esc]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" close"),
    ]));

    // Title with scroll indicator
    let scroll_indicator = if total > 0 {
        format!(" [{}/{}] ", selected + 1, total)
    } else {
        String::new()
    };

    let block = super::popup_block("Git Log", theme.help.border)
        .title_bottom(Line::from(scroll_indicator).right_aligned());

    let text = Paragraph::new(display_lines).block(block);
    frame.render_widget(text, popup);
}

pub(crate) fn render_conflict_resolution_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let (conflict_files, selected) = match app.active_dialog {
        Some(DialogState::ConflictResolution {
            ref files,
            selected,
            ..
        }) => (files, selected),
        _ => return,
    };

    let theme = &app.theme;
    let width = (area.width * 70 / 100).max(50).min(area.width);
    let height = (area.height * 60 / 100).max(12).min(area.height);
    let popup = super::clear_popup(frame, area, width, height);

    let inner_height = popup.height.saturating_sub(2) as usize; // borders
    let footer_lines = 2;
    let visible_rows = inner_height.saturating_sub(footer_lines);

    let mut lines: Vec<Line<'_>> = Vec::new();

    if conflict_files.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                "  No conflicted files",
                Style::default().fg(Color::DarkGray),
            ))
            .centered(),
        );
        lines.push(Line::from(""));
    } else {
        // Compute scroll to keep selected visible
        let scroll_offset = if selected >= visible_rows {
            selected + 1 - visible_rows
        } else {
            0
        };

        for (i, f) in conflict_files.iter().enumerate() {
            if i < scroll_offset {
                continue;
            }
            if lines.len() >= visible_rows {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let bg = if is_selected {
                theme.workspace_list.selected_bg
            } else {
                Color::Reset
            };
            let marker_style = Style::default().fg(Color::Cyan).bg(bg);
            let icon_style = Style::default()
                .fg(Color::Magenta)
                .bg(bg)
                .add_modifier(Modifier::BOLD);
            let path_style = Style::default().fg(Color::White).bg(bg);

            // Truncate path to fit
            let max_path_len = popup.width.saturating_sub(10) as usize;
            let path_display: String = if f.path.len() > max_path_len {
                format!("...{}", &f.path[f.path.len() - max_path_len.saturating_sub(3)..])
            } else {
                f.path.clone()
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", marker), marker_style),
                Span::styled("C ", icon_style),
                Span::styled(path_display, path_style),
            ]));
        }
    }

    // Pad remaining visible rows
    while lines.len() < visible_rows {
        lines.push(Line::from(""));
    }

    // Footer hints
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            " [o]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" ours  "),
        Span::styled(
            "[t]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" theirs  "),
        Span::styled(
            "[m]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" mark resolved  "),
        Span::styled(
            "[e]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" edit  "),
        Span::styled(
            "[A]",
            Style::default().fg(Color::Red),
        ),
        Span::raw(" abort  "),
        Span::styled(
            "[esc]",
            Style::default().fg(theme.workspace_list.name_active),
        ),
        Span::raw(" close"),
    ]));

    // Title with count indicator
    let count_indicator = format!(
        " [{}/{}] ",
        if conflict_files.is_empty() {
            0
        } else {
            selected + 1
        },
        conflict_files.len()
    );

    let block = super::popup_block("Conflict Resolution", Color::Magenta)
        .title_bottom(Line::from(count_indicator).right_aligned());

    let text = Paragraph::new(lines).block(block);
    frame.render_widget(text, popup);
}
