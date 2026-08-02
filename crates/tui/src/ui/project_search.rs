//! Project-wide content search overlay (ripgrep-backed), modeled on the
//! fuzzy file search: centered popup, query input, incremental results.
//! Rows read `path:line  text`, with the literal query highlighted in the
//! matched line text.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let state = match &app.project_search {
        Some(s) => s,
        None => return,
    };
    let theme = &app.theme.fuzzy_search;
    let shared = state.shared.lock();
    let searching = state.searching();

    // Centered overlay: 80% width (snippets need room), 60% height.
    let width = (area.width * 80 / 100).max(40).min(area.width);
    let height = (area.height * 60 / 100).max(10).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Search in Project ")
        .title_style(Style::default().fg(theme.border))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 {
        return;
    }

    // Line 0: query input
    let input_line = Line::from(vec![
        Span::styled(" > ", Style::default().fg(theme.border)),
        Span::styled(
            format!("{}█", state.query),
            Style::default().fg(theme.input_text),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(input_line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    // Line 1: hit count / progress
    let count_text = if searching {
        " searching…".to_string()
    } else if state.query.is_empty() {
        " type to search file contents".to_string()
    } else {
        format!(" {} matches", shared.hits.len())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            count_text,
            Style::default().fg(theme.count_text),
        ))),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Lines 2+: results
    let results_height = (inner.height as usize).saturating_sub(2);
    if results_height == 0 || shared.hits.is_empty() {
        return;
    }

    let selected = state.selected.min(shared.hits.len() - 1);
    let scroll_offset = selected.saturating_sub(results_height - 1);

    let query_lower = state.query.to_lowercase();
    for (i, hit) in shared
        .hits
        .iter()
        .skip(scroll_offset)
        .take(results_height)
        .enumerate()
    {
        let is_selected = scroll_offset + i == selected;
        let bg = if is_selected {
            theme.selected_bg
        } else {
            ratatui::style::Color::Reset
        };

        let loc_style = Style::default().fg(theme.count_text).bg(bg);
        let text_style = Style::default().fg(theme.result_text).bg(bg);
        let match_style = Style::default()
            .fg(theme.match_highlight)
            .bg(bg)
            .add_modifier(Modifier::BOLD);

        let mut spans = vec![
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled(format!("{}:{}", hit.path, hit.line_num), loc_style),
            Span::styled("  ", Style::default().bg(bg)),
        ];
        spans.extend(highlight_query(
            hit.text.trim(),
            &query_lower,
            text_style,
            match_style,
        ));

        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, inner.y + 2 + i as u16, inner.width, 1),
        );
    }
}

/// Split `text` into spans, styling case-insensitive occurrences of `query`
/// with `match_style`. `query` must already be lowercased.
fn highlight_query(text: &str, query: &str, normal: Style, matched: Style) -> Vec<Span<'static>> {
    if query.is_empty() {
        return vec![Span::styled(text.to_string(), normal)];
    }
    let lower = text.to_lowercase();
    // The lowercased haystack can have different byte offsets than `text`
    // (e.g. 'İ' lowercases to two chars) — bail out to unhighlighted rather
    // than slicing at a wrong or non-boundary offset.
    if lower.len() != text.len() {
        return vec![Span::styled(text.to_string(), normal)];
    }
    let mut spans = Vec::new();
    let mut pos = 0;
    while let Some(found) = lower[pos..].find(query) {
        let start = pos + found;
        let end = start + query.len();
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            break;
        }
        if start > pos {
            spans.push(Span::styled(text[pos..start].to_string(), normal));
        }
        spans.push(Span::styled(text[start..end].to_string(), matched));
        pos = end;
    }
    if pos < text.len() {
        spans.push(Span::styled(text[pos..].to_string(), normal));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(spans: &[Span<'static>]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    #[test]
    fn highlights_case_insensitive_occurrences() {
        let spans = highlight_query(
            "Foo bar foo",
            "foo",
            Style::default(),
            Style::default().add_modifier(Modifier::BOLD),
        );
        assert_eq!(texts(&spans), vec!["Foo", " bar ", "foo"]);
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(!spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn empty_query_is_one_plain_span() {
        let spans = highlight_query("hello", "", Style::default(), Style::default());
        assert_eq!(texts(&spans), vec!["hello"]);
    }

    #[test]
    fn multibyte_text_does_not_panic() {
        let spans = highlight_query(
            "búsqueda ñ búsqueda",
            "búsqueda",
            Style::default(),
            Style::default(),
        );
        assert_eq!(texts(&spans), vec!["búsqueda", " ñ ", "búsqueda"]);
    }
}
