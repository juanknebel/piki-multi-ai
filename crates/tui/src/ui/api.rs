use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{ApiResponseDisplay, ApiTabState, Selection};

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Render the API Explorer tab. Returns the inner area of the response panel
/// (for mouse hit-testing), or `None` if no response is displayed.
pub(crate) fn render(
    frame: &mut Frame,
    area: Rect,
    api: &ApiTabState,
    border_style: Style,
    selection: Option<&Selection>,
    selection_style: Style,
) -> Option<Rect> {
    let has_response = !api.responses.is_empty() || api.loading;

    let chunks = if has_response {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area)
    } else {
        Layout::vertical([Constraint::Min(0), Constraint::Length(0)]).split(area)
    };

    // ── Editor pane ──
    render_editor(frame, chunks[0], api, border_style);

    // ── Response pane ──
    if has_response {
        render_responses(
            frame,
            chunks[1],
            api,
            border_style,
            selection,
            selection_style,
        )
    } else {
        None
    }
}

fn render_editor(frame: &mut Frame, area: Rect, api: &ApiTabState, border_style: Style) {
    let block = Block::default()
        .title(" API Explorer ")
        .title_style(border_style)
        .title_bottom(
            Line::from(Span::styled(
                " [^S send] ",
                Style::default().fg(Color::DarkGray),
            ))
            .right_aligned(),
        )
        .borders(Borders::ALL)
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
            let mut spans = vec![Span::styled(line_num, Style::default().fg(Color::DarkGray))];

            if row == editor.cursor_row {
                let cursor_byte = char_to_byte_idx(line_text, editor.cursor_col);
                if cursor_byte >= line_text.len() {
                    spans.push(Span::raw(line_text.clone()));
                    spans.push(Span::styled(
                        " ",
                        Style::default().bg(Color::White).fg(Color::Black),
                    ));
                } else {
                    let next_byte = char_to_byte_idx(line_text, editor.cursor_col + 1);
                    if cursor_byte > 0 {
                        spans.push(Span::raw(line_text[..cursor_byte].to_string()));
                    }
                    spans.push(Span::styled(
                        line_text[cursor_byte..next_byte].to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
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

fn render_responses(
    frame: &mut Frame,
    area: Rect,
    api: &ApiTabState,
    border_style: Style,
    selection: Option<&Selection>,
    selection_style: Style,
) -> Option<Rect> {
    if api.loading {
        let block = Block::default()
            .title(" Response ")
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style);
        let text = Paragraph::new("  Sending...")
            .style(Style::default().fg(Color::Yellow))
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
        let status_color = status_color(resp.status);
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
        all_lines.extend(colorize_json(&resp.body));

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
        Color::Green
    } else if api
        .responses
        .iter()
        .any(|r| r.status == 0 || r.status >= 500)
    {
        Color::Red
    } else {
        Color::Yellow
    };

    let help_hint = if api.search.is_some() {
        " [^F search] "
    } else {
        " [^J/^K scroll | ^F search] "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(title_color)))
        .title_bottom(
            Line::from(Span::styled(
                help_hint,
                Style::default().fg(Color::DarkGray),
            ))
            .right_aligned(),
        )
        .borders(Borders::ALL)
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
            let match_style = Style::default().bg(Color::Yellow).fg(Color::Black);
            let current_style = Style::default().bg(Color::LightRed).fg(Color::Black);
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
            Span::styled(" / ", Style::default().fg(Color::Yellow)),
            Span::raw(&search.query),
            Span::styled(match_info, Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(bar, bar_area);
    }

    Some(inner)
}

fn status_color(status: u16) -> Color {
    match status {
        200..=299 => Color::Green,
        400..=499 => Color::Yellow,
        500..=599 => Color::Red,
        0 => Color::Red,
        _ => Color::White,
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

const KEY_COLOR: Color = Color::Cyan;
const STRING_COLOR: Color = Color::Green;
const NUMBER_COLOR: Color = Color::Yellow;
const BOOL_COLOR: Color = Color::Magenta;
const NULL_COLOR: Color = Color::DarkGray;
const PUNCT_COLOR: Color = Color::White;

/// Colorize a JSON string line-by-line using a simple state machine.
/// Falls back to plain white text if the input is not valid JSON.
fn colorize_json(text: &str) -> Vec<Line<'static>> {
    if serde_json::from_str::<serde_json::Value>(text).is_err() {
        return text.lines().map(|l| Line::raw(l.to_string())).collect();
    }

    text.lines()
        .map(|line| Line::from(colorize_json_line(line)))
        .collect()
}

/// Colorize a single line of pretty-printed JSON.
fn colorize_json_line(line: &str) -> Vec<Span<'static>> {
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
                    KEY_COLOR
                } else {
                    STRING_COLOR
                };
                spans.push(Span::styled(s, Style::default().fg(color)));
            }
            't' | 'f'
                if matches_keyword(&chars[i..], "true")
                    || matches_keyword(&chars[i..], "false") =>
            {
                let kw_len = if chars[i] == 't' { 4 } else { 5 };
                let s: String = chars[i..i + kw_len].iter().collect();
                spans.push(Span::styled(s, Style::default().fg(BOOL_COLOR)));
                i += kw_len;
            }
            'n' if matches_keyword(&chars[i..], "null") => {
                spans.push(Span::styled(
                    "null".to_string(),
                    Style::default().fg(NULL_COLOR),
                ));
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
                spans.push(Span::styled(s, Style::default().fg(NUMBER_COLOR)));
            }
            '{' | '}' | '[' | ']' | ':' | ',' => {
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(PUNCT_COLOR),
                ));
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
