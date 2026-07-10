use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::workspace_switcher::SwitcherRow;

/// Tree-style workspace/tab switcher: workspaces as headers, their tabs as
/// children with a right-aligned provider·status column, an accent selection
/// bar, and a breadcrumb + hints footer.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let state = match &app.workspace_switcher {
        Some(s) => s,
        None => return,
    };
    let theme = &app.theme.fuzzy_search;

    // Large centered overlay (this is a whole-screen navigator, like herdr's).
    let width = (area.width * 82 / 100).max(40).min(area.width);
    let height = (area.height * 74 / 100).max(10).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 4 || inner.width < 8 {
        return;
    }
    let iw = inner.width as usize;

    // ── Header: "/ query" left, counter right ─────────────────────────────
    let header_left = if state.query.is_empty() {
        " / switch workspace".to_string()
    } else {
        format!(" / {}\u{2588}", state.query)
    };
    let counter = format!("{} ", state.workspace_count());
    let header = pad_between(&header_left, &counter, iw);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            header,
            Style::default().fg(theme.count_text),
        ))),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    // ── Body: the tree, with derived scroll ───────────────────────────────
    // Rows: header(1) + [tree] + breadcrumb(1) + hints(1).
    let body_top = inner.y + 1;
    let body_height = inner.height.saturating_sub(3) as usize;
    if body_height == 0 {
        return;
    }
    let total = state.rows.len();
    let selected = state.selected.min(total.saturating_sub(1));
    let scroll = if selected >= body_height {
        selected + 1 - body_height
    } else {
        0
    };

    for i in 0..body_height {
        let idx = scroll + i;
        if idx >= total {
            break;
        }
        let is_selected = idx == selected;
        let line = row_line(app, state.rows[idx], is_selected, iw, theme);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, body_top + i as u16, inner.width, 1),
        );
    }

    // ── Footer: breadcrumb + hints ────────────────────────────────────────
    let breadcrumb = state
        .selected_row()
        .map(|r| breadcrumb_text(app, r))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {breadcrumb}"),
            Style::default().fg(theme.count_text),
        ))),
        Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1),
    );

    let hint_style = Style::default().fg(theme.count_text);
    let key_style = Style::default()
        .fg(theme.result_text)
        .add_modifier(Modifier::BOLD);
    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("enter", key_style),
        Span::styled(" switch  ", hint_style),
        Span::styled("^p/^n", key_style),
        Span::styled(" move  ", hint_style),
        Span::styled("type", key_style),
        Span::styled(" search  ", hint_style),
        Span::styled("esc", key_style),
        Span::styled(" close", hint_style),
    ]);
    frame.render_widget(
        Paragraph::new(hints),
        Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
    );
}

/// Build one tree row (workspace header or tab), as a full-width line so the
/// selection bar spans the whole row.
fn row_line<'a>(
    app: &App,
    row: SwitcherRow,
    is_selected: bool,
    iw: usize,
    theme: &crate::theme::FuzzySearchTheme,
) -> Line<'a> {
    let sel_bg = theme.selected_bg;
    let (left, right, right_color) = match row {
        SwitcherRow::Workspace { ws_idx } => {
            let Some(ws) = app.workspaces.get(ws_idx) else {
                return Line::from("");
            };
            let is_active = ws_idx == app.active_workspace;
            let (glyph, color) = match ws.agent_status_rollup() {
                Some(s) => {
                    let (g, _, c) = crate::ui::cli_agent_status_view(app, s);
                    (g, c)
                }
                None => ("○", app.theme.status.exited),
            };
            let marker = if is_active { "◆ " } else { "  " };
            let left = format!(" {marker}{glyph} {} ({})", ws.name, ws.tabs.len());
            (left, String::new(), color)
        }
        SwitcherRow::Tab { ws_idx, tab_idx } => {
            let Some(ws) = app.workspaces.get(ws_idx) else {
                return Line::from("");
            };
            let Some(tab) = ws.tabs.get(tab_idx) else {
                return Line::from("");
            };
            let is_active_tab = tab_idx == ws.active_tab;
            let (glyph, status_label, color) = match tab.cli_agent_snapshot() {
                Some((status, _)) => crate::ui::cli_agent_status_view(app, status),
                None => crate::ui::agent_tab_indicator(app, tab),
            };
            let label = tab
                .markdown_label
                .as_deref()
                .unwrap_or(tab.provider.label());
            let arrow = if is_active_tab { "→ " } else { "  " };
            let left = format!("    ├─ {arrow}{glyph} {label}");

            use piki_core::AIProvider;
            let (right, right_color) = if matches!(tab.provider, AIProvider::Custom(_))
                || tab.cli_agent_snapshot().is_some()
            {
                (
                    format!("{} · {}", tab.provider.label().to_lowercase(), status_label),
                    color,
                )
            } else {
                (tab.provider.label().to_lowercase(), Color::DarkGray)
            };
            (left, right, right_color)
        }
    };

    let is_bold = matches!(row, SwitcherRow::Workspace { ws_idx } if ws_idx == app.active_workspace)
        || matches!(row, SwitcherRow::Tab { ws_idx, tab_idx }
            if app.workspaces.get(ws_idx).is_some_and(|w| w.active_tab == tab_idx));

    if is_selected {
        // Full-width accent bar with dark text; right column keeps a subtle
        // contrast by staying bold.
        let base = Style::default().fg(Color::Black).bg(sel_bg);
        let text = pad_between(&left, &format!("{right} "), iw);
        let mut style = base;
        if is_bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        Line::from(Span::styled(text, style))
    } else {
        let mut left_style = Style::default().fg(match row {
            SwitcherRow::Workspace { .. } => theme.result_text,
            SwitcherRow::Tab { .. } => theme.result_text,
        });
        if is_bold {
            left_style = left_style.add_modifier(Modifier::BOLD);
        }
        let pad = iw
            .saturating_sub(display_width(&left))
            .saturating_sub(display_width(&right))
            .saturating_sub(1);
        Line::from(vec![
            Span::styled(left, left_style),
            Span::raw(" ".repeat(pad)),
            Span::styled(right, Style::default().fg(right_color)),
            Span::raw(" "),
        ])
    }
}

/// Breadcrumb for the footer, e.g. `piki-nightly · tab: Claude · idle`.
fn breadcrumb_text(app: &App, row: SwitcherRow) -> String {
    match row {
        SwitcherRow::Workspace { ws_idx } => app
            .workspaces
            .get(ws_idx)
            .map(|w| w.name.clone())
            .unwrap_or_default(),
        SwitcherRow::Tab { ws_idx, tab_idx } => {
            let Some(ws) = app.workspaces.get(ws_idx) else {
                return String::new();
            };
            let Some(tab) = ws.tabs.get(tab_idx) else {
                return ws.name.clone();
            };
            let label = tab
                .markdown_label
                .as_deref()
                .unwrap_or(tab.provider.label());
            let status = tab
                .cli_agent_snapshot()
                .map(|(s, _)| crate::ui::cli_agent_status_view(app, s).1)
                .unwrap_or("");
            if status.is_empty() {
                format!("{} · tab: {}", ws.name, label)
            } else {
                format!("{} · tab: {} · {}", ws.name, label, status)
            }
        }
    }
}

/// Left-justify `left`, right-justify `right`, padded to `width` columns
/// (display width aware). Truncates `left` if the two would overlap.
fn pad_between(left: &str, right: &str, width: usize) -> String {
    let lw = display_width(left);
    let rw = display_width(right);
    if lw + rw >= width {
        // Not enough room — just clip to width, prioritizing the left text.
        return truncate_to(left, width);
    }
    format!("{left}{}{right}", " ".repeat(width - lw - rw))
}

/// Approximate display width by char count. Workspace/tab names are almost
/// always ASCII; the padding this feeds is cosmetic, so a wide-char miss just
/// nudges alignment, never breaks layout.
fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn truncate_to(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}
