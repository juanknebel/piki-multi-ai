use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let state = match &app.fuzzy {
        Some(s) => s,
        None => return,
    };
    let theme = &app.theme.fuzzy_search;

    // Centered overlay: 70% width, 60% height
    let width = (area.width * 70 / 100).max(40).min(area.width);
    let height = (area.height * 60 / 100).max(10).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Fuzzy Search ")
        .title_style(Style::default().fg(theme.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 {
        return;
    }

    // Line 0: search input
    let cursor_char = "█";
    let input_line = Line::from(vec![
        Span::styled(" > ", Style::default().fg(theme.border)),
        Span::styled(
            format!("{}{}", state.query, cursor_char),
            Style::default().fg(theme.input_text),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(input_line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    // Line 1: result count
    let total = state.all_files.len();
    let filtered = state.results.len();
    let count_line = Line::from(Span::styled(
        format!(" {}/{}", filtered, total),
        Style::default().fg(theme.count_text),
    ));
    frame.render_widget(
        Paragraph::new(count_line),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Lines 2+: results list
    let results_height = (inner.height as usize).saturating_sub(2);
    if results_height == 0 {
        return;
    }

    // Compute scroll offset so selected item is visible
    let scroll_offset = if state.selected >= results_height {
        state.selected - results_height + 1
    } else {
        0
    };

    let visible_results = state
        .results
        .iter()
        .skip(scroll_offset)
        .take(results_height);

    for (i, result) in visible_results.enumerate() {
        let abs_idx = scroll_offset + i;
        let is_selected = abs_idx == state.selected;

        let bg = if is_selected {
            theme.selected_bg
        } else {
            ratatui::style::Color::Reset
        };

        let path = state.result_path(result);
        // Build spans with match highlighting
        let mut spans = vec![Span::styled(" ", Style::default().bg(bg))];
        for (ci, ch) in path.chars().enumerate() {
            let is_match = result.match_indices.contains(&(ci as u32));
            let style = if is_match {
                Style::default()
                    .fg(theme.match_highlight)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.result_text).bg(bg)
            };
            spans.push(Span::styled(String::from(ch), style));
        }

        let line = Line::from(spans);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + 2 + i as u16, inner.width, 1),
        );
    }
}
