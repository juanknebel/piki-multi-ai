use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::App;
use crate::code_review::{CodeReviewState, ReviewFocus};
use crate::config::Config;
use crate::theme::Theme;
use piki_core::github::{DiffLineType, ParsedDiff, ReviewVerdict};

/// Render the full-screen code review layout
pub(super) fn render_fullscreen(frame: &mut Frame, area: Rect, app: &mut App) {
    // Pre-compute and cache layout geometry for mouse hit-testing (before borrowing state).
    {
        let [_, body_area, _] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);
        let split_pct = app.code_review_split_pct;
        let [files_area, _] = Layout::horizontal([
            Constraint::Percentage(split_pct),
            Constraint::Percentage(100 - split_pct),
        ])
        .areas(body_area);
        app.code_review_divider_x = files_area.x + files_area.width;
        app.code_review_body_rect = body_area;
    }

    let ws = match app.current_workspace() {
        Some(ws) => ws,
        None => return,
    };
    let state = match &ws.code_review {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" Code Review ")
                .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.palette.line));
            let text = Paragraph::new("  Loading PR data...")
                .style(Style::default().fg(app.theme.palette.fg3))
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

    render_pr_header(frame, header_area, state, &app.theme);

    // Body: file list | diff (split ratio from app state)
    let split_pct = app.code_review_split_pct;
    let [files_area, diff_area] = Layout::horizontal([
        Constraint::Percentage(split_pct),
        Constraint::Percentage(100 - split_pct),
    ])
    .areas(body_area);

    render_file_list(frame, files_area, state, &app.theme);
    render_diff(frame, diff_area, state, &app.syntax, &app.theme);
    render_footer(frame, footer_area, state, &app.config, &app.theme);

    // Comment input overlay (on top of everything)
    if state.editing_comment.is_some() {
        render_comment_input_overlay(frame, area, state, &app.theme);
    }
    if state.confirm_close {
        render_confirm_close_overlay(frame, area, &app.theme);
    }
}

/// Confirm-before-discard overlay: `q` on an ephemeral review workspace
/// deletes its checkout from disk, so we ask before doing it.
fn render_confirm_close_overlay(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup_width = 54u16.min(area.width.saturating_sub(4));
    let popup_height = 7u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Discard Review ")
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.palette.err));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Discard this review? The PR checkout will be",
            Style::default().fg(theme.palette.fg0),
        )),
        Line::from(Span::styled(
            "  deleted from disk — this cannot be undone.",
            Style::default().fg(theme.palette.fg0),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  [y] Yes, delete it    [n] No, keep reviewing",
            Style::default().fg(theme.palette.fg2),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Render PR info header bar
fn render_pr_header(frame: &mut Frame, area: Rect, state: &CodeReviewState, theme: &Theme) {
    let pr = &state.pr_info;
    let state_color = if pr.state == "OPEN" {
        theme.palette.ok
    } else {
        theme.palette.err
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" PR #{}: ", pr.number),
            Style::default()
                .fg(theme.palette.info)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&pr.title, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("[{}]", pr.state), Style::default().fg(state_color)),
        Span::raw("  "),
        Span::styled(
            format!("{} ← {}", pr.base_ref_name, pr.head_ref_name),
            Style::default().fg(theme.palette.fg2),
        ),
        Span::raw("  "),
        Span::styled(
            format!("+{}", pr.additions),
            Style::default().fg(theme.diff.add),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{}", pr.deletions),
            Style::default().fg(theme.diff.del),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} files", state.files.len()),
            Style::default().fg(theme.palette.fg2),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.palette.line));
    let header = Paragraph::new(line).block(block);
    frame.render_widget(header, area);
}

/// Render the scrollable file list with selection and comment count badges
fn render_file_list(frame: &mut Frame, area: Rect, state: &CodeReviewState, theme: &Theme) {
    let focus_color = if state.focus == ReviewFocus::FileList {
        theme.palette.iris
    } else {
        theme.palette.line
    };

    let block = Block::default()
        .title(" Changed Files ")
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(focus_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.files.is_empty() {
        let text =
            Paragraph::new("  No files changed").style(Style::default().fg(theme.palette.fg3));
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
                    .fg(theme.palette.fg0)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.palette.fg1)
            };

            let mut spans = vec![
                Span::styled(format!("{}{} ", indicator, cached_marker), style),
                Span::styled(&file.path, style),
                Span::raw(" "),
                Span::styled(
                    format!("+{}", file.additions),
                    Style::default().fg(theme.diff.add),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", file.deletions),
                    Style::default().fg(theme.diff.del),
                ),
            ];

            if comment_count > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[{}]", comment_count),
                    Style::default().fg(theme.diff.comment),
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
    CommentHeader { diff_idx: usize, side: CommentSide },
    /// Comment box body on one side — `msg_idx` indexes into
    /// `CodeReviewState::thread_lines()` for this diff line/side.
    CommentBody { diff_idx: usize, side: CommentSide, msg_idx: usize },
    /// Comment box footer on one side.
    CommentFooter { diff_idx: usize, side: CommentSide },
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
    state: &CodeReviewState,
    file_path: &str,
) -> Vec<SplitRow> {
    let mut rows: Vec<SplitRow> = Vec::new();
    let mut del_buf: Vec<usize> = Vec::new(); // indices of consecutive deletions

    let lines = &diff.lines;

    // Flush pending deletions as left-only rows (right=None), with comment decorations.
    let flush_dels = |del_buf: &mut Vec<usize>,
                      rows: &mut Vec<SplitRow>,
                      state: &CodeReviewState,
                      file_path: &str| {
        for &di in del_buf.iter() {
            rows.push(SplitRow::Paired {
                left: Some(di),
                right: None,
            });
            append_comment_decorations(rows, diff, state, file_path, di);
        }
        del_buf.clear();
    };

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        match line.line_type {
            DiffLineType::FileHeader | DiffLineType::HunkHeader => {
                flush_dels(&mut del_buf, &mut rows, state, file_path);
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
                    append_comment_decorations(&mut rows, diff, state, file_path, i);
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
                            append_comment_decorations(&mut rows, diff, state, file_path, li);
                        }
                        if let Some(ri) = right {
                            append_comment_decorations(&mut rows, diff, state, file_path, ri);
                        }
                    }
                    del_buf.clear();
                    i = j;
                }
            }
            DiffLineType::Context => {
                flush_dels(&mut del_buf, &mut rows, state, file_path);
                rows.push(SplitRow::Paired {
                    left: Some(i),
                    right: Some(i),
                });
                append_comment_decorations(&mut rows, diff, state, file_path, i);
                i += 1;
            }
        }
    }
    // Flush any trailing deletions
    flush_dels(&mut del_buf, &mut rows, state, file_path);
    rows
}

/// Append comment decoration rows (header/body*/footer) for a diff line if
/// it has an existing thread and/or our own draft comment.
fn append_comment_decorations(
    rows: &mut Vec<SplitRow>,
    diff: &ParsedDiff,
    state: &CodeReviewState,
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
    let thread = state.thread_lines(file_path, ln, side_str);
    if thread.is_empty() {
        return;
    }
    rows.push(SplitRow::CommentHeader { diff_idx, side });
    for msg_idx in 0..thread.len() {
        rows.push(SplitRow::CommentBody { diff_idx, side, msg_idx });
    }
    rows.push(SplitRow::CommentFooter { diff_idx, side });
}

/// Render the side-by-side split diff pane with line numbers, cursor, and inline comments
fn render_diff(
    frame: &mut Frame,
    area: Rect,
    state: &CodeReviewState,
    syntax_hl: &crate::syntax::SyntaxHighlighter,
    theme: &Theme,
) {
    let focus_color = if state.focus == ReviewFocus::DiffView {
        theme.palette.iris
    } else {
        theme.palette.line
    };

    let title = if let Some(path) = state.current_file_path() {
        format!(" {} ", path)
    } else {
        " Diff ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(focus_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height == 0 {
        return;
    }

    if let Some(diff) = state.current_diff() {
        let file_path = state.current_file_path().unwrap_or("");
        let file_syntax = syntax_hl.find_syntax(file_path);
        let split_rows = compute_split_rows(diff, state, file_path);
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
                    let is_cursor =
                        state.focus == ReviewFocus::DiffView && state.cursor_line == *diff_idx;

                    let style = match diff_line.line_type {
                        DiffLineType::HunkHeader => Style::default()
                            .fg(theme.diff.hunk)
                            .add_modifier(Modifier::BOLD),
                        DiffLineType::FileHeader => Style::default()
                            .fg(theme.palette.fg0)
                            .add_modifier(Modifier::BOLD),
                        _ => Style::default().fg(theme.palette.fg1),
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
                        syntax_hl,
                        file_syntax,
                        theme,
                    );

                    // Vertical separator
                    let sep_x = inner.x + left_half;
                    if sep_x < inner.x + inner.width {
                        buf_set_string_clipped(
                            frame.buffer_mut(),
                            sep_x,
                            y,
                            "\u{2502}",
                            Style::default().fg(theme.palette.line),
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
                        syntax_hl,
                        file_syntax,
                        theme,
                    );
                }
                SplitRow::CommentHeader { diff_idx, side } => {
                    render_comment_decoration_row(
                        frame,
                        inner,
                        y,
                        left_half,
                        right_half,
                        diff,
                        state,
                        file_path,
                        *diff_idx,
                        *side,
                        CommentDecorPart::Header,
                        theme,
                    );
                }
                SplitRow::CommentBody { diff_idx, side, msg_idx } => {
                    render_comment_decoration_row(
                        frame,
                        inner,
                        y,
                        left_half,
                        right_half,
                        diff,
                        state,
                        file_path,
                        *diff_idx,
                        *side,
                        CommentDecorPart::Body(*msg_idx),
                        theme,
                    );
                }
                SplitRow::CommentFooter { diff_idx, side } => {
                    render_comment_decoration_row(
                        frame,
                        inner,
                        y,
                        left_half,
                        right_half,
                        diff,
                        state,
                        file_path,
                        *diff_idx,
                        *side,
                        CommentDecorPart::Footer,
                        theme,
                    );
                }
            }
        }
    } else if state.loading {
        let text =
            Paragraph::new("  Loading diff...").style(Style::default().fg(theme.palette.fg3));
        frame.render_widget(text, inner);
    } else {
        let text = Paragraph::new("  Select a file and press Enter to view diff")
            .style(Style::default().fg(theme.palette.fg3));
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
    syntax_hl: &crate::syntax::SyntaxHighlighter,
    file_syntax: Option<&syntect::parsing::SyntaxReference>,
    theme: &Theme,
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
                    .fg(theme.palette.fg3)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme.palette.fg3)
            };

            buf_set_string_clipped(frame.buffer_mut(), x, y, &ln_str, gutter_style, gutter);

            // Content: determine base style and prefix from diff line type
            let (base_style, prefix) = match diff_line.line_type {
                DiffLineType::Addition => (
                    Style::default().fg(theme.diff.add).bg(theme.diff.add_bg),
                    "+",
                ),
                DiffLineType::Deletion => (
                    Style::default().fg(theme.diff.del).bg(theme.diff.del_bg),
                    "-",
                ),
                DiffLineType::Context => (Style::default().fg(theme.diff.context), " "),
                _ => (Style::default().fg(theme.diff.context), " "),
            };

            let base_style = if is_cursor {
                base_style.add_modifier(Modifier::REVERSED)
            } else {
                base_style
            };

            // Write the prefix character with the base diff style
            buf_set_string_clipped(frame.buffer_mut(), x + gutter, y, prefix, base_style, 1);

            // Syntax-highlight the content portion (after removing +/- prefix)
            let content_start = x + gutter + 1;
            let code_width = content_width.saturating_sub(1);
            let used = if let Some(syntax) = file_syntax {
                let mut hl = syntax_hl.highlighter_for(syntax);
                let spans = syntax_hl.highlight_line(&mut hl, &diff_line.content, base_style);
                // When cursor is active, apply REVERSED modifier to all spans
                let spans: Vec<Span<'static>> = if is_cursor {
                    spans
                        .into_iter()
                        .map(|s| {
                            Span::styled(
                                s.content.into_owned(),
                                s.style.add_modifier(Modifier::REVERSED),
                            )
                        })
                        .collect()
                } else {
                    spans
                };
                buf_set_spans_clipped(frame.buffer_mut(), content_start, y, &spans, code_width)
            } else {
                // Fallback: no syntax highlighting
                let display = &diff_line.content;
                buf_set_string_clipped(
                    frame.buffer_mut(),
                    content_start,
                    y,
                    display,
                    base_style,
                    code_width,
                );
                display.chars().count().min(code_width as usize) as u16
            };

            // Fill remaining for cursor highlight
            if is_cursor && used < code_width {
                let pad = " ".repeat((code_width - used) as usize);
                buf_set_string_clipped(
                    frame.buffer_mut(),
                    content_start + used,
                    y,
                    &pad,
                    base_style,
                    code_width - used,
                );
            }
        }
        None => {
            // Empty filler — dim fill
            let fill_style = if is_cursor {
                Style::default()
                    .fg(theme.palette.fg3)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme.palette.fg3)
            };
            let fill = " ".repeat(half_width as usize);
            buf_set_string_clipped(frame.buffer_mut(), x, y, &fill, fill_style, half_width);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CommentDecorPart {
    Header,
    /// Index into `CodeReviewState::thread_lines()` for this diff line/side.
    Body(usize),
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
    state: &CodeReviewState,
    file_path: &str,
    diff_idx: usize,
    side: CommentSide,
    part: CommentDecorPart,
    theme: &Theme,
) {
    let diff_line = &diff.lines[diff_idx];
    let (ln, side_str) = match side {
        CommentSide::Left => (diff_line.old_line.unwrap_or(0), "LEFT"),
        CommentSide::Right => (diff_line.new_line.unwrap_or(0), "RIGHT"),
    };

    let comment_style = Style::default().fg(theme.diff.comment);

    // Determine the x offset and available width for this side
    let (side_x, side_width) = match side {
        CommentSide::Left => (inner.x, left_half),
        CommentSide::Right => (inner.x + left_half + 1, right_half.saturating_sub(1)),
    };

    let text = match part {
        CommentDecorPart::Header => {
            format!("  \u{250c}\u{2500}\u{2500} line {} \u{2500}\u{2500}", ln)
        }
        CommentDecorPart::Body(msg_idx) => {
            let lines = state.thread_lines(file_path, ln, side_str);
            let body = lines.get(msg_idx).map(String::as_str).unwrap_or("");
            format!("  \u{2502} {}", body)
        }
        CommentDecorPart::Footer => {
            "  \u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string()
        }
    };

    buf_set_string_clipped(
        frame.buffer_mut(),
        side_x,
        y,
        &text,
        comment_style,
        side_width,
    );

    // Draw vertical separator on comment rows too
    if left_half < inner.width {
        buf_set_string_clipped(
            frame.buffer_mut(),
            inner.x + left_half,
            y,
            "\u{2502}",
            Style::default().fg(theme.palette.line),
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

/// Helper: write syntax-highlighted spans to the buffer, clipped to max_width chars.
/// Returns the number of characters written.
fn buf_set_spans_clipped(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    y: u16,
    spans: &[Span<'_>],
    max_width: u16,
) -> u16 {
    if max_width == 0 {
        return 0;
    }
    let mut offset: u16 = 0;
    for span in spans {
        if offset >= max_width {
            break;
        }
        let remaining = (max_width - offset) as usize;
        let text: String = span.content.chars().take(remaining).collect();
        let char_count = text.chars().count() as u16;
        buf.set_string(x + offset, y, &text, span.style);
        offset += char_count;
    }
    offset
}

/// Render context-sensitive footer keybindings
fn render_footer(
    frame: &mut Frame,
    area: Rect,
    state: &CodeReviewState,
    config: &Config,
    theme: &Theme,
) {
    let keys = if state.editing_comment.is_some() {
        "[Enter] save comment  [Esc] cancel".to_string()
    } else if state.confirm_close {
        "[y] delete checkout  [n] keep reviewing".to_string()
    } else if state.show_submit {
        format!(
            "[Tab] cycle verdict  [Enter] submit  [Esc] close  [{}] discard",
            config.format_binding("ctrl-d"),
        )
    } else {
        match state.focus {
            ReviewFocus::FileList => {
                "[j/k] navigate  [Enter] view diff  [l] diff pane  [s] submit  [r] refresh  [q] close  [[/]] resize".to_string()
            }
            ReviewFocus::DiffView => {
                format!(
                    "[j/k] cursor  [{}/{}] page  [g/G] top/bottom  [c] comment  [R] reply  [d] del comment  [h] files  [n/p] next/prev  [s] submit  [q] close",
                    config.format_binding("ctrl-d"),
                    config.format_binding("ctrl-u"),
                )
            }
        }
    };

    let line = Line::from(Span::styled(
        format!(" {} ", keys),
        Style::default().fg(theme.palette.fg3),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

/// Render the comment input overlay
fn render_comment_input_overlay(
    frame: &mut Frame,
    area: Rect,
    state: &CodeReviewState,
    theme: &Theme,
) {
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

    let title = match ec.target {
        crate::code_review::CommentTarget::NewInline => format!(" Comment on line {} ", ec.line),
        crate::code_review::CommentTarget::Reply { .. } => format!(" Reply on line {} ", ec.line),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.palette.iris));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Body text
    let [body_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let body_text = if ec.body.is_empty() {
        Paragraph::new("Type comment...").style(Style::default().fg(theme.palette.fg3))
    } else {
        Paragraph::new(ec.body.as_str()).style(Style::default().fg(theme.palette.fg0))
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
        Style::default().fg(theme.palette.fg3),
    ));
    frame.render_widget(
        Paragraph::new(hints).alignment(Alignment::Center),
        hint_area,
    );
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

    let theme = &app.theme;
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
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.palette.line_strong));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let has_error = state.submit_error.is_some();
    let [
        _gap1,
        verdict_area,
        comments_area,
        error_area,
        _gap2,
        body_area,
        hint_area,
    ] = Layout::vertical([
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
                let verdict_color = match v {
                    ReviewVerdict::Approve => theme.palette.ok,
                    ReviewVerdict::RequestChanges => theme.palette.err,
                    ReviewVerdict::Comment => theme.palette.info,
                };
                Style::default()
                    .fg(verdict_color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.palette.fg1)
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
            format!(
                "  {} inline comment{}",
                comment_count,
                if comment_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(theme.diff.comment),
        )
    } else {
        Span::styled(
            "  No inline comments",
            Style::default().fg(theme.palette.fg3),
        )
    };
    frame.render_widget(Paragraph::new(Line::from(comments_text)), comments_area);

    // Error message (if any)
    if let Some(ref err) = state.submit_error {
        let err_text: String = err
            .chars()
            .take(inner.width.saturating_sub(2) as usize)
            .collect();
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", err_text),
                Style::default()
                    .fg(theme.palette.err)
                    .add_modifier(Modifier::BOLD),
            ))),
            error_area,
        );
    }

    // Body input area
    let body_block = Block::default()
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.palette.iris));
    let body_inner = body_block.inner(body_area);
    frame.render_widget(body_block, body_area);

    let body_text = if draft.body.is_empty() {
        Paragraph::new("Type review comment...").style(Style::default().fg(theme.palette.fg3))
    } else {
        Paragraph::new(draft.body.as_str()).style(Style::default().fg(theme.palette.fg0))
    };
    frame.render_widget(body_text, body_inner);

    // Position cursor in body
    if body_inner.width > 0 && body_inner.height > 0 {
        let cursor_col = draft.body_cursor.min(draft.body.len()) as u16;
        let col = body_inner.x + cursor_col.min(body_inner.width.saturating_sub(1));
        frame.set_cursor_position((col, body_inner.y));
    }

    // Hints (2 lines so everything is visible)
    let hint_style = Style::default().fg(theme.palette.fg3);
    let hints = vec![
        Line::from(Span::styled(
            " [Tab] cycle verdict   [Enter] submit ",
            hint_style,
        )),
        Line::from(Span::styled(
            format!(
                " [Esc] close           [{}] discard draft ",
                app.config.format_binding("ctrl-d"),
            ),
            hint_style,
        )),
    ];
    frame.render_widget(
        Paragraph::new(hints).alignment(Alignment::Center),
        hint_area,
    );
}
