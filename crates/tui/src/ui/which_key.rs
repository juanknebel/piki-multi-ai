//! Which-key overlay: a transient, bottom-anchored menu shown while a prefix
//! chord is pending (`InputState::PrefixPending`). It lists every key reachable
//! after the prefix, grouped by category and derived from the shared action
//! catalog — so it never drifts from the palette or the bindings themselves.
//!
//! It is a *menu*, not a filter: the next key dispatches (or cancels) exactly
//! as before, so muscle-memory chords like `C-g n` stay instant. The overlay is
//! purely a visual crutch for keys you don't remember yet.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::action_catalog::global_actions;
use crate::app::App;

/// Left column reserved for the category name.
const CAT_COL: usize = 11;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let cfg = &app.config;

    // Group prefix-reachable actions by category, preserving first-seen order.
    let mut groups: Vec<(&'static str, Vec<(String, &'static str)>)> = Vec::new();
    for a in global_actions() {
        let Some(chord) = cfg.prefix_chord(a.id) else {
            continue; // direct chords (e.g. copy) aren't reachable via the prefix
        };
        match groups.iter_mut().find(|(c, _)| *c == a.category) {
            Some((_, entries)) => entries.push((chord, a.short)),
            None => groups.push((a.category, vec![(chord, a.short)])),
        }
    }

    let chord_style = Style::default()
        .fg(app.theme.footer.key)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(app.theme.general.muted_text);
    let cat_style = Style::default()
        .fg(app.theme.palette.fg0)
        .add_modifier(Modifier::BOLD);
    let dim_style = Style::default().fg(app.theme.palette.fg3);

    let inner_width = area.width.saturating_sub(2) as usize;
    let wrap_width = inner_width.max(CAT_COL + 8);

    let mut lines: Vec<Line> = Vec::new();
    for (cat, entries) in &groups {
        let header = format!("{:<width$}", truncate(cat, CAT_COL), width = CAT_COL);
        let mut spans: Vec<Span> = vec![Span::styled(header, cat_style)];
        let mut x = CAT_COL;
        for (chord, short) in entries {
            let entry_w = chord.chars().count() + 1 + short.chars().count();
            if x > CAT_COL && x + entry_w > wrap_width {
                lines.push(Line::from(std::mem::take(&mut spans)));
                spans.push(Span::raw(" ".repeat(CAT_COL)));
                x = CAT_COL;
            }
            spans.push(Span::styled(chord.clone(), chord_style));
            spans.push(Span::raw(" "));
            spans.push(Span::styled((*short).to_string(), label_style));
            spans.push(Span::raw("  "));
            x += entry_w + 2;
        }
        lines.push(Line::from(spans));
    }

    // Meta row: the keys the catalog doesn't own.
    lines.push(Line::from(vec![
        Span::styled("1-9", dim_style),
        Span::styled(" tab   ", label_style),
        Span::styled(cfg.prefix_display(), dim_style),
        Span::styled(" literal   ", label_style),
        Span::styled("Esc", dim_style),
        Span::styled(" cancel", label_style),
    ]));

    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height),
        area.width,
        height,
    );

    let block = Block::default()
        .title(format!(" {} ", cfg.prefix_display()))
        .title_style(chord_style)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.border.active));

    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// Truncate a string to at most `max` columns (ASCII category names only).
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}
