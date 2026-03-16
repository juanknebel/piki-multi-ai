use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::App;
use crate::code_review::{CodeReviewState, ReviewFocus};
use piki_core::github::{DiffLineType, ParsedDiff, ReviewVerdict};

/// Render the full-screen code review layout
pub(super) fn render_fullscreen(frame: &mut Frame, area: Rect, app: &App) {
    let ws = match app.current_workspace() {
        Some(ws) => ws,
        None => return,
    };
    let state = match &ws.code_review {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" Code Review ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));
            let text = Paragraph::new("  Loading PR data...")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(text, area);
            return;
        }
    };

    // Split: header (2) + body (flex) + footer (1)
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    render_pr_header(frame, header_area, state);

    // Body: file list (25%) | diff (75%)
    let [files_area, diff_area] = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Percentage(75),
    ])
    .areas(body_area);

    render_file_list(frame, files_area, state);
    render_diff(frame, diff_area, state);
    render_footer(frame, footer_area, state);

    // Comment input overlay (on top of everything)
    if state.editing_comment.is_some() {
        render_comment_input_overlay(frame, area, state);
    }
}

/// Render PR info header bar
fn render_pr_header(frame: &mut Frame, area: Rect, state: &CodeReviewState) {
    let pr = &state.pr_info;
    let state_color = if pr.state == "OPEN" {
        Color::Green
    } else {
        Color::Red
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" PR #{}: ", pr.number),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &pr.title,
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", pr.state),
            Style::default().fg(state_color),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} ← {}", pr.base_ref_name, pr.head_ref_name),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("+{}", pr.additions),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{}", pr.deletions),
            Style::default().fg(Color::Red),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} files", state.files.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));
    let header = Paragraph::new(line).block(block);
    frame.render_widget(header, area);
}

/// Render the scrollable file list with selection and comment count badges
fn render_file_list(frame: &mut Frame, area: Rect, state: &CodeReviewState) {
    let focus_color = if state.focus == ReviewFocus::FileList {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Changed Files ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(focus_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.files.is_empty() {
        let text = Paragraph::new("  No files changed")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(text, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let scroll = state.file_scroll;
    let items: Vec<ListItem> = state
        .files
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(i, file)| {
            let selected = i == state.selected_file;
            let has_diff = state.file_diffs.contains_key(&file.path);
            let comment_count = state.draft.comment_count_for_file(&file.path);

            let indicator = if selected { ">" } else { " " };
            let cached_marker = if has_diff { "*" } else { " " };

            let style = if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let mut spans = vec![
                Span::styled(format!("{}{} ", indicator, cached_marker), style),
                Span::styled(&file.path, style),
                Span::raw(" "),
                Span::styled(format!("+{}", file.additions), Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::styled(format!("-{}", file.deletions), Style::default().fg(Color::Red)),
            ];

            if comment_count > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[{}]", comment_count),
                    Style::default().fg(Color::Yellow),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Compute the visual lines for a parsed diff, including comment decoration lines.
/// Returns (visual_lines, mapping from visual_row -> diff_line_index or None for comment rows).
fn compute_visual_lines(
    diff: &ParsedDiff,
    draft: &super::super::code_review::ReviewDraft,
    file_path: &str,
) -> Vec<VisualLine> {
    let mut visual = Vec::new();
    for (i, line) in diff.lines.iter().enumerate() {
        visual.push(VisualLine::DiffLine(i));

        // If this line has a comment, add decoration lines after it
        if let Some(ln) = line.new_line {
            if draft.comment_at_line(file_path, ln).is_some() {
                visual.push(VisualLine::CommentHeader(i));
                visual.push(VisualLine::CommentBody(i));
                visual.push(VisualLine::CommentFooter(i));
            }
        } else if let Some(ln) = line.old_line {
            // For deletion-only lines that have comments on old_line
            if line.line_type == DiffLineType::Deletion
                && draft.comment_at_line(file_path, ln).is_some()
            {
                visual.push(VisualLine::CommentHeader(i));
                visual.push(VisualLine::CommentBody(i));
                visual.push(VisualLine::CommentFooter(i));
            }
        }
    }
    visual
}

#[derive(Debug, Clone, Copy)]
enum VisualLine {
    DiffLine(usize),       // index into ParsedDiff.lines
    CommentHeader(usize),  // comment decoration for diff line at index
    CommentBody(usize),
    CommentFooter(usize),
}

/// Render the diff pane with line numbers, cursor, and inline comments
fn render_diff(frame: &mut Frame, area: Rect, state: &CodeReviewState) {
    let focus_color = if state.focus == ReviewFocus::DiffView {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if let Some(path) = state.current_file_path() {
        format!(" {} ", path)
    } else {
        " Diff ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(focus_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(diff) = state.current_diff() {
        let file_path = state.current_file_path().unwrap_or("");
        let visual_lines = compute_visual_lines(diff, &state.draft, file_path);
        let viewport_height = inner.height as usize;
        let scroll = state.diff_scroll;
        let gutter_width: u16 = 11; // "  old | new | "

        for (row_offset, vline) in visual_lines
            .iter()
            .skip(scroll)
            .take(viewport_height)
            .enumerate()
        {
            let y = inner.y + row_offset as u16;
            let available_content_width = inner.width.saturating_sub(gutter_width);

            match vline {
                VisualLine::DiffLine(idx) => {
                    let diff_line = &diff.lines[*idx];
                    let is_cursor = state.focus == ReviewFocus::DiffView
                        && state.cursor_line == *idx;

                    // Gutter
                    let old_str = diff_line
                        .old_line
                        .map(|n| format!("{:>4}", n))
                        .unwrap_or_else(|| "    ".to_string());
                    let new_str = diff_line
                        .new_line
                        .map(|n| format!("{:>4}", n))
                        .unwrap_or_else(|| "    ".to_string());

                    let gutter_style = if is_cursor {
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let gutter_text = format!("{} {} ", old_str, new_str);
                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        inner.x,
                        y,
                        &gutter_text,
                        gutter_style,
                        inner.width,
                    );

                    // Content
                    let (content_style, prefix) = match diff_line.line_type {
                        DiffLineType::Addition => (Style::default().fg(Color::Green), "+"),
                        DiffLineType::Deletion => (Style::default().fg(Color::Red), "-"),
                        DiffLineType::HunkHeader => (
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            "",
                        ),
                        DiffLineType::FileHeader => (
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                            "",
                        ),
                        DiffLineType::Context => (Style::default().fg(Color::Gray), " "),
                    };

                    let content_style = if is_cursor {
                        content_style.add_modifier(Modifier::REVERSED)
                    } else {
                        content_style
                    };

                    let display_text = if matches!(
                        diff_line.line_type,
                        DiffLineType::HunkHeader | DiffLineType::FileHeader
                    ) {
                        diff_line.content.clone()
                    } else {
                        format!("{}{}", prefix, diff_line.content)
                    };

                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        inner.x + gutter_width,
                        y,
                        &display_text,
                        content_style,
                        available_content_width,
                    );

                    // If cursor line, fill remaining width with reversed style
                    if is_cursor {
                        let used = gutter_width as usize + display_text.chars().count();
                        let remaining = inner.width as usize;
                        if used < remaining {
                            let pad = " ".repeat(remaining - used);
                            buf_set_string_clipped(
                                frame.buffer_mut(),
                                inner.x + used as u16,
                                y,
                                &pad,
                                content_style,
                                (remaining - used) as u16,
                            );
                        }
                    }
                }
                VisualLine::CommentHeader(idx) => {
                    let diff_line = &diff.lines[*idx];
                    let ln = diff_line.new_line.or(diff_line.old_line).unwrap_or(0);
                    let header = format!(
                        "{}  \u{250c}\u{2500}\u{2500} line {} \u{2500}\u{2500}",
                        " ".repeat(gutter_width as usize),
                        ln,
                    );
                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        inner.x,
                        y,
                        &header,
                        Style::default().fg(Color::Yellow),
                        inner.width,
                    );
                }
                VisualLine::CommentBody(idx) => {
                    let diff_line = &diff.lines[*idx];
                    let ln = diff_line.new_line.or(diff_line.old_line).unwrap_or(0);
                    let body = state
                        .draft
                        .comment_at_line(file_path, ln)
                        .map(|c| c.body.as_str())
                        .unwrap_or("");
                    let body_line = format!(
                        "{}  \u{2502} {}",
                        " ".repeat(gutter_width as usize),
                        body,
                    );
                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        inner.x,
                        y,
                        &body_line,
                        Style::default().fg(Color::Yellow),
                        inner.width,
                    );
                }
                VisualLine::CommentFooter(_idx) => {
                    let footer = format!(
                        "{}  \u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                        " ".repeat(gutter_width as usize),
                    );
                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        inner.x,
                        y,
                        &footer,
                        Style::default().fg(Color::Yellow),
                        inner.width,
                    );
                }
            }
        }
    } else if state.loading {
        let text = Paragraph::new("  Loading diff...")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(text, inner);
    } else {
        let text = Paragraph::new("  Select a file and press Enter to view diff")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(text, inner);
    }
}

/// Helper: write a string to the buffer, clipped to max_width chars
fn buf_set_string_clipped(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    y: u16,
    s: &str,
    style: Style,
    max_width: u16,
) {
    if max_width == 0 {
        return;
    }
    // Truncate to max_width characters
    let truncated: String = s.chars().take(max_width as usize).collect();
    buf.set_string(x, y, &truncated, style);
}

/// Render context-sensitive footer keybindings
fn render_footer(frame: &mut Frame, area: Rect, state: &CodeReviewState) {
    let keys = if state.editing_comment.is_some() {
        "[Enter] save comment  [Esc] cancel"
    } else if state.show_submit {
        "[Tab] cycle verdict  [Enter] submit  [Esc] close  [Ctrl+D] discard"
    } else {
        match state.focus {
            ReviewFocus::FileList => {
                "[j/k] navigate  [Enter] view diff  [l] diff pane  [s] submit  [r] refresh  [q] close"
            }
            ReviewFocus::DiffView => {
                "[j/k] cursor  [Ctrl+d/u] page  [g/G] top/bottom  [c] comment  [d] del comment  [h] files  [n/p] next/prev  [s] submit  [q] close"
            }
        }
    };

    let line = Line::from(Span::styled(
        format!(" {} ", keys),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

/// Render the comment input overlay
fn render_comment_input_overlay(frame: &mut Frame, area: Rect, state: &CodeReviewState) {
    let ec = match &state.editing_comment {
        Some(ec) => ec,
        None => return,
    };

    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 5u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup);

    let title = format!(" Comment on line {} ", ec.line);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Body text
    let [body_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let body_text = if ec.body.is_empty() {
        Paragraph::new("Type comment...").style(Style::default().fg(Color::DarkGray))
    } else {
        Paragraph::new(ec.body.as_str()).style(Style::default().fg(Color::White))
    };
    frame.render_widget(body_text, body_area);

    // Cursor
    if body_area.width > 0 && body_area.height > 0 {
        let cursor_col = ec.body_cursor.min(ec.body.len()) as u16;
        let col = body_area.x + cursor_col.min(body_area.width.saturating_sub(1));
        frame.set_cursor_position((col, body_area.y));
    }

    let hints = Line::from(Span::styled(
        " [Enter] save  [Esc] cancel ",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(hints).alignment(Alignment::Center), hint_area);
}

/// Render the submit review overlay (centered dialog reading from persistent draft)
pub(super) fn render_submit_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let ws = match app.current_workspace() {
        Some(ws) => ws,
        None => return,
    };
    let state = match &ws.code_review {
        Some(s) => s,
        None => return,
    };

    let draft = &state.draft;
    let comment_count = draft.comments.len();

    // Centered popup: 50 wide, 15 tall
    let popup_width = 50u16.min(area.width.saturating_sub(4));
    let popup_height = 15u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Submit Review ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let has_error = state.submit_error.is_some();
    let [_gap1, verdict_area, comments_area, error_area, _gap2, body_area, hint_area] =
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(if has_error { 1 } else { 0 }),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .areas(inner);

    // Verdict selector
    let verdicts = [
        (ReviewVerdict::Approve, "Approve"),
        (ReviewVerdict::RequestChanges, "Changes"),
        (ReviewVerdict::Comment, "Comment"),
    ];
    let verdict_spans: Vec<Span> = verdicts
        .iter()
        .flat_map(|(v, label)| {
            let selected = *v == draft.verdict;
            let marker = if selected { "\u{25cf}" } else { "\u{25cb}" };
            let style = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            vec![
                Span::raw("  "),
                Span::styled(format!("[{} {}]", marker, label), style),
            ]
        })
        .collect();
    let verdict_line = Line::from(verdict_spans);
    frame.render_widget(
        Paragraph::new(verdict_line).alignment(Alignment::Left),
        verdict_area,
    );

    // Inline comments count
    let comments_text = if comment_count > 0 {
        Span::styled(
            format!("  {} inline comment{}", comment_count, if comment_count == 1 { "" } else { "s" }),
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(
            "  No inline comments",
            Style::default().fg(Color::DarkGray),
        )
    };
    frame.render_widget(Paragraph::new(Line::from(comments_text)), comments_area);

    // Error message (if any)
    if let Some(ref err) = state.submit_error {
        let err_text: String = err.chars().take(inner.width.saturating_sub(2) as usize).collect();
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", err_text),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ))),
            error_area,
        );
    }

    // Body input area
    let body_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));
    let body_inner = body_block.inner(body_area);
    frame.render_widget(body_block, body_area);

    let body_text = if draft.body.is_empty() {
        Paragraph::new("Type review comment...").style(Style::default().fg(Color::DarkGray))
    } else {
        Paragraph::new(draft.body.as_str()).style(Style::default().fg(Color::White))
    };
    frame.render_widget(body_text, body_inner);

    // Position cursor in body
    if body_inner.width > 0 && body_inner.height > 0 {
        let cursor_col = draft.body_cursor.min(draft.body.len()) as u16;
        let col = body_inner.x + cursor_col.min(body_inner.width.saturating_sub(1));
        frame.set_cursor_position((col, body_inner.y));
    }

    // Hints (2 lines so everything is visible)
    let hint_style = Style::default().fg(Color::DarkGray);
    let hints = vec![
        Line::from(Span::styled(" [Tab] cycle verdict   [Enter] submit ", hint_style)),
        Line::from(Span::styled(" [Esc] close           [Ctrl+D] discard draft ", hint_style)),
    ];
    frame.render_widget(
        Paragraph::new(hints).alignment(Alignment::Center),
        hint_area,
    );
}
