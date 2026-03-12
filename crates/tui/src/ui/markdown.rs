use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Parse markdown content into a `Text<'static>` that can be cached across frames.
pub fn parse_to_static(content: &str) -> Text<'static> {
    let mut text = tui_markdown::from_str(content);

    // Bake line-level styles into each span.
    for line in &mut text.lines {
        let line_style = line.style;
        if line_style != Style::default() {
            for span in &mut line.spans {
                span.style = line_style.patch(span.style);
            }
        }
    }

    // Post-process code blocks
    let code_bg = Style::new().on_dark_gray();
    let fence_prefix_style = Style::new().fg(Color::DarkGray);
    let mut in_code_block = false;
    let mut processed: Vec<Line<'static>> = Vec::with_capacity(text.lines.len());

    for line in text.lines {
        let first_text = line
            .spans
            .first()
            .map(|s| s.content.as_ref())
            .unwrap_or("");

        if first_text.starts_with("```") {
            in_code_block = !in_code_block;
            processed.push(Line::from(Span::styled("  ▐ ", fence_prefix_style)));
            continue;
        }

        if in_code_block {
            let mut spans: Vec<Span<'static>> =
                vec![Span::styled("  ▐ ", fence_prefix_style)];
            for span in line.spans {
                let owned = Span::styled(span.content.into_owned(), code_bg.patch(span.style));
                spans.push(owned);
            }
            processed.push(Line::from(spans));
        } else {
            // Convert spans to 'static by owning their content
            let owned_spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|s| Span::styled(s.content.into_owned(), s.style))
                .collect();
            let mut owned_line = Line::from(owned_spans);
            owned_line.style = line.style;
            processed.push(owned_line);
        }
    }

    Text::from(processed)
}

/// Render pre-parsed cached markdown text.
pub fn render_cached(
    frame: &mut Frame,
    area: Rect,
    text: &Text<'static>,
    label: &str,
    scroll: u16,
    border_style: Style,
) {
    let block = Block::default()
        .title(format!(" {} ", label))
        .title_style(border_style)
        .borders(Borders::ALL)
        .border_style(border_style);

    let paragraph = Paragraph::new(text.clone())
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

/// Render markdown content in the main panel area (uncached path).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    content: &str,
    label: &str,
    scroll: u16,
    border_style: Style,
) {
    let text = parse_to_static(content);
    render_cached(frame, area, &text, label, scroll, border_style);
}
