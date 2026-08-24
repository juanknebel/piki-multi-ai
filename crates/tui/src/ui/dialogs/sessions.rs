//! Sessions overlay (`prefix ctrl-s`): every session the persistent-session
//! daemon holds, with kill/remove/adopt management. Pure render — all data
//! (including the daemon pid) lives in [`DialogState::Sessions`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::dialog_state::DialogState;
use piki_core::session::protocol::{SessionInfo, SessionState};

/// How one session relates to this app instance.
enum RowKind {
    /// Live and open as a tab here.
    Attached,
    /// Live but no local tab holds it (another client's, or orphaned).
    Detached,
    Exited(Option<i32>),
}

fn row_kind(app: &App, info: &SessionInfo) -> RowKind {
    if let SessionState::Exited { code } = info.state {
        return RowKind::Exited(code);
    }
    let local = app.workspaces.iter().any(|ws| {
        ws.tabs
            .iter()
            .any(|t| t.session_id.as_deref() == Some(info.id.as_str()))
    });
    if local {
        RowKind::Attached
    } else {
        RowKind::Detached
    }
}

pub(crate) fn render_sessions_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let Some(DialogState::Sessions {
        loading,
        ref error,
        ref sessions,
        selected,
        scroll_offset,
        daemon_pid,
    }) = app.active_dialog
    else {
        return;
    };

    let theme = &app.theme;
    let width = area.width * 80 / 100;
    let height = area.height * 70 / 100;
    let popup = super::clear_popup(frame, area, width.max(50), height.max(9));

    let inner_height = popup.height.saturating_sub(2) as usize; // borders
    let visible_rows = inner_height.saturating_sub(2); // hints + blank

    let muted = Style::default().fg(theme.palette.fg3);
    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(
            " [Enter] jump [x] kill [d] remove [r] refresh [Esc] close",
            muted,
        )),
        Line::from(""),
    ];

    let mut live = 0usize;
    let mut orphans = 0usize;

    if loading {
        lines.push(Line::from(Span::styled(" loading…", muted)));
    } else if let Some(err) = error {
        lines.push(Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(theme.status.error),
        )));
    } else if sessions.is_empty() {
        lines.push(Line::from(Span::styled(" no sessions", muted)));
    } else {
        let mut rows: Vec<Line<'_>> = Vec::with_capacity(sessions.len());
        for (i, info) in sessions.iter().enumerate() {
            let kind = row_kind(app, info);
            let (glyph, glyph_color, state_text, state_color) = match &kind {
                RowKind::Attached => {
                    live += 1;
                    (
                        "▷",
                        theme.status.running,
                        format!("attached {}", info.attached),
                        theme.status.running,
                    )
                }
                RowKind::Detached => {
                    live += 1;
                    orphans += 1;
                    let text = if info.attached > 0 {
                        format!("attached {} (elsewhere)", info.attached)
                    } else {
                        "detached".to_string()
                    };
                    ("⚠", theme.status.needs_you, text, theme.status.needs_you)
                }
                RowKind::Exited(code) => (
                    "○",
                    theme.status.exited,
                    match code {
                        Some(c) => format!("exited {c}"),
                        None => "exited".to_string(),
                    },
                    theme.palette.fg3,
                ),
            };

            // Display name: user title > provider label > command.
            let name = info
                .meta
                .title
                .clone()
                .filter(|t| !t.is_empty())
                .or_else(|| {
                    Some(info.meta.provider.clone()).filter(|p| !p.is_empty() && p != "Shell")
                })
                .unwrap_or_else(|| info.command.clone());
            let name: String = name.chars().take(22).collect();
            let ws_name = info
                .meta
                .workspace_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "—".to_string());
            let ws_name: String = ws_name.chars().take(18).collect();

            let bg = if i == selected {
                theme.workspace_list.selected_bg
            } else {
                Color::Reset
            };
            rows.push(Line::from(vec![
                Span::styled(" ", Style::default().bg(bg)),
                Span::styled(format!("{glyph} "), Style::default().fg(glyph_color).bg(bg)),
                Span::styled(
                    format!("{name:<23}"),
                    Style::default()
                        .fg(theme.palette.fg1)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{ws_name:<19}"),
                    Style::default().fg(theme.palette.fg2).bg(bg),
                ),
                Span::styled(state_text, Style::default().fg(state_color).bg(bg)),
            ]));
        }
        let scroll = scroll_offset.min(rows.len().saturating_sub(visible_rows.max(1)));
        lines.extend(rows.into_iter().skip(scroll).take(visible_rows.max(1)));
    }

    let daemon_label = match (app.session_daemon.is_some(), daemon_pid) {
        (true, Some(pid)) => format!("Sessions — daemon ● pid {pid}"),
        (true, None) => "Sessions — daemon ●".to_string(),
        (false, _) => "Sessions — daemon ○ not connected".to_string(),
    };
    let counts = if sessions.is_empty() {
        String::new()
    } else {
        let mut c = format!(" {} session(s) · {} live", sessions.len(), live);
        if orphans > 0 {
            c.push_str(&format!(" · {orphans} unattached"));
        }
        c.push_str(&format!(" [{}/{}] ", selected + 1, sessions.len()));
        c
    };

    let block = super::popup_block(&daemon_label, theme.help.border)
        .title_bottom(Line::from(counts).right_aligned());
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
