//! Session-daemon actions backing the sessions overlay (`prefix ctrl-s`):
//! list what the daemon holds, kill/remove one session, adopt an orphan as a
//! tab. Every daemon call is blocking socket I/O, so it runs on
//! `spawn_blocking`; the list result lands in `App::pending_sessions_list`
//! and the event loop folds it into the open dialog.

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, ToastLevel};
use crate::dialog_state::DialogState;
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::LoadSessions => load_sessions(app),
        Action::SessionKill(id) => {
            with_daemon(app, id, "kill", |daemon, id| daemon.kill(id)).await;
        }
        Action::SessionRemove(id) => {
            with_daemon(app, id, "remove", |daemon, id| daemon.remove(id)).await;
        }
        Action::SessionAttach(id) => attach_orphan(app, &id).await,
        _ => {}
    }
    Ok(())
}

/// Kick off a background `daemon.list()`; the result is folded into the open
/// `DialogState::Sessions` by the event loop.
fn load_sessions(app: &mut App) {
    let Some(daemon) = app.session_daemon.clone() else {
        // No daemon: resolve the dialog synchronously to its error state.
        if let Some(DialogState::Sessions { loading, error, .. }) = &mut app.active_dialog {
            *loading = false;
            *error = Some("session daemon not connected (disabled or unavailable)".to_string());
        }
        return;
    };
    let slot = std::sync::Arc::clone(&app.pending_sessions_list);
    tokio::task::spawn_blocking(move || {
        let result = daemon.list().map_err(|e| e.to_string());
        *slot.lock() = Some(result);
    });
}

/// Run one blocking daemon call for `id`, toast on failure, then reload the
/// list so the overlay reflects the new state.
async fn with_daemon<F>(app: &mut App, id: String, what: &'static str, op: F)
where
    F: FnOnce(&piki_core::session::client::Daemon, &str) -> piki_core::session::client::Result<()>
        + Send
        + 'static,
{
    let Some(daemon) = app.session_daemon.clone() else {
        return;
    };
    let result = tokio::task::spawn_blocking(move || op(&daemon, &id)).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => app.set_toast(format!("session {what} failed: {e}"), ToastLevel::Error),
        Err(e) => app.set_toast(format!("session {what} failed: {e}"), ToastLevel::Error),
    }
    load_sessions(app);
}

/// Adopt an orphan session as a tab: into its recorded workspace when that
/// workspace is loaded, else into the active one. Closes the overlay and
/// jumps to the new tab on success.
async fn attach_orphan(app: &mut App, id: &str) {
    let Some(daemon) = app.session_daemon.clone() else {
        return;
    };
    // The dialog already holds this session's info from the last list.
    let info = match &app.active_dialog {
        Some(DialogState::Sessions { sessions, .. }) => {
            sessions.iter().find(|s| s.id == id).cloned()
        }
        _ => None,
    };
    let Some(info) = info else {
        return;
    };
    let ws_idx = app
        .workspaces
        .iter()
        .position(|w| w.info.path == info.meta.workspace_path)
        .unwrap_or(app.active_workspace);
    if app.workspaces.is_empty() {
        app.set_toast("no workspace to attach the session to", ToastLevel::Error);
        return;
    }
    match crate::helpers::attach_session_as_tab(app, &daemon, &info, ws_idx) {
        Some(tab_idx) => {
            app.active_dialog = None;
            app.mode = crate::app::AppMode::Normal;
            app.switch_workspace_and_focus(ws_idx);
            app.workspaces[ws_idx].active_tab = tab_idx;
        }
        None => {
            app.set_toast("could not attach session", ToastLevel::Error);
            load_sessions(app);
        }
    }
}
