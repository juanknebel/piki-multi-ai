//! Tauri commands backing the desktop's Sessions dialog: list every session
//! the persistent-session daemon holds (including ones no local tab shows),
//! and kill/remove one. Mirrors the TUI's sessions overlay (`prefix ctrl-s`)
//! and the `sessions` CLI; the daemon is shared, so this lists what the TUI
//! opened too.

use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use crate::state::DesktopApp;

/// How a daemon session relates to this desktop instance.
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum SessionRowState {
    /// Live and open as a tab in this window.
    Attached,
    /// Live but no local tab holds it (another client's, or orphaned).
    Detached,
    /// The process has exited; the daemon retains the record.
    Exited,
}

/// One row of the Sessions dialog. Mirrored by `SessionRow` in
/// `frontend/src/types.ts`.
#[derive(Serialize)]
pub struct SessionRow {
    pub id: String,
    /// Custom title > provider label > command basename.
    pub name: String,
    /// Workspace folder name, or "—" when its path has none.
    pub workspace: String,
    state: SessionRowState,
    /// Clients attached to this session across all frontends.
    pub attached: usize,
    /// Exit code when exited (else null).
    pub exit_code: Option<i32>,
    /// Workspace + tab index when this session is a tab in THIS window, so the
    /// frontend can jump to it. `null` for detached/exited sessions.
    pub local_workspace_idx: Option<usize>,
    pub local_tab_idx: Option<usize>,
}

/// Whether the daemon is reachable + its pid (for the dialog title).
#[derive(Serialize)]
pub struct SessionsSnapshot {
    pub connected: bool,
    pub daemon_pid: Option<u32>,
    pub sessions: Vec<SessionRow>,
    /// Set when the daemon is connected but the list call failed.
    pub error: Option<String>,
}

fn read_daemon_pid(app: &DesktopApp) -> Option<u32> {
    std::fs::read_to_string(app.paths.session_pid_file())
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Whether this instance spawns tabs in the session daemon. The frontend
/// uses it to decide if "Close, keep running" is on offer at all (a local
/// tab has nothing to keep).
#[tauri::command]
pub fn sessions_available(state: State<'_, Mutex<DesktopApp>>) -> bool {
    state.lock().session_daemon.is_some()
}

#[tauri::command]
pub fn list_sessions(state: State<'_, Mutex<DesktopApp>>) -> SessionsSnapshot {
    let app = state.lock();
    let Some(daemon) = app.session_daemon.clone() else {
        return SessionsSnapshot {
            connected: false,
            daemon_pid: None,
            sessions: Vec::new(),
            error: None,
        };
    };
    let daemon_pid = read_daemon_pid(&app);

    let mut list = match daemon.list() {
        Ok(l) => l,
        Err(e) => {
            return SessionsSnapshot {
                connected: true,
                daemon_pid,
                sessions: Vec::new(),
                error: Some(e.to_string()),
            };
        }
    };
    list.sort_by(|a, b| {
        (&a.meta.workspace_path, a.meta.order).cmp(&(&b.meta.workspace_path, b.meta.order))
    });

    let sessions = list
        .into_iter()
        .map(|info| {
            // Is this session open as a tab in this window?
            let local = app.workspaces.iter().enumerate().find_map(|(wi, ws)| {
                ws.tabs
                    .iter()
                    .position(|t| t.id == info.id)
                    .map(|ti| (wi, ti))
            });
            let (state, exit_code) = match info.state {
                piki_core::session::protocol::SessionState::Exited { code } => {
                    (SessionRowState::Exited, code)
                }
                _ if local.is_some() => (SessionRowState::Attached, None),
                _ => (SessionRowState::Detached, None),
            };
            let name = info
                .meta
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .or_else(|| {
                    Some(info.meta.provider.clone()).filter(|p| !p.is_empty() && p != "Shell")
                })
                .unwrap_or_else(|| {
                    std::path::Path::new(&info.command)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| info.command.clone())
                });
            let workspace = info
                .meta
                .workspace_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "—".to_string());
            let (local_workspace_idx, local_tab_idx) = match local {
                Some((wi, ti)) => (Some(wi), Some(ti)),
                None => (None, None),
            };
            SessionRow {
                id: info.id,
                name,
                workspace,
                state,
                attached: info.attached,
                exit_code,
                local_workspace_idx,
                local_tab_idx,
            }
        })
        .collect();

    SessionsSnapshot {
        connected: true,
        daemon_pid,
        sessions,
        error: None,
    }
}

/// Clone the daemon handle without holding the app lock across socket I/O.
fn daemon_of(
    state: &State<'_, Mutex<DesktopApp>>,
) -> Result<piki_core::session::client::Daemon, String> {
    state
        .lock()
        .session_daemon
        .clone()
        .ok_or_else(|| "session daemon not connected".to_string())
}

/// Kill one session's process (kept in the list as exited). Returns the
/// refreshed snapshot.
#[tauri::command]
pub fn kill_session(
    state: State<'_, Mutex<DesktopApp>>,
    session_id: String,
) -> Result<SessionsSnapshot, String> {
    daemon_of(&state)?
        .kill(&session_id)
        .map_err(|e| e.to_string())?;
    Ok(list_sessions(state))
}

/// Remove one session from the daemon entirely (kills + drops the record).
#[tauri::command]
pub fn remove_session(
    state: State<'_, Mutex<DesktopApp>>,
    session_id: String,
) -> Result<SessionsSnapshot, String> {
    daemon_of(&state)?
        .remove(&session_id)
        .map_err(|e| e.to_string())?;
    Ok(list_sessions(state))
}
