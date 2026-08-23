//! Desktop side of the persistent-session daemon: connect/launch it, re-attach
//! its sessions on startup, and spawn a tab through it. Mirrors the TUI's
//! `helpers` glue (docs/persistent-sessions.md), but wires attachments to the
//! desktop's `RawPtySession::Remote` (which streams to xterm.js over Tauri
//! events) instead of a vt100 parser.

use std::process::Stdio;

use tauri::AppHandle;

use piki_core::AIProvider;
use piki_core::paths::DataPaths;
use piki_core::session::client::{ClientError, Daemon, ensure_daemon};
use piki_core::session::protocol::{SessionMeta, SpawnRequest};

use crate::pty_raw::RawPtySession;
use crate::state::{DesktopTab, DesktopWorkspace};

/// Connect to — or launch — the session daemon for `paths`. `None` when
/// sessions are disabled in `config.toml` or unavailable/incompatible; the
/// caller then runs tabs in-process (Local), exactly as before this feature
/// existed.
pub fn connect_session_daemon(paths: &DataPaths) -> Option<Daemon> {
    if !piki_core::session::sessions_enabled(&paths.config_path()) {
        tracing::info!("persistent sessions disabled in config.toml; in-process tabs");
        return None;
    }
    let socket = paths.session_socket();
    let base = paths.base().to_path_buf();
    let launch = move || -> std::io::Result<()> {
        let exe = std::env::current_exe()?;
        std::process::Command::new(exe)
            .arg("--serve-sessions")
            .arg("--data-dir")
            .arg(&base)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    };
    match ensure_daemon(&socket, launch) {
        Ok(daemon) => {
            tracing::info!("session daemon ready; desktop tabs will persist");
            Some(daemon)
        }
        Err(ClientError::Incompatible { daemon_protocol }) => {
            tracing::warn!(
                daemon_protocol,
                "session daemon protocol mismatch; in-process tabs"
            );
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "session daemon unavailable; in-process tabs");
            None
        }
    }
}

/// Re-attach the daemon's persisted sessions to `workspaces` on startup. Each
/// session whose `workspace_path` matches a workspace becomes a `DesktopTab`
/// again (ordered by stored `order`), its reader streaming the restored screen
/// to the frontend once the terminal mounts and requests a resync. Sessions
/// with no matching workspace are left running.
pub fn reattach_sessions(
    app_handle: &AppHandle,
    daemon: &Daemon,
    workspaces: &mut [DesktopWorkspace],
    provider_manager: &piki_core::providers::ProviderManager,
) {
    let mut sessions = match daemon.list() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "could not list sessions to re-attach");
            return;
        }
    };
    sessions.sort_by_key(|s| (s.meta.workspace_path.clone(), s.meta.order));

    let mut reattached = 0usize;
    for info in sessions {
        let Some(ws) = workspaces
            .iter_mut()
            .find(|w| w.info.path == info.meta.workspace_path)
        else {
            continue; // orphan: keep it running
        };
        if ws.tabs.iter().any(|t| t.id == info.id) {
            continue;
        }
        let provider = AIProvider::from_label(&info.meta.provider);
        let provider_cfg = if let AIProvider::Custom(name) = &provider {
            provider_manager.get(name)
        } else {
            None
        };
        let att = match daemon.attach(&info.id, info.rows.max(24), info.cols.max(80)) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(session = %info.id, error = %e, "re-attach failed");
                continue;
            }
        };
        let mut tab = DesktopTab::new(provider, provider_cfg);
        tab.id = info.id.clone();
        tab.custom_title = info.meta.title.clone();
        tab.pty = Some(RawPtySession::from_attachment(
            app_handle.clone(),
            info.id.clone(),
            att,
            info.integration_on,
        ));
        tab.alive = info.state.is_live();
        ws.tabs.push(tab);
        reattached += 1;
    }
    if reattached > 0 {
        tracing::info!(count = reattached, "re-attached persisted desktop sessions");
    }
}

/// Try to spawn a tab in the daemon; returns the `Remote` session on success.
/// The caller falls back to a local PTY on `None`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_remote_tab(
    app_handle: &AppHandle,
    daemon: &Daemon,
    tab_id: &str,
    provider: &AIProvider,
    command: &str,
    args: &[String],
    env: &[(String, String)],
    extra_args: &[String],
    cwd: &std::path::Path,
    integration_on: bool,
    cli_agent_sock: Option<std::path::PathBuf>,
    order: u32,
) -> Option<RawPtySession> {
    // Full child environment (the daemon env_clears): login env, then the
    // plan's overrides, with a sane TERM for xterm.js.
    let mut full_env: Vec<(String, String)> = piki_core::shell_env::user_login_env()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    full_env.push(("TERM".into(), "xterm-256color".into()));
    full_env.push(("COLORTERM".into(), "truecolor".into()));
    full_env.extend(env.iter().cloned());

    let mut all_args = extra_args.to_vec();
    all_args.extend(args.iter().cloned());

    let req = SpawnRequest {
        id: tab_id.to_string(),
        command: piki_core::shell_env::resolve_command(command),
        args: all_args,
        cwd: cwd.to_path_buf(),
        env: full_env,
        rows: 24,
        cols: 80,
        integration_on,
        cli_agent_sock,
        meta: SessionMeta {
            workspace_path: cwd.to_path_buf(),
            provider: provider.label().to_string(),
            title: None,
            order,
            closable: true,
        },
        attach: true,
    };
    match daemon.spawn_attach(req) {
        Ok(att) => Some(RawPtySession::from_attachment(
            app_handle.clone(),
            tab_id.to_string(),
            att,
            integration_on,
        )),
        Err(e) => {
            tracing::warn!(error = %e, "daemon spawn failed; falling back to in-process PTY");
            None
        }
    }
}

/// Remove a session from the daemon (kills its child, drops the record).
/// Fire-and-forget on a blocking task.
pub fn remove_session(daemon: &Daemon, session_id: &str) {
    let daemon = daemon.clone();
    let id = session_id.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = daemon.remove(&id) {
            tracing::warn!(session = %id, error = %e, "failed to remove session");
        }
    });
}
