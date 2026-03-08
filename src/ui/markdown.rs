use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Render markdown content in the main panel area.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    content: &str,
    label: &str,
    scroll: u16,
    border_style: Style,
) {
    let mut text = tui_markdown::from_str(content);

    // Bake line-level styles into each span.
    // tui-markdown sets styles on Line (e.g. headings, blockquotes) but leaves
    // spans with Style::new(). Paragraph+Wrap doesn't propagate line.style to
    // spans, so headings and blockquotes lose their color. Fix by patching each
    // span with its parent line style.
    for line in &mut text.lines {
        let line_style = line.style;
        if line_style != Style::default() {
            for span in &mut line.spans {
                span.style = line_style.patch(span.style);
            }
        }
    }

    // Post-process code blocks: remove ```lang / ``` fence lines and apply a
    // background to code lines, plus add a left-border "│" prefix (like mdr).
    let code_bg = Style::new().on_dark_gray();
    let fence_prefix_style = Style::new().fg(Color::DarkGray);
    let mut in_code_block = false;
    let mut processed: Vec<Line> = Vec::with_capacity(text.lines.len());

    for line in text.lines {
        let first_text = line
            .spans
            .first()
            .map(|s| s.content.as_ref())
            .unwrap_or("");

        if first_text.starts_with("```") {
            if !in_code_block {
                // Opening fence — replace with a thin separator
                in_code_block = true;
                processed.push(Line::from(Span::styled("  ▐ ", fence_prefix_style)));
            } else {
                // Closing fence — thin separator
                in_code_block = false;
                processed.push(Line::from(Span::styled("  ▐ ", fence_prefix_style)));
            }
            continue;
        }

        if in_code_block {
            // Apply code background and left border to every span
            let mut spans: Vec<Span> =
                vec![Span::styled("  ▐ ", fence_prefix_style)];
            for mut span in line.spans {
                span.style = code_bg.patch(span.style);
                spans.push(span);
            }
            processed.push(Line::from(spans));
        } else {
            processed.push(line);
        }
    }

    text.lines = processed;

    let block = Block::default()
        .title(format!(" {} ", label))
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}
