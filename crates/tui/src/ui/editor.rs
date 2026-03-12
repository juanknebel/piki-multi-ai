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
                // Render line with cursor using 3 slices instead of per-char alloc
                let cursor_byte = char_to_byte_idx(line_text, editor.cursor_col);
                if cursor_byte >= line_text.len() {
                    // Cursor at end of line
                    spans.push(Span::raw(line_text.clone()));
                    spans.push(Span::styled(
                        " ",
                        Style::default()
                            .bg(ratatui::style::Color::White)
                            .fg(ratatui::style::Color::Black),
                    ));
                } else {
                    let next_byte = char_to_byte_idx(line_text, editor.cursor_col + 1);
                    if cursor_byte > 0 {
                        spans.push(Span::raw(line_text[..cursor_byte].to_string()));
                    }
                    spans.push(Span::styled(
                        line_text[cursor_byte..next_byte].to_string(),
                        Style::default()
                            .bg(ratatui::style::Color::White)
                            .fg(ratatui::style::Color::Black),
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
