use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{Tab, Workspace};
use crate::theme::Theme;

/// Column width of one tab block (excluding the 1-col gap that follows it).
/// Must stay the single source for both rendering and mouse hit-testing.
fn block_width(tab: &Tab, i: usize, theme: &Theme) -> u16 {
    let label = tab.display_label();
    // " icon " (3) + label + trailing " " (1)
    let mut w = label.len() as u16 + 4;
    // " N" index hint (2, first nine tabs)
    if i < 9 {
        w += 2;
    }
    // " g" agent glyph (2) — only for *actionable* states, mirroring render
    if let Some((status, attention, _)) = tab.cli_agent_snapshot()
        && crate::ui::actionable_status_view(theme, status, attention).is_some()
    {
        w += 2;
    }
    // " ×" close button (2)
    if tab.closable {
        w += 2;
    }
    w
}

/// Width of a `‹N` / `N›` hidden-count indicator block (incl. its gap).
fn indicator_width(count: usize) -> u16 {
    // "‹N " rendered as: glyph (1) + count digits + trailing space (1)
    (count.to_string().len() as u16) + 2
}

/// Windowed tab-bar layout: which tabs are visible and where each block
/// starts, relative to the bar's left edge. With many tabs the bar scrolls so
/// the ACTIVE tab is always on screen; `‹N`/`N›` indicators report how many
/// tabs are clipped on each side (and are click targets for the nearest
/// hidden tab).
pub(crate) struct TabBarLayout {
    /// (tab index, x offset, block width) for each visible tab.
    pub blocks: Vec<(usize, u16, u16)>,
    /// Left `‹N` indicator: (x, width). Present when tabs are clipped left.
    pub left: Option<(u16, u16)>,
    /// Right `N›` indicator: (x, width).
    pub right: Option<(u16, u16)>,
    pub hidden_left: usize,
    pub hidden_right: usize,
    /// x of the trailing ` + ` button.
    pub plus_x: u16,
}

pub(crate) fn layout(ws: &Workspace, theme: &Theme, width: u16) -> TabBarLayout {
    const PLUS_W: u16 = 3;
    let n = ws.tabs.len();
    let widths: Vec<u16> = ws
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| block_width(t, i, theme))
        .collect();
    let active = ws.active_tab.min(n.saturating_sub(1));

    // Fast path: everything fits (blocks + gaps + the `+` button).
    let total: u16 = widths.iter().map(|w| w + 1).sum::<u16>() + PLUS_W;
    let mut first = 0;
    let mut last = n.saturating_sub(1);
    if n > 0 && total > width {
        // Find the smallest `first` whose window reaches the active tab.
        let mut found = None;
        for f in 0..=active {
            let left_w = if f > 0 { indicator_width(f) } else { 0 };
            let mut used = left_w;
            let mut fit_last: Option<usize> = None;
            for (j, &w) in widths.iter().enumerate().skip(f) {
                let reserve = if j + 1 < n {
                    indicator_width(n - 1 - j)
                } else {
                    PLUS_W
                };
                if used + w + 1 + reserve <= width {
                    used += w + 1;
                    fit_last = Some(j);
                } else {
                    break;
                }
            }
            if fit_last.is_some_and(|l| l >= active) {
                found = Some((f, fit_last.unwrap()));
                break;
            }
        }
        // Degenerate: the active tab alone doesn't fit — show just it, clipped.
        (first, last) = found.unwrap_or((active, active));
    }

    let mut blocks = Vec::new();
    let mut x = 0u16;
    let hidden_left = first;
    let hidden_right = n.saturating_sub(1).saturating_sub(last);
    let left = (hidden_left > 0).then(|| {
        let w = indicator_width(hidden_left);
        let pos = (x, w);
        x += w;
        pos
    });
    if n > 0 {
        for (j, &w) in widths
            .iter()
            .enumerate()
            .take(last.min(n - 1) + 1)
            .skip(first)
        {
            blocks.push((j, x, w));
            x += w + 1;
        }
    }
    let right = (hidden_right > 0).then(|| {
        let w = indicator_width(hidden_right);
        let pos = (x, w);
        x += w;
        pos
    });

    TabBarLayout {
        blocks,
        left,
        right,
        hidden_left,
        hidden_right,
        plus_x: x,
    }
}

/// Render the tab bar as herdr-style solid blocks: the active tab gets the
/// accent background, inactive tabs a subtle one, one-space gaps in between
/// and a trailing `+` button. With more tabs than fit, the bar windows around
/// the active tab and shows `‹N`/`N›` clipped-count indicators. Per-tab
/// column layout comes from [`layout`], shared with `helpers::subtab_index_at`.
pub fn render(frame: &mut Frame, area: Rect, ws: &Workspace, theme: &Theme) {
    let lay = layout(ws, theme, area.width);
    let dim = Style::default()
        .fg(theme.subtabs.inactive)
        .add_modifier(Modifier::DIM);
    let mut spans: Vec<Span> = Vec::new();

    if lay.left.is_some() {
        spans.push(Span::styled(format!("‹{} ", lay.hidden_left), dim));
    }

    for &(i, _, _) in &lay.blocks {
        let tab = &ws.tabs[i];
        let label = tab.display_label();
        let icon = if tab.markdown_content.is_some() {
            "◆"
        } else {
            tab.provider.icon()
        };
        let is_active = i == ws.active_tab;
        let base_style = if is_active {
            Style::default()
                .fg(theme.subtabs.active_fg)
                .bg(theme.subtabs.active)
        } else {
            Style::default()
                .fg(theme.subtabs.inactive)
                .bg(theme.subtabs.inactive_bg)
        };

        // A dim 1-based index makes the `prefix 1..9` tab-jump discoverable.
        // Its 2-col width (" N") must stay mirrored in `block_width`.
        if i < 9 {
            spans.push(Span::styled(
                format!(" {}", i + 1),
                base_style.add_modifier(Modifier::DIM),
            ));
        }
        spans.push(Span::styled(format!(" {} ", icon), base_style));
        spans.push(Span::styled(
            label.to_string(),
            if is_active {
                base_style.add_modifier(Modifier::BOLD)
            } else {
                base_style
            },
        ));
        if let Some((status, attention, _)) = tab.cli_agent_snapshot()
            && let Some((glyph, color)) =
                crate::ui::actionable_status_view(theme, status, attention)
        {
            // Only actionable states reach the tab bar (activity stays in the
            // Agents pane); the glyph keeps its semantic color even on the
            // active accent block.
            spans.push(Span::styled(format!(" {}", glyph), base_style.fg(color)));
        }
        if tab.closable {
            spans.push(Span::styled(" ×", base_style.add_modifier(Modifier::DIM)));
        }
        spans.push(Span::styled(" ", base_style));
        spans.push(Span::raw(" "));
    }

    if lay.right.is_some() {
        spans.push(Span::styled(format!("{}› ", lay.hidden_right), dim));
    }

    spans.push(Span::styled(
        " + ",
        Style::default().fg(theme.subtabs.inactive),
    ));

    let bar = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(bar, area);
}
