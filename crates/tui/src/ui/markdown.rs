use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::syntax::SyntaxHighlighter;

/// Parse markdown content into a `Text<'static>` that can be cached across frames.
/// When a `SyntaxHighlighter` is provided, fenced code blocks with language hints
/// (e.g., ` ```rust `) will be syntax-highlighted.
pub fn parse_to_static(content: &str, syntax_hl: Option<&SyntaxHighlighter>) -> Text<'static> {
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
    let mut code_lang: Option<String> = None;
    let mut processed: Vec<Line<'static>> = Vec::with_capacity(text.lines.len());

    for line in text.lines {
        let first_text = line.spans.first().map(|s| s.content.as_ref()).unwrap_or("");

        if let Some(after_fence) = first_text.strip_prefix("```") {
            if in_code_block {
                // Closing fence
                in_code_block = false;
                code_lang = None;
            } else {
                // Opening fence — extract language hint (e.g., "```rust" -> "rust")
                in_code_block = true;
                let lang = after_fence.trim();
                code_lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
            }
            processed.push(Line::from(Span::styled("  \u{2590} ", fence_prefix_style)));
            continue;
        }

        if in_code_block {
            let mut spans: Vec<Span<'static>> =
                vec![Span::styled("  \u{2590} ", fence_prefix_style)];

            // Try syntax highlighting for the code block
            let highlighted = syntax_hl.zip(code_lang.as_deref()).and_then(|(hl, lang)| {
                let syntax = hl.find_syntax_by_name(lang)?;
                let mut highlighter = hl.highlighter_for(syntax);
                // Reconstruct the raw text from spans
                let raw_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                Some(hl.highlight_line(&mut highlighter, &raw_text, code_bg))
            });

            if let Some(hl_spans) = highlighted {
                spans.extend(hl_spans);
            } else {
                for span in line.spans {
                    let owned = Span::styled(span.content.into_owned(), code_bg.patch(span.style));
                    spans.push(owned);
                }
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
    let visible_height = area.height.saturating_sub(2) as usize;
    super::scrollbar::render_vertical(
        frame,
        area,
        scroll as usize,
        text.lines.len(),
        visible_height,
        Color::DarkGray,
    );
}

/// Render markdown content in the main panel area (uncached path).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    content: &str,
    label: &str,
    scroll: u16,
    border_style: Style,
    syntax_hl: Option<&SyntaxHighlighter>,
) {
    let text = parse_to_static(content, syntax_hl);
    render_cached(frame, area, &text, label, scroll, border_style);
}
