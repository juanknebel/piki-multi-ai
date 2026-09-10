//! Tauri commands backing the desktop's Sessions dialog: list every session
//! the persistent-session daemon holds (including ones no local tab shows),
//! and kill/remove one. Mirrors the TUI's sessions overlay (`prefix ctrl-s`)
//! and the `sessions` CLI; the daemon is shared, so this lists what the TUI
//! opened too.

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, State};

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
    /// Loaded workspace whose path is the session's recorded workspace, if
    /// any — the default target when adopting it as a tab.
    pub workspace_idx: Option<usize>,
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

/// Daemon health for the status bar segment. `off` = disabled (Settings ▸
/// General override or `config.toml`) when this process started;
/// `unavailable` = enabled but no daemon answers (never connected, or it
/// died since — the list call is the liveness probe, so a manually killed
/// daemon shows within one poll); `on` = answering, with the number of live
/// sessions it holds (all clients, not just this window). `enabled_next` is
/// what the NEXT launch will use (DB > config.toml > default, re-read on
/// every poll) so the bar can flag a pending change made in Settings.
#[derive(Serialize)]
pub struct SessionStatus {
    pub state: &'static str,
    pub live: usize,
    pub daemon_pid: Option<u32>,
    pub enabled_next: bool,
}

#[tauri::command]
pub async fn session_status(state: State<'_, Mutex<DesktopApp>>) -> Result<SessionStatus, String> {
    let (enabled, enabled_next, daemon, daemon_pid) = {
        let app = state.lock();
        let next = piki_core::app_settings::resolve(
            &app.paths.config_path(),
            app.storage.ui_prefs.as_deref(),
        )
        .sessions_enabled;
        (
            app.sessions_enabled,
            next,
            app.session_daemon.clone(),
            read_daemon_pid(&app),
        )
    };
    if !enabled {
        return Ok(SessionStatus {
            state: "off",
            live: 0,
            daemon_pid: None,
            enabled_next,
        });
    }
    let Some(daemon) = daemon else {
        return Ok(SessionStatus {
            state: "unavailable",
            live: 0,
            daemon_pid,
            enabled_next,
        });
    };
    // Socket round-trip off the main thread; the status bar polls this.
    let listed = tauri::async_runtime::spawn_blocking(move || daemon.list()).await;
    Ok(match listed {
        Ok(Ok(list)) => SessionStatus {
            state: "on",
            live: list.iter().filter(|s| s.state.is_live()).count(),
            daemon_pid,
            enabled_next,
        },
        _ => SessionStatus {
            state: "unavailable",
            live: 0,
            daemon_pid,
            enabled_next,
        },
    })
}

/// What startup re-attach restored (see `session::RestoreSummary`).
#[tauri::command]
pub fn restore_summary(state: State<'_, Mutex<DesktopApp>>) -> crate::session::RestoreSummary {
    state.lock().restore_summary.clone()
}

/// Live tabs split by what quitting does to them: daemon-backed ones keep
/// running, in-process ones die with the window.
#[derive(Serialize)]
pub struct QuitSummary {
    pub persistent: usize,
    pub local: usize,
}

#[tauri::command]
pub fn quit_summary(state: State<'_, Mutex<DesktopApp>>) -> QuitSummary {
    let app = state.lock();
    let mut out = QuitSummary {
        persistent: 0,
        local: 0,
    };
    for tab in app.workspaces.iter().flat_map(|w| w.tabs.iter()) {
        let Some(pty) = tab.pty.as_ref() else {
            continue;
        };
        if !tab.alive {
            continue;
        }
        if pty.is_remote() {
            out.persistent += 1;
        } else {
            out.local += 1;
        }
    }
    out
}

/// Adopt an orphan (detached) session as a tab of `workspace_idx` — TUI
/// parity (`attach_orphan`). Attaches, builds the tab exactly like startup
/// re-attach, appends it and returns its tab index so the frontend can jump.
#[tauri::command]
pub async fn adopt_session(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    session_id: String,
    workspace_idx: usize,
) -> Result<usize, String> {
    let daemon = daemon_of(&state)?;
    {
        let app = state.lock();
        if workspace_idx >= app.workspaces.len() {
            return Err("Workspace index out of range".to_string());
        }
        if app
            .workspaces
            .iter()
            .any(|w| w.tabs.iter().any(|t| t.id == session_id))
        {
            return Err("That session is already open as a tab here".to_string());
        }
    }
    let id = session_id.clone();
    let d = daemon.clone();
    let info = tauri::async_runtime::spawn_blocking(move || d.list())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| "Session no longer exists".to_string())?;
    let (rows, cols, id) = (info.rows.max(24), info.cols.max(80), info.id.clone());
    let att = tauri::async_runtime::spawn_blocking(move || daemon.attach(&id, rows, cols))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let mut app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    let tab = crate::session::tab_from_session(&app_handle, &info, att, &app.provider_manager);
    let ws = &mut app.workspaces[workspace_idx];
    ws.tabs.push(tab);
    ws.active_tab = ws.tabs.len() - 1;
    Ok(ws.active_tab)
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
            let workspace_idx = app
                .workspaces
                .iter()
                .position(|w| w.info.path == info.meta.workspace_path);
            SessionRow {
                id: info.id,
                name,
                workspace,
                state,
                attached: info.attached,
                exit_code,
                local_workspace_idx,
                local_tab_idx,
                workspace_idx,
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
