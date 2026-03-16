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

/// Which side a comment decoration belongs to in the split view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentSide {
    Left,
    Right,
}

/// A single row in the side-by-side split diff view.
#[derive(Debug, Clone)]
pub(crate) enum SplitRow {
    /// File or hunk header — rendered full width.
    FullWidth { diff_idx: usize },
    /// Paired content row: left (old file) and right (new file).
    /// Each side is `Some(diff_line_index)` or `None` (blank filler).
    Paired {
        left: Option<usize>,
        right: Option<usize>,
    },
    /// Comment box header on one side.
    CommentHeader {
        diff_idx: usize,
        side: CommentSide,
    },
    /// Comment box body on one side.
    CommentBody {
        diff_idx: usize,
        side: CommentSide,
    },
    /// Comment box footer on one side.
    CommentFooter {
        diff_idx: usize,
        side: CommentSide,
    },
}

impl SplitRow {
    /// Returns true if this row references the given diff line index.
    pub(crate) fn contains_diff_idx(&self, idx: usize) -> bool {
        match self {
            SplitRow::FullWidth { diff_idx } => *diff_idx == idx,
            SplitRow::Paired { left, right } => {
                left.is_some_and(|i| i == idx) || right.is_some_and(|i| i == idx)
            }
            SplitRow::CommentHeader { diff_idx, .. }
            | SplitRow::CommentBody { diff_idx, .. }
            | SplitRow::CommentFooter { diff_idx, .. } => *diff_idx == idx,
        }
    }
}

/// Build the list of split rows from a parsed diff, pairing deletions with additions.
pub(crate) fn compute_split_rows(
    diff: &ParsedDiff,
    draft: &super::super::code_review::ReviewDraft,
    file_path: &str,
) -> Vec<SplitRow> {
    let mut rows: Vec<SplitRow> = Vec::new();
    let mut del_buf: Vec<usize> = Vec::new(); // indices of consecutive deletions

    let lines = &diff.lines;

    // Flush pending deletions as left-only rows (right=None), with comment decorations.
    let flush_dels = |del_buf: &mut Vec<usize>,
                      rows: &mut Vec<SplitRow>,
                      draft: &super::super::code_review::ReviewDraft,
                      file_path: &str| {
        for &di in del_buf.iter() {
            rows.push(SplitRow::Paired {
                left: Some(di),
                right: None,
            });
            append_comment_decorations(rows, diff, draft, file_path, di);
        }
        del_buf.clear();
    };

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        match line.line_type {
            DiffLineType::FileHeader | DiffLineType::HunkHeader => {
                flush_dels(&mut del_buf, &mut rows, draft, file_path);
                rows.push(SplitRow::FullWidth { diff_idx: i });
                i += 1;
            }
            DiffLineType::Deletion => {
                del_buf.push(i);
                i += 1;
            }
            DiffLineType::Addition => {
                if del_buf.is_empty() {
                    // Addition with no preceding deletion → right-only
                    rows.push(SplitRow::Paired {
                        left: None,
                        right: Some(i),
                    });
                    append_comment_decorations(&mut rows, diff, draft, file_path, i);
                    i += 1;
                } else {
                    // Pair deletions with additions 1:1
                    let mut add_buf: Vec<usize> = Vec::new();
                    let mut j = i;
                    while j < lines.len() && lines[j].line_type == DiffLineType::Addition {
                        add_buf.push(j);
                        j += 1;
                    }
                    let max_len = del_buf.len().max(add_buf.len());
                    for k in 0..max_len {
                        let left = del_buf.get(k).copied();
                        let right = add_buf.get(k).copied();
                        rows.push(SplitRow::Paired { left, right });
                        // Append comment decorations for whichever side(s) exist
                        if let Some(li) = left {
                            append_comment_decorations(&mut rows, diff, draft, file_path, li);
                        }
                        if let Some(ri) = right {
                            append_comment_decorations(&mut rows, diff, draft, file_path, ri);
                        }
                    }
                    del_buf.clear();
                    i = j;
                }
            }
            DiffLineType::Context => {
                flush_dels(&mut del_buf, &mut rows, draft, file_path);
                rows.push(SplitRow::Paired {
                    left: Some(i),
                    right: Some(i),
                });
                append_comment_decorations(&mut rows, diff, draft, file_path, i);
                i += 1;
            }
        }
    }
    // Flush any trailing deletions
    flush_dels(&mut del_buf, &mut rows, draft, file_path);
    rows
}

/// Append comment decoration rows (header/body/footer) for a diff line if it has a comment.
fn append_comment_decorations(
    rows: &mut Vec<SplitRow>,
    diff: &ParsedDiff,
    draft: &super::super::code_review::ReviewDraft,
    file_path: &str,
    diff_idx: usize,
) {
    let line = &diff.lines[diff_idx];
    let (ln, side_str, side) = match line.line_type {
        DiffLineType::Deletion => {
            if let Some(ln) = line.old_line {
                (ln, "LEFT", CommentSide::Left)
            } else {
                return;
            }
        }
        DiffLineType::Addition | DiffLineType::Context => {
            if let Some(ln) = line.new_line {
                (ln, "RIGHT", CommentSide::Right)
            } else {
                return;
            }
        }
        _ => return,
    };
    if draft
        .comment_at_line_and_side(file_path, ln, side_str)
        .is_some()
    {
        rows.push(SplitRow::CommentHeader {
            diff_idx,
            side,
        });
        rows.push(SplitRow::CommentBody {
            diff_idx,
            side,
        });
        rows.push(SplitRow::CommentFooter {
            diff_idx,
            side,
        });
    }
}

/// Render the side-by-side split diff pane with line numbers, cursor, and inline comments
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

    if inner.width < 4 || inner.height == 0 {
        return;
    }

    if let Some(diff) = state.current_diff() {
        let file_path = state.current_file_path().unwrap_or("");
        let split_rows = compute_split_rows(diff, &state.draft, file_path);
        let viewport_height = inner.height as usize;
        let scroll = state.diff_scroll;

        let left_half = inner.width / 2;
        let right_half = inner.width - left_half;
        let gutter: u16 = 7; // " %4d |" = 7 chars

        for (row_offset, srow) in split_rows
            .iter()
            .skip(scroll)
            .take(viewport_height)
            .enumerate()
        {
            let y = inner.y + row_offset as u16;

            match srow {
                SplitRow::FullWidth { diff_idx } => {
                    let diff_line = &diff.lines[*diff_idx];
                    let is_cursor = state.focus == ReviewFocus::DiffView
                        && state.cursor_line == *diff_idx;

                    let style = match diff_line.line_type {
                        DiffLineType::HunkHeader => Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                        DiffLineType::FileHeader => Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                        _ => Style::default().fg(Color::Gray),
                    };
                    let style = if is_cursor {
                        style.add_modifier(Modifier::REVERSED)
                    } else {
                        style
                    };

                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        inner.x,
                        y,
                        &diff_line.content,
                        style,
                        inner.width,
                    );
                    // Fill remaining width for cursor highlight
                    if is_cursor {
                        let used = diff_line.content.chars().count().min(inner.width as usize);
                        if used < inner.width as usize {
                            let pad = " ".repeat(inner.width as usize - used);
                            buf_set_string_clipped(
                                frame.buffer_mut(),
                                inner.x + used as u16,
                                y,
                                &pad,
                                style,
                                (inner.width as usize - used) as u16,
                            );
                        }
                    }
                }
                SplitRow::Paired { left, right } => {
                    let is_cursor = state.focus == ReviewFocus::DiffView
                        && (left.is_some_and(|i| i == state.cursor_line)
                            || right.is_some_and(|i| i == state.cursor_line));

                    // Left half
                    render_half_line(
                        frame,
                        inner.x,
                        y,
                        left_half,
                        gutter,
                        *left,
                        diff,
                        is_cursor,
                        true,
                    );

                    // Vertical separator
                    let sep_x = inner.x + left_half;
                    if sep_x < inner.x + inner.width {
                        buf_set_string_clipped(
                            frame.buffer_mut(),
                            sep_x,
                            y,
                            "\u{2502}",
                            Style::default().fg(Color::DarkGray),
                            1,
                        );
                    }

                    // Right half (starts after separator, so we use right_half - 1 if separator takes space)
                    let right_start = inner.x + left_half + 1;
                    let right_width = right_half.saturating_sub(1);
                    render_half_line(
                        frame,
                        right_start,
                        y,
                        right_width,
                        gutter,
                        *right,
                        diff,
                        is_cursor,
                        false,
                    );
                }
                SplitRow::CommentHeader { diff_idx, side } => {
                    render_comment_decoration_row(
                        frame, inner, y, left_half, right_half, diff, &state.draft, file_path,
                        *diff_idx, *side, CommentDecorPart::Header,
                    );
                }
                SplitRow::CommentBody { diff_idx, side } => {
                    render_comment_decoration_row(
                        frame, inner, y, left_half, right_half, diff, &state.draft, file_path,
                        *diff_idx, *side, CommentDecorPart::Body,
                    );
                }
                SplitRow::CommentFooter { diff_idx, side } => {
                    render_comment_decoration_row(
                        frame, inner, y, left_half, right_half, diff, &state.draft, file_path,
                        *diff_idx, *side, CommentDecorPart::Footer,
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

/// Render one half (left or right) of a paired split row.
#[allow(clippy::too_many_arguments)]
fn render_half_line(
    frame: &mut Frame,
    x: u16,
    y: u16,
    half_width: u16,
    gutter: u16,
    line_idx: Option<usize>,
    diff: &ParsedDiff,
    is_cursor: bool,
    is_left: bool,
) {
    if half_width < gutter + 1 {
        return;
    }
    let content_width = half_width.saturating_sub(gutter);

    match line_idx {
        Some(idx) => {
            let diff_line = &diff.lines[idx];

            // Gutter: line number
            let ln = if is_left {
                diff_line.old_line
            } else {
                diff_line.new_line
            };
            let ln_str = ln
                .map(|n| format!(" {:>4}\u{2502}", n))
                .unwrap_or_else(|| "     \u{2502}".to_string());

            let gutter_style = if is_cursor {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            buf_set_string_clipped(frame.buffer_mut(), x, y, &ln_str, gutter_style, gutter);

            // Content
            let (content_style, prefix) = match diff_line.line_type {
                DiffLineType::Addition => (Style::default().fg(Color::Green), "+"),
                DiffLineType::Deletion => (Style::default().fg(Color::Red), "-"),
                DiffLineType::Context => (Style::default().fg(Color::Gray), " "),
                _ => (Style::default().fg(Color::Gray), " "),
            };

            let content_style = if is_cursor {
                content_style.add_modifier(Modifier::REVERSED)
            } else {
                content_style
            };

            let display = format!("{}{}", prefix, diff_line.content);
            buf_set_string_clipped(
                frame.buffer_mut(),
                x + gutter,
                y,
                &display,
                content_style,
                content_width,
            );

            // Fill remaining for cursor highlight
            if is_cursor {
                let used = display.chars().count().min(content_width as usize);
                if used < content_width as usize {
                    let pad = " ".repeat(content_width as usize - used);
                    buf_set_string_clipped(
                        frame.buffer_mut(),
                        x + gutter + used as u16,
                        y,
                        &pad,
                        content_style,
                        (content_width as usize - used) as u16,
                    );
                }
            }
        }
        None => {
            // Empty filler — dim fill
            let fill_style = if is_cursor {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let fill = " ".repeat(half_width as usize);
            buf_set_string_clipped(frame.buffer_mut(), x, y, &fill, fill_style, half_width);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CommentDecorPart {
    Header,
    Body,
    Footer,
}

/// Render a comment decoration row (header/body/footer) on the appropriate side.
#[allow(clippy::too_many_arguments)]
fn render_comment_decoration_row(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    left_half: u16,
    right_half: u16,
    diff: &ParsedDiff,
    draft: &super::super::code_review::ReviewDraft,
    file_path: &str,
    diff_idx: usize,
    side: CommentSide,
    part: CommentDecorPart,
) {
    let diff_line = &diff.lines[diff_idx];
    let (ln, side_str) = match side {
        CommentSide::Left => (
            diff_line.old_line.unwrap_or(0),
            "LEFT",
        ),
        CommentSide::Right => (
            diff_line.new_line.unwrap_or(0),
            "RIGHT",
        ),
    };

    let comment_style = Style::default().fg(Color::Yellow);

    // Determine the x offset and available width for this side
    let (side_x, side_width) = match side {
        CommentSide::Left => (inner.x, left_half),
        CommentSide::Right => (inner.x + left_half + 1, right_half.saturating_sub(1)),
    };

    let text = match part {
        CommentDecorPart::Header => {
            format!("  \u{250c}\u{2500}\u{2500} line {} \u{2500}\u{2500}", ln)
        }
        CommentDecorPart::Body => {
            let body = draft
                .comment_at_line_and_side(file_path, ln, side_str)
                .map(|c| c.body.as_str())
                .unwrap_or("");
            format!("  \u{2502} {}", body)
        }
        CommentDecorPart::Footer => {
            "  \u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string()
        }
    };

    buf_set_string_clipped(frame.buffer_mut(), side_x, y, &text, comment_style, side_width);

    // Draw vertical separator on comment rows too
    if left_half < inner.width {
        buf_set_string_clipped(
            frame.buffer_mut(),
            inner.x + left_half,
            y,
            "\u{2502}",
            Style::default().fg(Color::DarkGray),
            1,
        );
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
