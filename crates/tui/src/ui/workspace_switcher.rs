use std::collections::HashSet;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let state = match &app.workspace_switcher {
        Some(s) => s,
        None => return,
    };
    let theme = &app.theme.fuzzy_search;
    let snap = state.nucleo.snapshot();

    // Centered overlay: 60% width, 50% height
    let width = (area.width * 60 / 100).max(30).min(area.width);
    let height = (area.height * 50 / 100).max(8).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Switch Workspace (Space) ")
        .title_style(Style::default().fg(theme.border))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 {
        return;
    }

    // Line 0: search input
    let cursor_char = "\u{2588}";
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
    let total = snap.item_count();
    let filtered = snap.matched_item_count();
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
    if results_height == 0 || filtered == 0 {
        return;
    }

    let scroll_offset = if state.selected >= results_height {
        state.selected - results_height + 1
    } else {
        0
    };

    let pattern = snap.pattern().column_pattern(0);
    let has_pattern = !state.query.is_empty();
    let mut matcher = nucleo::Matcher::default();
    let mut indices_buf = Vec::new();
    let mut utf32_buf = Vec::new();

    for i in 0..results_height {
        let abs_idx = (scroll_offset + i) as u32;
        if abs_idx >= filtered {
            break;
        }

        let item = match snap.get_matched_item(abs_idx) {
            Some(item) => item,
            None => break,
        };
        let entry = &item.data;
        let display_text = if let Some(ref group) = entry.group {
            format!("{} ({}) [{}]", entry.name, group, entry.branch)
        } else {
            format!("{} [{}]", entry.name, entry.branch)
        };
        let is_selected = scroll_offset + i == state.selected;

        let bg = if is_selected {
            theme.selected_bg
        } else {
            ratatui::style::Color::Reset
        };

        // Compute match indices for highlighting
        indices_buf.clear();
        if has_pattern {
            let haystack = nucleo::Utf32Str::new(&display_text, &mut utf32_buf);
            pattern.indices(haystack, &mut matcher, &mut indices_buf);
            indices_buf.sort_unstable();
            indices_buf.dedup();
        }

        let matched_style = Style::default()
            .fg(theme.match_highlight)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let normal_style = Style::default().fg(theme.result_text).bg(bg);
        let hint_style = Style::default().fg(theme.count_text).bg(bg);

        let match_set: HashSet<u32> = indices_buf.iter().copied().collect();
        let mut spans = vec![Span::styled(" ", Style::default().bg(bg))];

        // Number badge (1-9) for quick access hint
        if entry.index < 9 {
            spans.push(Span::styled(format!("{} ", entry.index + 1), hint_style));
        } else {
            spans.push(Span::styled("  ", Style::default().bg(bg)));
        }

        let mut run_start: Option<(usize, bool)> = None;
        for (ci, (byte_idx, _ch)) in display_text.char_indices().enumerate() {
            let is_match = match_set.contains(&(ci as u32));
            match run_start {
                Some((start, prev_match)) if prev_match != is_match => {
                    let style = if prev_match {
                        matched_style
                    } else {
                        normal_style
                    };
                    spans.push(Span::styled(
                        display_text[start..byte_idx].to_string(),
                        style,
                    ));
                    run_start = Some((byte_idx, is_match));
                }
                None => {
                    run_start = Some((byte_idx, is_match));
                }
                _ => {}
            }
        }
        if let Some((start, is_match)) = run_start {
            let style = if is_match {
                matched_style
            } else {
                normal_style
            };
            spans.push(Span::styled(display_text[start..].to_string(), style));
        }

        let line = Line::from(spans);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + 2 + i as u16, inner.width, 1),
        );
    }
}
