use std::sync::Arc;

use ratatui::layout::Rect;

use crate::app::{self, App};
use crate::clipboard;
use crate::ui;
use piki_core::AIProvider;
use piki_core::cli_agent::bridge_for_command;
use piki_core::pty::PtySession;
use piki_core::session::client::Daemon;
use piki_core::session::protocol::{SessionMeta, SetMetaRequest, SpawnRequest};

/// Tear down on quit. Dropping each tab's [`PtySession`] does the right thing
/// per backend: a **Remote** (daemon) session DETACHES and keeps running so it
/// can be re-attached next launch; a **Local** (in-process) session is killed.
/// We therefore must NOT call `kill()` here — persistence is the whole point —
/// just drop the tabs and let their `Drop` decide.
pub(crate) fn shutdown(app: &mut App) {
    for ws in &mut app.workspaces {
        ws.tabs.clear();
        ws.watcher = None;
    }
}

/// Connect to — or launch — the session daemon for `paths`. Returns `None`
/// when sessions are unavailable or the daemon speaks another protocol; the
/// caller then runs every tab in-process (Local), exactly as before this
/// feature existed.
pub(crate) fn connect_session_daemon(paths: &piki_core::paths::DataPaths) -> Option<Daemon> {
    use piki_core::session::client::{ClientError, ensure_daemon};
    use std::process::Stdio;

    let socket = paths.session_socket();
    let base = paths.base().to_path_buf();
    // The daemon is this same binary re-invoked with `serve`, pinned to the
    // same data dir so it computes the same socket path.
    let launch = move || -> std::io::Result<()> {
        let exe = std::env::current_exe()?;
        std::process::Command::new(exe)
            .arg("--data-dir")
            .arg(&base)
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    };

    match ensure_daemon(&socket, launch) {
        Ok(daemon) => {
            tracing::info!("session daemon ready; tabs will persist");
            Some(daemon)
        }
        Err(ClientError::Incompatible { daemon_protocol }) => {
            tracing::warn!(
                daemon_protocol,
                "session daemon protocol mismatch; running tabs in-process"
            );
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "session daemon unavailable; running tabs in-process");
            None
        }
    }
}

/// Re-attach the daemon's persisted sessions to their workspaces on startup.
/// Each session whose `workspace_path` matches a loaded workspace becomes a
/// tab again (ordered by its stored `order`), with its screen + scrollback
/// restored. Sessions with no matching workspace are left running (visible via
/// `sessions list` / the sessions dialog), never killed.
pub(crate) fn reattach_sessions(app: &mut App) {
    let Some(daemon) = app.session_daemon.clone() else {
        return;
    };
    let mut sessions = match daemon.list() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "could not list sessions to re-attach");
            return;
        }
    };
    // Attach in stored tab order so the subtab bar comes back as it was.
    sessions.sort_by_key(|s| (s.meta.workspace_path.clone(), s.meta.order));

    let (rows, cols) = (app.pty_rows, app.pty_cols);
    let mut reattached = 0usize;
    for info in sessions {
        let Some(ws_idx) = app
            .workspaces
            .iter()
            .position(|w| w.info.path == info.meta.workspace_path)
        else {
            continue; // orphan: keep it running, don't adopt it here
        };
        // Already have this session as a tab? (defensive — startup runs once)
        if app.workspaces[ws_idx]
            .tabs
            .iter()
            .any(|t| t.session_id.as_deref() == Some(info.id.as_str()))
        {
            continue;
        }

        let provider = AIProvider::from_label(&info.meta.provider);
        let provider_cfg = if let AIProvider::Custom(name) = &provider {
            app.provider_manager.get(name)
        } else {
            None
        };

        let att = match daemon.attach(&info.id, rows, cols) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(session = %info.id, error = %e, "re-attach failed");
                continue;
            }
        };
        let pty =
            PtySession::from_attachment(att, info.integration_on, Some(app.pty_output.clone()));
        let ws = &mut app.workspaces[ws_idx];
        let idx = ws.add_tab(provider, info.meta.closable, provider_cfg);
        ws.tabs[idx].pty_parser = Some(Arc::clone(pty.parser()));
        ws.tabs[idx].pty_session = Some(pty);
        ws.tabs[idx].session_id = Some(info.id.clone());
        if let Some(title) = info.meta.title.clone() {
            ws.tabs[idx].custom_title = Some(title);
        }
        ws.status = app::WorkspaceStatus::Busy;
        ws.dirty = true;
        ws.last_refresh = None;
        reattached += 1;
    }
    if reattached > 0 {
        tracing::info!(count = reattached, "re-attached persisted sessions");
    }
}

/// Remove a session from the daemon (its child is killed and the record
/// dropped). Fire-and-forget on a blocking task; errors are logged only. Use
/// when a tab is explicitly closed or its workspace deleted.
pub(crate) fn remove_session(app: &App, session_id: &str) {
    let Some(daemon) = app.session_daemon.clone() else {
        return;
    };
    let id = session_id.to_string();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = daemon.remove(&id) {
            tracing::warn!(session = %id, error = %e, "failed to remove session");
        }
    });
}

/// Push a renamed tab's title to its daemon session so the rename survives a
/// restart. Fire-and-forget; no-op without a daemon or session id.
pub(crate) fn rename_session(app: &App, session_id: &str, title: Option<String>) {
    let Some(daemon) = app.session_daemon.clone() else {
        return;
    };
    let req = SetMetaRequest {
        id: session_id.to_string(),
        set_title: true,
        title,
        ..Default::default()
    };
    tokio::task::spawn_blocking(move || {
        if let Err(e) = daemon.set_meta(req) {
            tracing::debug!(error = %e, "failed to sync tab title to session");
        }
    });
}

/// Tools a provider's hook bridge needs but that aren't on PATH, with the
/// agent's display name. `None` when the provider has no bridge (nothing to
/// degrade) or when everything it needs is installed.
///
/// The tab spawns either way — [`spawn_tab`] just falls back to the byte-silence
/// idle heuristic — so callers use this to *warn*, never to block.
pub(crate) fn missing_bridge_prereqs(
    provider: &AIProvider,
    provider_manager: &piki_core::providers::ProviderManager,
) -> Option<(String, Vec<String>)> {
    let AIProvider::Custom(name) = provider else {
        return None;
    };
    let cmd = provider_manager.get(name).map(|c| c.command.clone())?;
    let bridge = bridge_for_command(&cmd)?;
    let missing = piki_core::cli_agent::missing_prerequisites(bridge);
    if missing.is_empty() {
        return None;
    }
    Some((
        bridge.label().to_string(),
        missing.into_iter().map(String::from).collect(),
    ))
}

/// Fire-and-forget save of the current workspace list under `source`'s key.
/// `save_workspaces` only touches rows whose `source_repo` matches, so callers
/// must pass the repo of the workspace that changed.
///
/// The save is detached, so there is no `&mut App` left to toast with — a
/// failure is routed back through `status_tx` (the event loop toasts it),
/// because a silent failure here means the user's workspaces vanish on the
/// next start.
pub(crate) fn persist_workspaces(app: &app::App, source: std::path::PathBuf) {
    let infos = app.persistable_workspaces();
    let storage = Arc::clone(&app.storage);
    let status_tx = app.status_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = storage.workspaces.save_workspaces(&source, &infos) {
            tracing::error!(
                source = %source.display(),
                error = %e,
                "failed to persist workspaces — changes will be lost on restart"
            );
            let _ = status_tx.send(format!(
                "Failed to save workspace list — changes may be lost on restart: {e}"
            ));
        }
    });
}

/// Spawn a new tab with the given provider in a workspace.
/// For Custom providers, `provider_manager` is used to resolve the command and prompt args.
///
/// Returns the new tab's index plus, when the PTY failed to start, a message
/// describing why. The tab is kept either way so the user has something to
/// close; callers MUST surface the message — a blank dead tab with no
/// explanation is the single most confusing failure this app can produce
/// (a missing or non-executable provider binary looks identical to a hang).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_tab(
    ws: &mut app::Workspace,
    provider: &AIProvider,
    rows: u16,
    cols: u16,
    prompt: Option<&str>,
    provider_manager: Option<&piki_core::providers::ProviderManager>,
    paths: &piki_core::paths::DataPaths,
    daemon: Option<Daemon>,
    output_signal: piki_core::pty::PtyOutputSignal,
) -> (usize, Option<String>) {
    // Resolve the provider's `providers.toml` entry up front so its
    // per-provider idle knobs drive the tab's IdleWatcher (re-used below for
    // command/arg resolution).
    let provider_cfg = if let AIProvider::Custom(name) = provider {
        provider_manager.and_then(|m| m.get(name))
    } else {
        None
    };
    let idx = ws.add_tab(provider.clone(), true, provider_cfg);
    // Kanban / CodeReview / Api render from app state, not a PTY.
    if *provider == AIProvider::Kanban || *provider == AIProvider::CodeReview {
        return (idx, None);
    }
    if *provider == AIProvider::Api {
        ws.tabs[idx].api_state = Some(app::ApiTabState::new());
        return (idx, None);
    }

    // Everything about how to start this tab — command, args, env, shell
    // integration, cli-agent channel — is resolved by core so the desktop app
    // makes the identical decisions (see core::pty::launch).
    // `shell_override` is None: the TUI has no shell setting yet (the desktop
    // reads one from its UI prefs). Adding one is now just a config key — the
    // launch side already honours it.
    //
    // When a session daemon is available the tab gets a persistent id up
    // front so its cli-agent FIFO is named after the session (stable across a
    // frontend restart); otherwise a process-unique FIFO is fine.
    let session_id = piki_core::session::new_session_id();
    let plan = match piki_core::pty::launch_plan_for_session(
        provider,
        prompt,
        provider_manager,
        paths,
        None,
        daemon.as_ref().map(|_| session_id.as_str()),
    ) {
        Ok(plan) => plan,
        Err(e) => return (idx, Some(e.to_string())),
    };

    // Preferred path: spawn the tab in the session daemon so it survives us.
    // Any failure degrades to an in-process PTY with a log line (never a
    // hard error — the tab must still come up).
    if let Some(daemon) = daemon {
        match spawn_remote(
            daemon,
            &session_id,
            provider,
            &plan,
            &ws.path,
            rows,
            cols,
            idx,
            output_signal.clone(),
        )
        .await
        {
            Ok(session) => {
                ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
                ws.tabs[idx].pty_session = Some(session);
                ws.tabs[idx].session_id = Some(session_id);
                ws.status = app::WorkspaceStatus::Busy;
                ws.dirty = true;
                ws.last_refresh = None;
                return (idx, None);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "session daemon spawn failed; falling back to an in-process PTY"
                );
            }
        }
    }

    let cmd = plan.command;
    let spawn_error = match PtySession::spawn(
        &ws.path,
        rows,
        cols,
        &cmd,
        &plan.args,
        &plan.env,
        &plan.extra_args,
        plan.integration_on,
        plan.cli_agent_sock,
        Some(output_signal),
    )
    .await
    {
        Ok(session) => {
            ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
            ws.tabs[idx].pty_session = Some(session);
            ws.status = app::WorkspaceStatus::Busy;
            None
        }
        Err(e) => {
            // Usually a missing or non-executable provider binary. Without
            // this the tab just sits there blank forever.
            tracing::warn!(%cmd, error = %e, "failed to spawn PTY for tab");
            Some(format!("Could not start '{cmd}': {e}"))
        }
    };
    // A tool (shell/agent/git) just activated for this workspace — that's
    // the trigger for inferring its branch (never persisted, see
    // `ws.branch`), so kick the background refresh loop immediately instead
    // of waiting for its next periodic tick.
    ws.dirty = true;
    ws.last_refresh = None;
    (idx, spawn_error)
}

/// Spawn a tab in the session daemon and wrap the attachment as a `Remote`
/// [`PtySession`]. Blocking daemon calls run on a blocking task so the async
/// event loop is never stalled.
#[allow(clippy::too_many_arguments)]
async fn spawn_remote(
    daemon: Daemon,
    session_id: &str,
    provider: &AIProvider,
    plan: &piki_core::pty::LaunchPlan,
    cwd: &std::path::Path,
    rows: u16,
    cols: u16,
    order: usize,
    output_signal: piki_core::pty::PtyOutputSignal,
) -> Result<PtySession, piki_core::session::client::ClientError> {
    // The daemon env_clears, so send the full child environment: our process
    // env (PATH/HOME/…) with the launch plan's overrides applied last.
    let mut env: Vec<(String, String)> = std::env::vars().collect();
    env.push(("TERM".to_string(), "xterm-256color".to_string()));
    env.extend(plan.env.iter().cloned());
    // Shell-integration / hook-bridge args are prepended before the
    // provider's own args (matching the local spawn's ordering).
    let mut args = plan.extra_args.clone();
    args.extend(plan.args.iter().cloned());

    let req = SpawnRequest {
        id: session_id.to_string(),
        // Resolve to an absolute path: the daemon env_cleared, so relying on
        // its PATH to find a bare command is fragile.
        command: piki_core::shell_env::resolve_command(&plan.command),
        args,
        cwd: cwd.to_path_buf(),
        env,
        rows,
        cols,
        integration_on: plan.integration_on,
        cli_agent_sock: plan.cli_agent_sock.clone(),
        meta: SessionMeta {
            workspace_path: cwd.to_path_buf(),
            provider: provider.label().to_string(),
            title: None,
            order: order as u32,
            closable: true,
        },
        attach: true,
    };

    let integration_on = plan.integration_on;
    let att = tokio::task::spawn_blocking(move || daemon.spawn_attach(req))
        .await
        .map_err(|e| {
            piki_core::session::client::ClientError::Io(std::io::Error::other(e.to_string()))
        })??;
    Ok(PtySession::from_attachment(
        att,
        integration_on,
        Some(output_signal),
    ))
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
                app.set_toast("Terminal content copied", crate::app::ToastLevel::Success);
            }
            Err(e) => {
                app.set_toast(format!("Copy failed: {}", e), crate::app::ToastLevel::Error);
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

/// What a click on the tab bar landed on.
pub(crate) enum SubtabHit {
    /// Tab index + whether the close button (`×`) was hit.
    Tab(usize, bool),
    /// The trailing `+` (new tab) button.
    NewTab,
}

/// Calculate what was clicked in the tab bar. Geometry comes from
/// `ui::subtabs::layout`, the same function the renderer uses, so the hit
/// regions can't drift from the pixels (including the overflow window and
/// its `‹N`/`N›` indicators, which activate the nearest hidden tab).
pub(crate) fn subtab_index_at(app: &App, col: u16, area: Rect) -> Option<SubtabHit> {
    let ws = app.current_workspace()?;
    let lay = ui::subtabs::layout(ws, &app.theme, area.width);
    let rel = col.checked_sub(area.x)?;

    if let Some((x, w)) = lay.left
        && rel >= x
        && rel < x + w
    {
        // Step to the nearest clipped tab on the left.
        return Some(SubtabHit::Tab(lay.hidden_left - 1, false));
    }
    for &(i, x, w) in &lay.blocks {
        if rel >= x && rel < x + w {
            let tab = &ws.tabs[i];
            // The block ends with " ×" (2 cols) then a trailing space (1 col).
            // The close target is just those two `" ×"` columns; excluding the
            // trailing space keeps a click in the padding from closing the tab.
            let on_close = tab.closable && rel >= x + w - 3 && rel < x + w - 1;
            return Some(SubtabHit::Tab(i, on_close));
        }
    }
    if let Some((x, w)) = lay.right
        && rel >= x
        && rel < x + w
    {
        // Step to the nearest clipped tab on the right.
        let last_visible = lay.blocks.last().map(|&(i, _, _)| i)?;
        return Some(SubtabHit::Tab(last_visible + 1, false));
    }
    if rel >= lay.plus_x && rel < lay.plus_x + 3 {
        return Some(SubtabHit::NewTab);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{add_test_workspace, test_app};

    fn bogus_provider(name: &str, command: &str) -> piki_core::providers::ProviderConfig {
        piki_core::providers::ProviderConfig {
            name: name.to_string(),
            description: String::new(),
            command: command.to_string(),
            default_args: Vec::new(),
            prompt_format: piki_core::providers::PromptFormat::Positional,
            dispatchable: false,
            agent_dir: None,
            idle_threshold_secs: None,
            idle_notify: false,
            icon: None,
        }
    }

    /// A Custom provider that just runs `cat` — no bridge, no shell
    /// integration, so it touches no disk; perfect for exercising the
    /// daemon/remote path with a deterministic echo.
    fn cat_provider(name: &str) -> piki_core::providers::ProviderConfig {
        bogus_provider(name, "cat")
    }

    fn ws_info_at(name: &str, path: &std::path::Path) -> piki_core::WorkspaceInfo {
        let mut info = crate::test_support::test_ws_info(name, 0);
        info.path = path.to_path_buf();
        info.source_repo = path.to_path_buf();
        info
    }

    /// The headline feature: a daemon-backed tab survives the frontend and is
    /// re-attached (with its screen restored) on the next launch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn remote_tab_persists_across_a_restart() {
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::time::{Duration, Instant};

        // An in-process session daemon on a temp socket (no fork).
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = piki_core::session::daemon::Server::new();
        let serve = server.clone();
        let daemon_thread = std::thread::spawn(move || {
            let _ = serve.serve(listener);
        });
        for _ in 0..200 {
            if UnixStream::connect(&socket).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let daemon = piki_core::session::client::Daemon::new(socket);

        let ws_path = dir.path().join("ws");
        std::fs::create_dir_all(&ws_path).unwrap();

        let contents = |app: &App, wi: usize, ti: usize| -> String {
            app.workspaces[wi].tabs[ti]
                .pty_session
                .as_ref()
                .unwrap()
                .parser()
                .lock()
                .screen()
                .contents()
        };
        let wait_for = |app: &App, wi: usize, ti: usize, needle: &str| -> bool {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if contents(app, wi, ti).contains(needle) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            false
        };

        // ── Session 1: spawn a remote tab and type into it. ──
        let (mut app, _tmp) = crate::test_support::test_app_isolated();
        app.session_daemon = Some(daemon.clone());
        app.provider_manager.upsert(cat_provider("kitty"));
        app.workspaces
            .push(crate::app::Workspace::from_info(ws_info_at(
                "proj", &ws_path,
            )));
        app.pty_rows = 24;
        app.pty_cols = 80;

        let (idx, err) = spawn_tab(
            &mut app.workspaces[0],
            &AIProvider::Custom("kitty".into()),
            24,
            80,
            None,
            Some(&app.provider_manager),
            &app.paths,
            app.session_daemon.clone(),
            app.pty_output.clone(),
        )
        .await;
        assert!(err.is_none(), "remote spawn should succeed: {err:?}");
        let sid = app.workspaces[0].tabs[idx]
            .session_id
            .clone()
            .expect("a remote tab carries a daemon session id");
        assert!(
            app.workspaces[0].tabs[idx]
                .pty_session
                .as_ref()
                .unwrap()
                .is_remote(),
            "the tab is daemon-backed"
        );

        app.workspaces[0].tabs[idx]
            .pty_session
            .as_mut()
            .unwrap()
            .write(b"persisted-line\n")
            .unwrap();
        assert!(
            wait_for(&app, 0, idx, "persisted-line"),
            "the write should echo into the tab's parser"
        );

        // ── Restart: dropping the app DETACHES its remote tabs (they survive). ──
        drop(app);
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            daemon
                .list()
                .unwrap()
                .iter()
                .any(|s| s.id == sid && s.state.is_live() && s.attached == 0),
            "the session must survive the frontend, detached"
        );

        // ── Session 2: startup re-attach rebuilds the tab with its screen. ──
        let (mut app2, _tmp2) = crate::test_support::test_app_isolated();
        app2.session_daemon = Some(daemon.clone());
        app2.provider_manager.upsert(cat_provider("kitty"));
        app2.workspaces
            .push(crate::app::Workspace::from_info(ws_info_at(
                "proj", &ws_path,
            )));
        app2.pty_rows = 24;
        app2.pty_cols = 80;

        reattach_sessions(&mut app2);
        assert_eq!(
            app2.workspaces[0].tabs.len(),
            1,
            "the persisted session came back as a tab"
        );
        assert_eq!(
            app2.workspaces[0].tabs[0].session_id.as_deref(),
            Some(sid.as_str())
        );
        assert!(
            app2.workspaces[0].tabs[0]
                .pty_session
                .as_ref()
                .unwrap()
                .is_remote()
        );
        assert!(
            wait_for(&app2, 0, 0, "persisted-line"),
            "the re-attached tab restores the earlier screen"
        );

        // Cleanup.
        let _ = daemon.remove(&sid);
        server.request_shutdown();
        let _ = daemon_thread.join();
    }

    /// A tab whose provider isn't configured must say so, not sit there blank.
    #[tokio::test]
    async fn spawn_tab_reports_an_unknown_provider() {
        let mut app = test_app();
        let ws_idx = add_test_workspace(&mut app);
        let paths = piki_core::paths::DataPaths::default_paths();
        let signal = piki_core::pty::PtyOutputSignal::new();

        let (idx, err) = spawn_tab(
            &mut app.workspaces[ws_idx],
            &AIProvider::Custom("does-not-exist".into()),
            24,
            80,
            None,
            None,
            &paths,
            None,
            signal,
        )
        .await;

        assert_eq!(idx, 0, "the tab is still created so the user can close it");
        let err = err.expect("an unconfigured provider must report why");
        assert!(err.contains("does-not-exist"), "{err}");
    }

    /// The headline case: a provider whose binary doesn't exist used to leave
    /// a blank, dead tab with no message at all.
    #[tokio::test]
    async fn spawn_tab_reports_a_missing_binary() {
        let mut app = test_app();
        let ws_idx = add_test_workspace(&mut app);
        app.provider_manager.upsert(bogus_provider(
            "ghost",
            "/nonexistent/definitely-not-a-real-binary",
        ));
        let paths = piki_core::paths::DataPaths::default_paths();
        let signal = piki_core::pty::PtyOutputSignal::new();

        let (_idx, err) = spawn_tab(
            &mut app.workspaces[ws_idx],
            &AIProvider::Custom("ghost".into()),
            24,
            80,
            None,
            Some(&app.provider_manager),
            &paths,
            None,
            signal,
        )
        .await;

        let err = err.expect("a missing binary must report why");
        assert!(
            err.contains("definitely-not-a-real-binary"),
            "the message must name the command: {err}"
        );
    }

    /// Kanban/CodeReview/Api tabs render from app state and never spawn a
    /// PTY — they must not be reported as failures.
    #[tokio::test]
    async fn spawn_tab_succeeds_for_ptyless_providers() {
        let mut app = test_app();
        let ws_idx = add_test_workspace(&mut app);
        let paths = piki_core::paths::DataPaths::default_paths();

        for provider in [AIProvider::Kanban, AIProvider::CodeReview, AIProvider::Api] {
            let (_idx, err) = spawn_tab(
                &mut app.workspaces[ws_idx],
                &provider,
                24,
                80,
                None,
                None,
                &paths,
                None,
                piki_core::pty::PtyOutputSignal::new(),
            )
            .await;
            assert!(err.is_none(), "{provider:?} should not report an error");
        }
    }
}
