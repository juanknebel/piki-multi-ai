use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

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
                // Render line with cursor
                for (ci, ch) in line_text.chars().enumerate() {
                    if ci == editor.cursor_col {
                        spans.push(Span::styled(
                            String::from(ch),
                            Style::default()
                                .bg(ratatui::style::Color::White)
                                .fg(ratatui::style::Color::Black),
                        ));
                    } else {
                        spans.push(Span::raw(String::from(ch)));
                    }
                }
                // If cursor is at end of line
                if editor.cursor_col >= line_text.chars().count() {
                    spans.push(Span::styled(
                        " ",
                        Style::default()
                            .bg(ratatui::style::Color::White)
                            .fg(ratatui::style::Color::Black),
                    ));
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
