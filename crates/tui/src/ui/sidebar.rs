use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, SidebarItem, Workspace, agent_status_severity};
use piki_core::WorkspaceType;
use piki_core::cli_agent::CliAgentStatus;

use super::layout::{pane_border_style, pane_title_style};

/// Icon prefix for each workspace type.
fn workspace_type_icon(ws_type: WorkspaceType) -> &'static str {
    match ws_type {
        WorkspaceType::Worktree => "⎇ ",
        WorkspaceType::Project => "▣ ",
        WorkspaceType::Simple => "○ ",
    }
}

/// Per-`source_repo` rollup used to drive label choice (folder name vs. own
/// name) and, when a family's parent row is collapsed, to surface its hidden
/// children's attention signals (idle notification, agent status, changed
/// files, ahead/behind) onto that one visible row instead of losing them.
#[derive(Default, Clone)]
struct FamilySummary {
    count: usize,
    has_parent: bool,
    has_idle: bool,
    worst_status: Option<(CliAgentStatus, bool)>,
    changed_files: usize,
    ahead: usize,
    behind: usize,
}

/// How a row relates to its worktree family, resolved once per row and
/// shared by both the gutter-width pre-pass and the actual render pass.
struct RowClass<'a> {
    is_parent: bool,
    is_child: bool,
    has_parent: bool,
    summary: &'a FamilySummary,
}

fn classify_row<'a>(ws: &Workspace, family_info: &'a HashMap<PathBuf, FamilySummary>) -> RowClass<'a> {
    static EMPTY: FamilySummary = FamilySummary {
        count: 0,
        has_parent: false,
        has_idle: false,
        worst_status: None,
        changed_files: 0,
        ahead: 0,
        behind: 0,
    };
    let summary = family_info.get(&ws.info.source_repo).unwrap_or(&EMPTY);
    let is_parent = summary.count > 1 && ws.info.workspace_type != WorkspaceType::Worktree;
    let is_child = summary.count > 1 && ws.info.workspace_type == WorkspaceType::Worktree;
    RowClass {
        is_parent,
        is_child,
        has_parent: summary.has_parent,
        summary,
    }
}

/// A row's effective attention signals: its own values normally, or its
/// worktree family's aggregate when it's a collapsed parent — otherwise a
/// hidden child's attention would vanish entirely.
struct EffectiveSignals {
    has_idle: bool,
    status: Option<(CliAgentStatus, bool)>,
    changed: usize,
    ahead_behind: Option<(usize, usize)>,
}

fn effective_signals(ws: &Workspace, class: &RowClass, collapsed: Option<bool>) -> EffectiveSignals {
    if class.is_parent && collapsed.unwrap_or(false) {
        let s = class.summary;
        EffectiveSignals {
            has_idle: s.has_idle,
            status: s.worst_status,
            changed: s.changed_files,
            ahead_behind: (s.ahead > 0 || s.behind > 0).then_some((s.ahead, s.behind)),
        }
    } else {
        EffectiveSignals {
            has_idle: ws.has_idle_notification,
            status: ws.agent_status_rollup(),
            changed: ws.file_count(),
            ahead_behind: ws.ahead_behind,
        }
    }
}

/// Right-aligned metadata spans (agent status glyph, changed-file count,
/// ahead/behind) for a row's effective signals. `detail_color` styles the
/// Δ/↑↓ text; the status glyph keeps its own semantic color regardless.
fn right_metadata_spans(
    app: &App,
    detail_color: Color,
    status: Option<(CliAgentStatus, bool)>,
    changed: usize,
    ahead_behind: Option<(usize, usize)>,
) -> Vec<Span<'static>> {
    let mut right: Vec<Span<'static>> = Vec::new();
    if let Some((status, attention)) = status
        && let Some((glyph, color)) = crate::ui::actionable_status_view(&app.theme, status, attention)
    {
        right.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
    }
    if changed > 0 {
        if !right.is_empty() {
            right.push(Span::raw(" "));
        }
        right.push(Span::styled(
            format!("{}∆", changed),
            Style::default().fg(detail_color),
        ));
    }
    if let Some((ahead, behind)) = ahead_behind
        && (ahead > 0 || behind > 0)
    {
        if !right.is_empty() {
            right.push(Span::raw(" "));
        }
        let mut ab = String::new();
        if ahead > 0 {
            ab.push_str(&format!("↑{}", ahead));
        }
        if behind > 0 {
            if ahead > 0 {
                ab.push(' ');
            }
            ab.push_str(&format!("↓{}", behind));
        }
        right.push(Span::styled(ab, Style::default().fg(detail_color)));
    }
    right
}

/// Returns the visual height (in lines) of a sidebar item at the given index.
/// Workspace items that follow another workspace get an extra separator line.
pub(super) fn render_workspace_list(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = pane_border_style(app, ActivePane::WorkspaceList);
    let theme = &app.theme.workspace_list;
    // Selection has two temperatures: the iris wash where the focus is, a
    // neutral raised surface where it is not — you never lose your place.
    let sel_bg = if app.active_pane == ActivePane::WorkspaceList {
        theme.selected_bg
    } else {
        app.theme.palette.bg2
    };
    // The cursor is a single left rail — iris where the focus is, muted where
    // it is not. It is the ONLY selection signal, so it never competes with
    // the group triangle or the type icon.
    let sel_bar_fg = if app.active_pane == ActivePane::WorkspaceList {
        app.theme.palette.iris
    } else {
        app.theme.palette.fg3
    };
    // Muted vertical guide that ties group children back to their header.
    let guide_fg = app.theme.palette.line;

    let block = Block::default()
        .title(" WORKSPACES ")
        .title_style(pane_title_style(app, ActivePane::WorkspaceList))
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    if app.workspaces.is_empty() {
        let key_style = Style::default().fg(app.theme.footer.key);
        let desc_style = Style::default().fg(theme.empty_text);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!(" [{}]", app.config.get_binding("app", "new_workspace")),
                    key_style,
                ),
                Span::styled(" New workspace", desc_style),
            ]),
        ];
        let text = Paragraph::new(lines).block(block);
        frame.render_widget(text, area);
        return;
    }

    let sidebar_items = app.sidebar_items();
    let visual_rows = app.sidebar_visual_rows();

    // Precompute, per source_repo, how many loaded workspaces share it,
    // whether one of them is a non-Worktree "parent", and the family's
    // aggregated attention signals (folded in below). Drives label choice,
    // the vertical guide, and what a collapsed parent surfaces on its own row.
    let mut family_info: HashMap<PathBuf, FamilySummary> = HashMap::new();
    for ws in &app.workspaces {
        let entry = family_info.entry(ws.info.source_repo.clone()).or_default();
        entry.count += 1;
        if ws.info.workspace_type != WorkspaceType::Worktree {
            entry.has_parent = true;
        }
        entry.has_idle |= ws.has_idle_notification;
        if let Some((status, attention)) = ws.agent_status_rollup() {
            let better = entry
                .worst_status
                .is_none_or(|(s, a)| agent_status_severity(status, attention) > agent_status_severity(s, a));
            if better {
                entry.worst_status = Some((status, attention));
            }
        }
        entry.changed_files += ws.file_count();
        if let Some((ahead, behind)) = ws.ahead_behind {
            entry.ahead += ahead;
            entry.behind += behind;
        }
    }

    // Fixed right-hand gutter: computed once over every row (not just the
    // visible slice, so it doesn't jitter while scrolling) so metadata always
    // starts at the same column instead of hugging each row's name length.
    let gutter_w = sidebar_items
        .iter()
        .map(|SidebarItem::Workspace { index, collapsed }| {
            let ws = &app.workspaces[*index];
            let class = classify_row(ws, &family_info);
            let sig = effective_signals(ws, &class, *collapsed);
            right_metadata_spans(app, theme.detail_normal, sig.status, sig.changed, sig.ahead_behind)
                .iter()
                .map(|sp| sp.width())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);

    // All rows are one line tall; scroll follows the selection's position in
    // `visual_rows` (which may include blank separators sidebar_items() doesn't).
    let visible_height = area.height.saturating_sub(2) as usize;
    let selected_visual = visual_rows
        .iter()
        .position(|r| *r == Some(app.selected_sidebar_row))
        .unwrap_or(0);
    let scroll_offset = if visible_height > 0 && selected_visual >= visible_height {
        selected_visual + 1 - visible_height
    } else {
        0
    };

    let inner_width = area.width.saturating_sub(2) as usize;

    let items: Vec<ListItem> = visual_rows
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|slot| {
            let Some(row) = *slot else {
                return ListItem::new(vec![Line::from("")]);
            };
            let item = &sidebar_items[row];
            let is_selected = row == app.selected_sidebar_row;
            match item {
                SidebarItem::Workspace { index, collapsed } => {
                    let ws = &app.workspaces[*index];
                    let class = classify_row(ws, &family_info);
                    let is_parent = class.is_parent;
                    let is_child = class.is_child;
                    let guide = is_child && class.has_parent;

                    let detail_color = if is_selected {
                        theme.detail_selected
                    } else {
                        theme.detail_normal
                    };

                    let is_active = *index == app.active_workspace;
                    // Icon brightness carries a second signal: parent/active
                    // rows are "the point of attention" and stay at full
                    // brightness, plain flat/child rows recede to the same
                    // muted token the tree guide already uses — so scanning
                    // the list, only containers and the active workspace pop.
                    let icon_color = if is_selected {
                        theme.detail_selected
                    } else if is_active || is_parent {
                        theme.detail_normal
                    } else {
                        app.theme.palette.fg3
                    };
                    let header_style = Style::default()
                        .fg(theme.name_inactive)
                        .add_modifier(Modifier::BOLD);

                    // Label: any non-Worktree workspace shows the repo folder
                    // name, with its own branch alongside whenever it has one
                    // ("agent-multi (master)") — empty `branch` means the
                    // configured folder isn't actually a git repo (`Project`
                    // workspaces, or a `Simple` one pointed at a plain
                    // directory), so there's nothing to show there. A
                    // worktree child shows just its branch; an orphaned
                    // worktree with no recognized parent falls back to its
                    // own name.
                    let is_worktree = ws.info.workspace_type == WorkspaceType::Worktree;
                    let label = if !is_worktree {
                        let folder = ws
                            .info
                            .source_repo
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| {
                                if !ws.info.source_repo_display.is_empty() {
                                    ws.info.source_repo_display.clone()
                                } else {
                                    ws.name.clone()
                                }
                            });
                        if ws.info.branch.is_empty() {
                            folder
                        } else {
                            format!("{folder} ({})", ws.info.branch)
                        }
                    } else if is_child {
                        ws.info.branch.clone()
                    } else {
                        ws.name.clone()
                    };

                    // Selection rail in col 0; chevron/guide in col 1. The
                    // active workspace is carried by the name weight/brightness.
                    let bar = if is_selected {
                        Span::styled("▎", Style::default().fg(sel_bar_fg))
                    } else {
                        Span::raw(" ")
                    };
                    let second_col = if is_parent {
                        let arrow = if collapsed.unwrap_or(false) {
                            "▸"
                        } else {
                            "▾"
                        };
                        Span::styled(format!("{} ", arrow), header_style)
                    } else if guide {
                        Span::styled("│ ", Style::default().fg(guide_fg))
                    } else {
                        Span::raw("  ")
                    };
                    let type_icon = workspace_type_icon(ws.info.workspace_type);

                    // A collapsed family parent surfaces its hidden children's
                    // signals (idle dot + metadata below) instead of losing
                    // them — see `effective_signals`.
                    let sig = effective_signals(ws, &class, *collapsed);

                    let mut left: Vec<Span> = vec![
                        bar,
                        second_col,
                        Span::styled(type_icon, Style::default().fg(icon_color)),
                        Span::styled(
                            label,
                            if is_active {
                                Style::default()
                                    .fg(theme.name_active)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.name_inactive)
                            },
                        ),
                    ];
                    if sig.has_idle {
                        left.push(Span::styled(
                            " ●",
                            Style::default()
                                .fg(app.theme.status.needs_you)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }

                    // Right-aligned metadata, shown only when it says something:
                    // actionable agent status, changed-file count, ahead/behind.
                    // Activity (running) stays in the Agents pane.
                    let right =
                        right_metadata_spans(app, detail_color, sig.status, sig.changed, sig.ahead_behind);

                    // Metadata always starts at the same column across every
                    // row (leaving a 1-column margin before the border, as
                    // the old per-row layout did) instead of hugging each
                    // row's own name length. A name long enough to reach that
                    // column collapses `pad` to 0 and the metadata is
                    // appended right after it rather than hidden outright —
                    // a graceful squeeze, not a disappearing act.
                    let left_w: usize = left.iter().map(|sp| sp.width()).sum();
                    let mut spans = left;
                    if gutter_w > 0 {
                        let gutter_end = inner_width.saturating_sub(1);
                        let pad = gutter_end.saturating_sub(left_w).saturating_sub(gutter_w);
                        spans.push(Span::raw(" ".repeat(pad)));
                        spans.extend(right);
                    }

                    let style = if is_selected {
                        Style::default().bg(sel_bg)
                    } else {
                        Style::default()
                    };
                    ListItem::new(vec![Line::from(spans)]).style(style)
                }
            }
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);

    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        visual_rows.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
}

/// Bottom-left pane: active AI agents across ALL workspaces.
/// One row per (workspace, tab) running a Custom provider; Enter/click jumps
/// to that workspace+tab. Status comes from the OSC 777 channel when present.
pub(super) fn render_agents_pane(frame: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_pane == ActivePane::Agents;
    let border_style = pane_border_style(app, ActivePane::Agents);
    let theme = &app.theme.file_list;

    let block = Block::default()
        .title(" AGENTS ")
        .title_style(pane_title_style(app, ActivePane::Agents))
        .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    let rows = app.agent_rows();
    if rows.is_empty() {
        let hint = format!(
            "  No agents running\n  [{}] new agent tab",
            app.config.get_binding("app", "new_tab")
        );
        let text = Paragraph::new(hint)
            .style(Style::default().fg(theme.empty_text))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let selected = app.selected_agent_row.min(rows.len() - 1);
    let visible_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = if selected >= visible_height {
        selected + 1 - visible_height
    } else {
        0
    };

    let items: Vec<ListItem> = rows
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(vis_idx, &(wi, ti))| {
            let row_idx = vis_idx + scroll_offset;
            let ws = &app.workspaces[wi];
            let tab = &ws.tabs[ti];

            let (glyph, status_label, status_color) = match tab.cli_agent_snapshot() {
                Some((status, attention, _)) => {
                    crate::ui::cli_agent_status_view(app, status, attention)
                }
                None => crate::ui::agent_tab_indicator(app, tab),
            };
            // A non-Custom tab only lists here because its cli-agent channel
            // reported — a `claude` run manually inside that tab.
            let label = if matches!(tab.provider, piki_core::AIProvider::Custom(_)) {
                tab.provider.label().to_string()
            } else {
                format!("Claude ({})", tab.provider.label())
            };

            // Selection cools to a neutral surface when the pane loses focus
            // but never disappears.
            let row_bg = if row_idx == selected {
                Style::default().bg(if is_active {
                    theme.selected_bg
                } else {
                    app.theme.palette.bg2
                })
            } else {
                Style::default()
            };
            let mut spans = vec![
                Span::styled(format!(" {glyph} "), row_bg.fg(status_color)),
                Span::styled(ws.name.clone(), row_bg.fg(theme.file_path)),
                Span::styled(" · ", row_bg.fg(theme.empty_text)),
                Span::styled(label, row_bg.fg(theme.file_path)),
                Span::styled(format!(" {status_label}"), row_bg.fg(status_color)),
            ];
            if ws.has_idle_notification {
                spans.push(Span::styled(" ●", row_bg.fg(app.theme.status.needs_you)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);

    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        rows.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
}
