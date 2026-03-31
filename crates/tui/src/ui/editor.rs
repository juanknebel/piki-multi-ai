use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let editor = match &app.editor {
        Some(e) => e,
        None => return,
    };
    let file_path = app
        .editing_file
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "?".to_string());

    let theme = &app.theme.fuzzy_search;

    let block = Block::default()
        .title(format!(" EDIT: {} ", file_path))
        .title_style(Style::default().fg(theme.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    let line_num_width = format!("{}", editor.lines.len()).len();

    // Find syntax for the file being edited
    let file_syntax = app.syntax.find_syntax(&file_path);

    let lines: Vec<Line> = editor
        .lines
        .iter()
        .enumerate()
        .skip(editor.scroll_offset)
        .take(visible_height)
        .map(|(row, line_text)| {
            let line_num = format!("{:>width$} ", row + 1, width = line_num_width);
            let mut spans = vec![Span::styled(
                line_num,
                Style::default().fg(ratatui::style::Color::DarkGray),
            )];

            if row == editor.cursor_row {
                // Cursor line: apply syntax highlighting then overlay cursor
                let highlighted = if let Some(syntax) = file_syntax {
                    let mut hl = app.syntax.highlighter_for(syntax);
                    app.syntax
                        .highlight_line(&mut hl, line_text, Style::default())
                } else {
                    vec![Span::raw(line_text.clone())]
                };

                let cursor_byte = char_to_byte_idx(line_text, editor.cursor_col);
                if cursor_byte >= line_text.len() {
                    // Cursor at end of line — append highlighted spans then cursor block
                    spans.extend(highlighted);
                    spans.push(Span::styled(
                        " ",
                        Style::default()
                            .bg(ratatui::style::Color::White)
                            .fg(ratatui::style::Color::Black),
                    ));
                } else {
                    // Split highlighted spans at cursor position
                    let next_byte = char_to_byte_idx(line_text, editor.cursor_col + 1);
                    let cursor_style = Style::default()
                        .bg(ratatui::style::Color::White)
                        .fg(ratatui::style::Color::Black);
                    let mut byte_pos: usize = 0;
                    for span in &highlighted {
                        let span_len = span.content.len();
                        let span_start = byte_pos;
                        let span_end = byte_pos + span_len;

                        if span_end <= cursor_byte {
                            // Entirely before cursor
                            spans.push(span.clone());
                        } else if span_start >= next_byte {
                            // Entirely after cursor
                            spans.push(span.clone());
                        } else {
                            // Span overlaps with cursor position — split it
                            if span_start < cursor_byte {
                                let before = &span.content[..cursor_byte - span_start];
                                spans.push(Span::styled(before.to_string(), span.style));
                            }
                            let cursor_text = &line_text[cursor_byte..next_byte];
                            spans.push(Span::styled(cursor_text.to_string(), cursor_style));
                            if next_byte < span_end {
                                let after = &span.content[next_byte - span_start..];
                                spans.push(Span::styled(after.to_string(), span.style));
                            }
                        }
                        byte_pos = span_end;
                    }
                }
            } else {
                // Non-cursor line: apply syntax highlighting
                if let Some(syntax) = file_syntax {
                    let mut hl = app.syntax.highlighter_for(syntax);
                    let highlighted =
                        app.syntax
                            .highlight_line(&mut hl, line_text, Style::default());
                    spans.extend(highlighted);
                } else {
                    spans.push(Span::raw(line_text.clone()));
                }
            }

            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
