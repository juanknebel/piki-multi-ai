use std::sync::Arc;

use ratatui::layout::Rect;

use crate::app::{self, App};
use crate::clipboard;
use crate::ui;
use piki_core::pty::PtySession;
use piki_core::AIProvider;

/// Kill all PTY sessions and drop watchers for a clean exit.
pub(crate) fn shutdown(app: &mut App) {
    for ws in &mut app.workspaces {
        for tab in &mut ws.tabs {
            if let Some(ref mut pty) = tab.pty_session {
                let _ = pty.kill();
            }
        }
        ws.tabs.clear();
        ws.watcher = None;
    }
}

/// Spawn an initial Shell tab for a workspace.
pub(crate) async fn spawn_initial_shell(ws: &mut app::Workspace, rows: u16, cols: u16) {
    let idx = ws.add_tab(AIProvider::Shell, false); // first shell is not closable
    let cmd = AIProvider::Shell.resolved_command();
    match PtySession::spawn(&ws.path, rows, cols, &cmd).await {
        Ok(session) => {
            ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
            ws.tabs[idx].pty_session = Some(session);
            ws.status = app::WorkspaceStatus::Busy;
        }
        Err(_) => {}
    }
}

/// Spawn a new tab with the given provider in a workspace.
pub(crate) async fn spawn_tab(
    ws: &mut app::Workspace,
    provider: AIProvider,
    rows: u16,
    cols: u16,
) -> usize {
    let idx = ws.add_tab(provider, true);
    if provider == AIProvider::Kanban {
        return idx;
    }
    let cmd = provider.resolved_command();
    match PtySession::spawn(&ws.path, rows, cols, &cmd).await {
        Ok(session) => {
            ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
            ws.tabs[idx].pty_session = Some(session);
        }
        Err(_) => {}
    }
    idx
}

/// Probe the actual scrollback buffer size by setting a large offset and reading back.
/// `scrollback()` returns the current offset (which is always 0 after render reset),
/// so we temporarily set it to MAX, read the clamped value, then restore to 0.
pub(crate) fn scrollback_max(parser: &Arc<parking_lot::Mutex<vt100::Parser>>) -> usize {
    let mut guard = parser.lock();
    guard.screen_mut().set_scrollback(usize::MAX);
    let max = guard.screen().scrollback();
    guard.screen_mut().set_scrollback(0);
    max
}

pub(crate) fn copy_visible_terminal(app: &mut App) {
    if let Some(ws) = app.workspaces.get(app.active_workspace)
        && let Some(tab) = ws.current_tab()
        && let Some(ref parser) = tab.pty_parser
    {
        let mut guard = parser.lock();
        guard.screen_mut().set_scrollback(tab.term_scroll);
        let text = guard.screen().contents();
        guard.screen_mut().set_scrollback(0);
        drop(guard);
        match clipboard::copy_to_clipboard(&text) {
            Ok(()) => {
                app.status_message = Some("Terminal content copied".into());
            }
            Err(e) => {
                app.status_message = Some(format!("Copy failed: {}", e));
            }
        }
    }
}

/// Recompute PTY dimensions after sidebar resize and resize all PTY sessions
pub(crate) fn resize_all_ptys(app: &mut App) {
    // We need the current terminal size; use cached pty dimensions as proxy
    // The actual resize will happen on next render via compute_terminal_area_with
    // For now, just mark that a resize is needed — the Event::Resize path handles actual PTY resize
    // But we can estimate from the stored terminal_inner_area
    if let Some(inner) = app.terminal_inner_area {
        // Rough estimate: total width = inner.width / old_main_pct * 100
        let total_width = inner.x + inner.width + 2; // approximate
        let total_height = inner.y + inner.height + 4; // approximate (tabs+subtabs+status+footer+borders)
        let new_area = ui::layout::compute_terminal_area_with(
            Rect::new(0, 0, total_width, total_height),
            app.sidebar_pct,
        );
        app.pty_rows = new_area.height;
        app.pty_cols = new_area.width;
        for ws in &mut app.workspaces {
            for tab in &mut ws.tabs {
                if let Some(ref mut pty) = tab.pty_session {
                    let _ = pty.resize(new_area.height, new_area.width);
                }
            }
        }
    }
}

/// Check if a point (col, row) is inside a Rect
pub(crate) fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Calculate which workspace tab index was clicked based on column position
pub(crate) fn tab_index_at(app: &App, col: u16, area: Rect) -> Option<usize> {
    let mut x = area.x;
    for (i, ws) in app.workspaces.iter().enumerate() {
        let tab_width = ws.name.len() as u16 + 2; // " name "
        if col >= x && col < x + tab_width {
            return Some(i);
        }
        x += tab_width + 1; // +1 for "|" divider
    }
    None
}

/// Calculate which sub-tab index was clicked and whether the close button was hit
pub(crate) fn subtab_index_at(app: &App, col: u16, area: Rect) -> Option<(usize, bool)> {
    let ws = app.current_workspace()?;
    let mut x = area.x;
    for (i, tab) in ws.tabs.iter().enumerate() {
        let label = tab
            .markdown_label
            .as_deref()
            .unwrap_or(tab.provider.label());
        // Matches subtabs.rs: format!(" {}{} ", label, close_marker) where close_marker = " ×" or ""
        // Display widths: " " (1) + label (ascii len) + " ×" (2 display cols) + " " (1) = label.len() + 4
        // Without close: " " (1) + label + " " (1) = label.len() + 2
        let tab_display_width = if tab.closable {
            label.len() as u16 + 4 // " label × "
        } else {
            label.len() as u16 + 2 // " label "
        };
        if col >= x && col < x + tab_display_width {
            // Close button is the last 2 display columns before trailing space: "× "
            let on_close = tab.closable && col >= x + tab_display_width - 3;
            return Some((i, on_close));
        }
        x += tab_display_width + 1; // +1 for "|" divider
    }
    None
}
