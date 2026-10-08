use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ActivePane, App, Bucket, ProjectTreeRow, Workspace, agent_status_severity};
use piki_core::WorkspaceType;
use piki_core::cli_agent::CliAgentStatus;

use super::dialogs::projects::{ellipsize_end, ellipsize_start};
use super::layout::{pane_border_style, pane_title_style};

/// Pane title. One view now — the project tree — so the title is a label,
/// not a tab bar.
const PANE_TITLE: &str = " PROJECTS ";

/// Display label for a synthetic bucket header. Core only hands back the
/// variant; the wording is the frontend's (the desktop mirrors these two).
fn bucket_label(bucket: Bucket) -> &'static str {
    match bucket {
        Bucket::PrReview => "pr-review",
        Bucket::Unassigned => "no project",
        // A real project renders its own name, never this.
        Bucket::Project(_) => "",
    }
}

/// Icon prefix for a checkout row. A `Simple` workspace pointed at a plain
/// (non-git) directory gets a distinct folder icon — otherwise it's
/// indistinguishable from a git-backed one until its branch happens to
/// resolve (see `Workspace::branch`). An ephemeral PR review checkout gets
/// its own icon regardless of type, since it's transient in a way none of
/// the other workspace kinds are.
fn workspace_type_icon(ws_type: WorkspaceType, is_git_repo: bool, ephemeral: bool) -> &'static str {
    if ephemeral {
        // Ad-hoc PR review checkout — never persisted, deleted on close.
        // Distinct from every other icon so it reads as "throwaway" at a glance.
        return "◎ ";
    }
    match ws_type {
        WorkspaceType::Worktree => "⎇ ",
        WorkspaceType::Project => "▣ ",
        WorkspaceType::Simple if !is_git_repo => "🗁 ",
        WorkspaceType::Simple => "○ ",
    }
}

/// Attention signals for one row, or rolled up across a collapsed header's
/// hidden descendants — otherwise collapsing a project would silently hide
/// the one agent that needs you.
#[derive(Default, Clone, Copy)]
struct Signals {
    has_idle: bool,
    worst_status: Option<(CliAgentStatus, bool)>,
    changed: usize,
    ahead: usize,
    behind: usize,
}

impl Signals {
    fn of(ws: &Workspace) -> Self {
        let (ahead, behind) = ws.ahead_behind.unwrap_or((0, 0));
        Self {
            has_idle: ws.has_idle_notification,
            worst_status: ws.agent_status_rollup(),
            changed: ws.file_count(),
            ahead,
            behind,
        }
    }

    fn absorb(&mut self, other: Signals) {
        self.has_idle |= other.has_idle;
        if let Some((status, attention)) = other.worst_status {
            let worse = self.worst_status.is_none_or(|(s, a)| {
                agent_status_severity(status, attention) > agent_status_severity(s, a)
            });
            if worse {
                self.worst_status = Some((status, attention));
            }
        }
        self.changed += other.changed;
        self.ahead += other.ahead;
        self.behind += other.behind;
    }

    fn ahead_behind(&self) -> Option<(usize, usize)> {
        (self.ahead > 0 || self.behind > 0).then_some((self.ahead, self.behind))
    }
}

/// Signals per collapse key, rolled up from the FULLY EXPANDED tree so a
/// collapsed project or repo can surface what it is hiding. Built off a second
/// `project_tree` pass with nothing collapsed, since the rows actually being
/// drawn omit their hidden descendants by construction.
fn rollups(app: &App) -> HashMap<String, Signals> {
    let infos: Vec<piki_core::WorkspaceInfo> =
        app.workspaces.iter().map(|w| w.info.clone()).collect();
    let expanded = piki_core::projects::tree::project_tree(
        &app.sidebar_projects,
        &infos,
        &std::collections::HashSet::new(),
    );
    let mut out: HashMap<String, Signals> = HashMap::new();
    // The header rows a checkout belongs to: its project/bucket, and its repo
    // group when it has one.
    let mut project_key: Option<String> = None;
    let mut repo_key: Option<String> = None;
    for row in &expanded {
        match row {
            ProjectTreeRow::Project { key, .. } => {
                project_key = Some(key.clone());
                repo_key = None;
            }
            ProjectTreeRow::Repo { key, .. } => repo_key = Some(key.clone()),
            ProjectTreeRow::Checkout {
                workspace_index,
                depth,
                ..
            } => {
                let sig = Signals::of(&app.workspaces[*workspace_index]);
                if let Some(k) = &project_key {
                    out.entry(k.clone()).or_default().absorb(sig);
                }
                // A depth-1 checkout hangs off the header, not off the repo
                // group that happens to precede it.
                if *depth == 2
                    && let Some(k) = &repo_key
                {
                    out.entry(k.clone()).or_default().absorb(sig);
                }
            }
            ProjectTreeRow::Dir { .. } => {}
        }
    }
    out
}

/// Right-aligned metadata spans (agent status glyph, changed-file count,
/// ahead/behind) for a row's signals. `detail_color` styles the Δ/↑↓ text;
/// the status glyph keeps its own semantic color regardless.
fn right_metadata_spans(app: &App, detail_color: Color, sig: &Signals) -> Vec<Span<'static>> {
    let mut right: Vec<Span<'static>> = Vec::new();
    if let Some((status, attention)) = sig.worst_status
        && let Some((glyph, color)) =
            crate::ui::actionable_status_view(&app.theme, status, attention)
    {
        right.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
    }
    if sig.changed > 0 {
        if !right.is_empty() {
            right.push(Span::raw(" "));
        }
        right.push(Span::styled(
            format!("{}∆", sig.changed),
            Style::default().fg(detail_color),
        ));
    }
    if let Some((ahead, behind)) = sig.ahead_behind() {
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

/// Label for a checkout row. Under a repo group the repo name is already on
/// the header, so the child says which *branch* it is — that is the whole
/// point of the tree. `branch == None` (the background refresh hasn't landed,
/// or the directory isn't a git repo) must never leave the row blank, so it
/// falls back to the checkout's own directory name, then its workspace name.
fn checkout_label(ws: &Workspace, depth: u8, hoisted: bool) -> String {
    // A hoisted row IS its repository (its group had a single checkout), so it
    // is named the way the old flat sidebar named a clone: the repository
    // folder, with the branch alongside when one is known.
    if hoisted {
        let folder = ws
            .info
            .source_repo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if ws.info.source_repo_display.is_empty() {
                    ws.name.clone()
                } else {
                    ws.info.source_repo_display.clone()
                }
            });
        return match &ws.branch {
            Some(branch) => format!("{folder} ({branch})"),
            None => folder,
        };
    }
    if let Some(branch) = &ws.branch
        && depth == 2
    {
        return branch.clone();
    }
    if depth == 2 {
        return ws
            .info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ws.name.clone());
    }
    // Depth 1 and not hoisted: a plain-directory workspace or a PR review, so
    // the row carries its own identity plus a branch if it has one.
    match &ws.branch {
        Some(branch) => format!("{} ({branch})", ws.name),
        None => ws.name.clone(),
    }
}

/// Top-left pane: the project tree — projects, their repositories, and each
/// repository's checkouts (main clone + worktrees). The only sidebar view:
/// every registered workspace is reachable here, under a project or under one
/// of the two synthetic buckets (see `piki_core::projects::tree`).
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
    // Muted vertical guide that ties a checkout back to its repo.
    let guide_fg = app.theme.palette.line;
    let header_style = Style::default()
        .fg(theme.name_inactive)
        .add_modifier(Modifier::BOLD);

    let block = Block::default()
        .title(PANE_TITLE)
        .title_style(pane_title_style(app, ActivePane::WorkspaceList))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style);

    let rows = app.sidebar_rows();
    if rows.is_empty() {
        let key_style = Style::default().fg(app.theme.footer.key);
        let desc_style = Style::default().fg(theme.empty_text);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!(" [{}]", app.config.get_binding("projects", "new")),
                    key_style,
                ),
                Span::styled(" New project", desc_style),
            ]),
            Line::from(vec![
                Span::styled(
                    format!(" [{}]", app.config.get_binding("app", "new_workspace")),
                    key_style,
                ),
                Span::styled(" New workspace", desc_style),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines).block(block), area);
        return;
    }

    let visual_rows = app.sidebar_visual_rows();
    let rollups = rollups(app);
    // All rows are one line tall; the wheel scrolls the viewport freely and
    // selection moves pull it along (see `reveal_sidebar_selection`).
    let visible_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = app.sidebar_viewport();
    let inner_w = area.width.saturating_sub(2) as usize;

    let items: Vec<ListItem> = visual_rows
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|slot| {
            let Some(row_idx) = *slot else {
                return ListItem::new(vec![Line::from("")]);
            };
            let row = &rows[row_idx];
            let is_selected = row_idx == app.selected_sidebar_row;
            let detail_color = if is_selected {
                theme.detail_selected
            } else {
                theme.detail_normal
            };
            let bar = if is_selected {
                Span::styled("▎", Style::default().fg(sel_bar_fg))
            } else {
                Span::raw(" ")
            };
            let chevron = |collapsed: bool| {
                Span::styled(
                    format!("{} ", if collapsed { "▸" } else { "▾" }),
                    header_style,
                )
            };

            let spans: Vec<Span> = match row {
                ProjectTreeRow::Project {
                    bucket,
                    key,
                    collapsed,
                    checkouts,
                } => {
                    let (name, dot_color) = match bucket {
                        Bucket::Project(pi) => {
                            let p = &app.sidebar_projects[*pi];
                            (p.name.clone(), app.theme.projects.color(p.clamped_color()))
                        }
                        other => (bucket_label(*other).to_string(), app.theme.palette.fg3),
                    };
                    let count = format!(" {checkouts}");
                    // bar(1) + chevron(2) + dot(2) + trailing count.
                    let avail = inner_w.saturating_sub(5 + count.chars().count());
                    let mut spans = vec![
                        bar,
                        chevron(*collapsed),
                        Span::styled("● ", Style::default().fg(dot_color)),
                        Span::styled(ellipsize_end(&name, avail), header_style),
                        Span::styled(count, Style::default().fg(detail_color)),
                    ];
                    // Collapsed: surface what's hidden underneath.
                    if *collapsed && let Some(sig) = rollups.get(key) {
                        append_signals(&mut spans, app, detail_color, sig);
                    }
                    spans
                }
                ProjectTreeRow::Repo {
                    display,
                    key,
                    collapsed,
                    checkouts,
                    ..
                } => {
                    let avail = inner_w.saturating_sub(7);
                    let mut spans = vec![
                        bar,
                        Span::raw(" "),
                        chevron(*collapsed),
                        Span::styled("⎇ ", Style::default().fg(theme.detail_normal)),
                        Span::styled(
                            ellipsize_end(display, avail),
                            Style::default().fg(theme.name_inactive),
                        ),
                    ];
                    // A repo nobody has opened yet: say so instead of looking
                    // like an empty group.
                    if *checkouts == 0 {
                        spans.push(Span::styled(
                            " (not open)",
                            Style::default().fg(app.theme.palette.fg3),
                        ));
                    }
                    if *collapsed && let Some(sig) = rollups.get(key) {
                        append_signals(&mut spans, app, detail_color, sig);
                    }
                    spans
                }
                ProjectTreeRow::Checkout {
                    workspace_index,
                    depth,
                    hoisted,
                    ..
                } => {
                    let ws = &app.workspaces[*workspace_index];
                    let is_active = *workspace_index == app.active_workspace;
                    // Icon brightness carries a second signal: the active
                    // checkout stays at full brightness, the rest recede to the
                    // same muted token the tree guide uses — so scanning the
                    // list, only where you are pops.
                    let icon_color = if is_selected {
                        theme.detail_selected
                    } else if is_active {
                        theme.detail_normal
                    } else {
                        app.theme.palette.fg3
                    };
                    let mut spans = vec![bar];
                    if *depth == 2 {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled("│ ", Style::default().fg(guide_fg)));
                    } else {
                        spans.push(Span::raw("   "));
                    }
                    spans.push(Span::styled(
                        workspace_type_icon(
                            ws.info.workspace_type,
                            ws.info.is_git_repo,
                            ws.info.ephemeral,
                        ),
                        Style::default().fg(icon_color),
                    ));
                    spans.push(Span::styled(
                        checkout_label(ws, *depth, *hoisted),
                        if is_active {
                            Style::default()
                                .fg(theme.name_active)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.name_inactive)
                        },
                    ));
                    if ws.info.ephemeral {
                        spans.push(Span::styled(
                            " [PR]",
                            Style::default().fg(app.theme.palette.info),
                        ));
                    }
                    append_signals(&mut spans, app, detail_color, &Signals::of(ws));
                    spans
                }
                ProjectTreeRow::Dir { path, .. } => {
                    let avail = inner_w.saturating_sub(6);
                    vec![
                        bar,
                        Span::raw(" "),
                        Span::styled("🗁 ", Style::default().fg(app.theme.palette.fg3)),
                        Span::styled(
                            ellipsize_start(&path.to_string_lossy(), avail),
                            Style::default().fg(app.theme.palette.fg3),
                        ),
                    ]
                }
            };

            let style = if is_selected {
                Style::default().bg(sel_bg)
            } else {
                Style::default()
            };
            ListItem::new(vec![Line::from(spans)]).style(style)
        })
        .collect();

    frame.render_widget(List::new(items).block(block), area);

    super::scrollbar::render_vertical(
        frame,
        area,
        scroll_offset,
        visual_rows.len(),
        visible_height,
        app.theme.general.scrollbar_thumb,
    );
}

/// Append the idle dot plus the metadata run (agent status glyph, changed-file
/// count, ahead/behind) to a row, each only when it says something.
///
/// Placed right after the label instead of right-aligned against the border —
/// right alignment meant a narrow pane (or a long name) pushed it clean off
/// the visible edge; here it's always the next thing rendered, so it survives
/// any pane width down to the label itself getting cut.
fn append_signals(spans: &mut Vec<Span<'static>>, app: &App, detail_color: Color, sig: &Signals) {
    if sig.has_idle {
        spans.push(Span::styled(
            " ●",
            Style::default()
                .fg(app.theme.status.needs_you)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let right = right_metadata_spans(app, detail_color, sig);
    if !right.is_empty() {
        spans.push(Span::raw(" "));
        spans.extend(right);
    }
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
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
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
    let scroll_offset = app.agents_viewport();

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
            // Custom title takes precedence; otherwise show provider label
            // (with Claude prefix for non-Custom shell tabs).
            let label = if tab
                .custom_title
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
            {
                tab.display_label().to_string()
            } else if matches!(tab.provider, piki_core::AIProvider::Custom(_)) {
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
            // Elapsed run time (since session start / last prompt; gone on
            // Stop) — same label the desktop Agents panel shows.
            if let Some(elapsed) = tab.cli_agent_elapsed() {
                spans.push(Span::styled(
                    format!(" {}", piki_core::cli_agent::format_elapsed(elapsed)),
                    row_bg.fg(theme.empty_text),
                ));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ephemeral_icon_overrides_workspace_type() {
        // An ephemeral review checkout is visually distinct regardless of
        // its underlying WorkspaceType, so it never gets mistaken for a
        // regular worktree/project/simple workspace in the sidebar.
        let ephemeral_icon = workspace_type_icon(WorkspaceType::Simple, true, true);
        assert_ne!(
            ephemeral_icon,
            workspace_type_icon(WorkspaceType::Simple, true, false)
        );
        assert_ne!(
            ephemeral_icon,
            workspace_type_icon(WorkspaceType::Worktree, true, false)
        );
        assert_ne!(
            ephemeral_icon,
            workspace_type_icon(WorkspaceType::Project, true, false)
        );
        assert_eq!(
            ephemeral_icon,
            workspace_type_icon(WorkspaceType::Worktree, true, true)
        );
    }
}
