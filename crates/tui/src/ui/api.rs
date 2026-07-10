use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{ApiResponseDisplay, ApiTabState, Selection};
use crate::theme::{Palette, Theme};

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Render the API Explorer tab. Returns the inner area of the response panel
/// (for mouse hit-testing), or `None` if no response is displayed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    frame: &mut Frame,
    area: Rect,
    api: &ApiTabState,
    theme: &Theme,
    border_style: Style,
    title_style: Style,
    selection: Option<&Selection>,
    selection_style: Style,
) -> Option<Rect> {
    let p = &theme.palette;
    let has_response = !api.responses.is_empty() || api.loading;

    let chunks = if has_response {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area)
    } else {
        Layout::vertical([Constraint::Min(0), Constraint::Length(0)]).split(area)
    };

    // ── Editor pane ──
    render_editor(frame, chunks[0], api, p, border_style, title_style);

    // ── Response pane ──
    let result = if has_response {
        render_responses(
            frame,
            chunks[1],
            api,
            p,
            border_style,
            title_style,
            selection,
            selection_style,
        )
    } else {
        None
    };

    // ── History overlay ──
    if let Some(ref hist) = api.history {
        render_history_overlay(frame, area, hist, p);
    }

    result
}

fn render_editor(
    frame: &mut Frame,
    area: Rect,
    api: &ApiTabState,
    p: &Palette,
    border_style: Style,
    title_style: Style,
) {
    let block = Block::default()
        .title(" API Explorer ")
        .title_style(title_style)
        .title_bottom(
            Line::from(Span::styled(
                " [^S send | ^H history] ",
                Style::default().fg(p.fg3),
            ))
            .right_aligned(),
        )
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let editor = &api.editor;
    let visible_height = inner.height as usize;
    let line_num_width = format!("{}", editor.lines.len()).len();

    let lines: Vec<Line> = editor
        .lines
        .iter()
        .enumerate()
        .skip(editor.scroll_offset)
        .take(visible_height)
        .map(|(row, line_text)| {
            let line_num = format!("{:>width$} ", row + 1, width = line_num_width);
            let mut spans = vec![Span::styled(line_num, Style::default().fg(p.fg2))];

            if row == editor.cursor_row {
                let cursor_byte = char_to_byte_idx(line_text, editor.cursor_col);
                if cursor_byte >= line_text.len() {
                    spans.push(Span::raw(line_text.clone()));
                    spans.push(Span::styled(" ", Style::default().bg(p.fg0).fg(p.bg0)));
                } else {
                    let next_byte = char_to_byte_idx(line_text, editor.cursor_col + 1);
                    if cursor_byte > 0 {
                        spans.push(Span::raw(line_text[..cursor_byte].to_string()));
                    }
                    spans.push(Span::styled(
                        line_text[cursor_byte..next_byte].to_string(),
                        Style::default().bg(p.fg0).fg(p.bg0),
                    ));
                    if next_byte < line_text.len() {
                        spans.push(Span::raw(line_text[next_byte..].to_string()));
                    }
                }
            } else {
                spans.push(Span::raw(line_text.clone()));
            }

            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

#[allow(clippy::too_many_arguments)]
fn render_responses(
    frame: &mut Frame,
    area: Rect,
    api: &ApiTabState,
    p: &Palette,
    border_style: Style,
    title_style: Style,
    selection: Option<&Selection>,
    selection_style: Style,
) -> Option<Rect> {
    if api.loading {
        let block = Block::default()
            .title(" Response ")
            .title_style(title_style)
            .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style);
        let text = Paragraph::new("  Sending...")
            .style(Style::default().fg(p.warn))
            .block(block);
        frame.render_widget(text, area);
        return None;
    }

    if api.responses.is_empty() {
        return None;
    }

    // Build all response lines with separators between responses
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    let total = api.responses.len();

    for (idx, resp) in api.responses.iter().enumerate() {
        // Response header line
        let status_color = status_color(resp.status, p);
        let header = if resp.status == 0 {
            format!("── Response #{} (error) — {}ms ", idx + 1, resp.elapsed_ms)
        } else if total == 1 {
            format!("── {} — {}ms ", resp.status, resp.elapsed_ms)
        } else {
            format!("── #{} ({}) — {}ms ", idx + 1, resp.status, resp.elapsed_ms)
        };
        all_lines.push(Line::from(Span::styled(
            header,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        )));

        // Colorized body
        all_lines.extend(colorize_json(&resp.body, p));

        // Separator between responses
        if idx + 1 < total {
            all_lines.push(Line::from(""));
        }
    }

    // Title summary
    let title = if total == 1 {
        response_title(&api.responses[0])
    } else {
        let all_ok = api.responses.iter().all(|r| (200..300).contains(&r.status));
        let total_ms: u128 = api.responses.iter().map(|r| r.elapsed_ms).sum();
        if all_ok {
            format!(" {} responses (all OK) — {}ms ", total, total_ms)
        } else {
            let errors = api
                .responses
                .iter()
                .filter(|r| !(200..300).contains(&r.status))
                .count();
            format!(" {} responses ({} errors) — {}ms ", total, errors, total_ms)
        }
    };

    let title_color = if api.responses.iter().all(|r| (200..300).contains(&r.status)) {
        p.ok
    } else if api
        .responses
        .iter()
        .any(|r| r.status == 0 || r.status >= 500)
    {
        p.err
    } else {
        p.warn
    };

    let help_hint = if api.search.is_some() {
        " [^F search] "
    } else {
        " [^J/^K scroll | ^F search] "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(title_color)))
        .title_bottom(
            Line::from(Span::styled(help_hint, Style::default().fg(p.fg3))).right_aligned(),
        )
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    let text = Paragraph::new(all_lines)
        .block(block)
        .scroll((api.response_scroll, 0));
    frame.render_widget(text, area);

    // Render selection highlight overlay
    if let Some(sel) = selection {
        let (start_row, start_col, end_row, end_col) = sel.normalized();
        let first_row = start_row.min(inner.height.saturating_sub(1));
        let last_row = end_row.min(inner.height.saturating_sub(1));
        let buf = frame.buffer_mut();
        for row in first_row..=last_row {
            let col_start = if row == start_row { start_col } else { 0 };
            let col_end = if row == end_row {
                end_col.min(inner.width.saturating_sub(1))
            } else {
                inner.width.saturating_sub(1)
            };
            for col in col_start..=col_end {
                let x = inner.x + col;
                let y = inner.y + row;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(selection_style);
                }
            }
        }
    }

    // Render search highlights and search bar
    if let Some(ref search) = api.search {
        let query_len = search.query.chars().count();
        if query_len > 0 {
            let match_style = Style::default().bg(p.iris_wash).fg(p.fg0);
            let current_style = Style::default().bg(p.iris).fg(p.bg0);
            let scroll = api.response_scroll as usize;
            let visible_height = inner.height as usize;
            let buf = frame.buffer_mut();

            for (idx, &(line, col)) in search.matches.iter().enumerate() {
                if line < scroll || line >= scroll + visible_height {
                    continue;
                }
                let style = if idx == search.current_match {
                    current_style
                } else {
                    match_style
                };
                let screen_row = (line - scroll) as u16;
                for offset in 0..query_len {
                    let c = (col + offset) as u16;
                    if c >= inner.width {
                        break;
                    }
                    let x = inner.x + c;
                    let y = inner.y + screen_row;
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(style);
                    }
                }
            }
        }

        // Search input bar at the bottom of the response area
        let bar_y = area.y + area.height.saturating_sub(2);
        let bar_area = Rect::new(area.x + 1, bar_y, inner.width, 1);
        frame.render_widget(Clear, bar_area);

        let match_info = if search.matches.is_empty() {
            if search.query.is_empty() {
                String::new()
            } else {
                " (no matches)".to_string()
            }
        } else {
            format!(" {}/{}", search.current_match + 1, search.matches.len())
        };

        let bar = Paragraph::new(Line::from(vec![
            Span::styled(" / ", Style::default().fg(p.iris)),
            Span::raw(&search.query),
            Span::styled(match_info, Style::default().fg(p.fg3)),
        ]));
        frame.render_widget(bar, bar_area);
    }

    Some(inner)
}

fn status_color(status: u16, p: &Palette) -> Color {
    match status {
        200..=299 => p.ok,
        400..=499 => p.warn,
        500..=599 => p.err,
        0 => p.err,
        _ => p.fg0,
    }
}

fn response_title(resp: &ApiResponseDisplay) -> String {
    if resp.status == 0 {
        format!(" Response (error) — {}ms ", resp.elapsed_ms)
    } else {
        format!(" Response ({}) — {}ms ", resp.status, resp.elapsed_ms)
    }
}

// ── JSON syntax colorizer ──

/// Colorize a JSON string line-by-line using a simple state machine.
/// Falls back to plain text if the input is not valid JSON.
fn colorize_json(text: &str, p: &Palette) -> Vec<Line<'static>> {
    if serde_json::from_str::<serde_json::Value>(text).is_err() {
        return text.lines().map(|l| Line::raw(l.to_string())).collect();
    }

    text.lines()
        .map(|line| Line::from(colorize_json_line(line, p)))
        .collect()
}

/// Colorize a single line of pretty-printed JSON.
fn colorize_json_line(line: &str, p: &Palette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Leading whitespace
    let indent_end = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(len);
    if indent_end > 0 {
        spans.push(Span::raw(chars[..indent_end].iter().collect::<String>()));
        i = indent_end;
    }

    while i < len {
        let ch = chars[i];
        match ch {
            '"' => {
                let start = i;
                i += 1;
                while i < len {
                    if chars[i] == '\\' {
                        i += 2;
                    } else if chars[i] == '"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                let s: String = chars[start..i].iter().collect();

                let rest_trimmed: String = chars[i..]
                    .iter()
                    .collect::<String>()
                    .trim_start()
                    .to_string();
                let color = if rest_trimmed.starts_with(':') {
                    p.info
                } else {
                    p.ok
                };
                spans.push(Span::styled(s, Style::default().fg(color)));
            }
            't' | 'f'
                if matches_keyword(&chars[i..], "true")
                    || matches_keyword(&chars[i..], "false") =>
            {
                let kw_len = if chars[i] == 't' { 4 } else { 5 };
                let s: String = chars[i..i + kw_len].iter().collect();
                // Constants take the magenta-family slot of classic JSON
                // highlighting; the accent never colors syntax.
                spans.push(Span::styled(s, Style::default().fg(p.err)));
                i += kw_len;
            }
            'n' if matches_keyword(&chars[i..], "null") => {
                spans.push(Span::styled("null".to_string(), Style::default().fg(p.fg3)));
                i += 4;
            }
            '0'..='9' | '-' => {
                let start = i;
                i += 1;
                while i < len
                    && (chars[i].is_ascii_digit()
                        || chars[i] == '.'
                        || chars[i] == 'e'
                        || chars[i] == 'E'
                        || chars[i] == '+'
                        || chars[i] == '-')
                {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                spans.push(Span::styled(s, Style::default().fg(p.warn)));
            }
            '{' | '}' | '[' | ']' | ':' | ',' => {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(p.fg0)));
                i += 1;
            }
            _ => {
                spans.push(Span::raw(ch.to_string()));
                i += 1;
            }
        }
    }

    spans
}

fn matches_keyword(chars: &[char], keyword: &str) -> bool {
    let kw: Vec<char> = keyword.chars().collect();
    if chars.len() < kw.len() {
        return false;
    }
    chars[..kw.len()] == kw[..] && chars.get(kw.len()).is_none_or(|c| !c.is_alphanumeric())
}

// ── History overlay ──

fn render_history_overlay(
    frame: &mut Frame,
    area: Rect,
    hist: &crate::app::ApiHistoryState,
    p: &Palette,
) {
    // Centered floating panel: 80% width, 70% height
    let overlay_w = (area.width as u32 * 80 / 100).min(area.width as u32) as u16;
    let overlay_h = (area.height as u32 * 70 / 100)
        .min(area.height as u32)
        .max(5) as u16;
    let x = area.x + (area.width.saturating_sub(overlay_w)) / 2;
    let y = area.y + (area.height.saturating_sub(overlay_h)) / 2;
    let overlay_area = Rect::new(x, y, overlay_w, overlay_h);

    frame.render_widget(Clear, overlay_area);

    let title = if hist.searching {
        format!(" API History (/{}) ", hist.search_query)
    } else {
        format!(" API History ({} entries) ", hist.entries.len())
    };

    let block = Block::default()
        .title(title)
        .title_bottom(
            Line::from(Span::styled(
                " [j/k nav | Enter load | d delete | / search | Esc close] ",
                Style::default().fg(p.fg3),
            ))
            .right_aligned(),
        )
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(p.line_strong));

    let inner = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);

    if hist.entries.is_empty() {
        let empty = Paragraph::new("  No history entries").style(Style::default().fg(p.fg3));
        frame.render_widget(empty, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let lines: Vec<Line<'static>> = hist
        .entries
        .iter()
        .enumerate()
        .skip(hist.scroll_offset)
        .take(visible_height)
        .map(|(idx, entry)| {
            let status_color = status_color(entry.status, p);
            let ts = if entry.created_at.len() >= 16 {
                &entry.created_at[..16]
            } else {
                &entry.created_at
            };
            let text = format!(
                " {} {} {} -> {} ({}ms)",
                ts, entry.method, entry.url, entry.status, entry.elapsed_ms,
            );
            let style = if idx == hist.selected {
                Style::default()
                    .fg(p.fg0)
                    .bg(p.iris_wash)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(status_color)
            };
            Line::styled(text, style)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
