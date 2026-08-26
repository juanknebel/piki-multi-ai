use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;
use tauri::AppHandle;
use tauri::{Emitter, Manager};

use piki_core::cli_agent::CliAgentEvent;
use piki_core::notifications;
use piki_core::pty::ShellSession;
use piki_core::session::client::{Attachment, AttachmentSender};
use piki_core::session::protocol::{Frame, read_frame};
use piki_core::shell_integration::ShellEvent;
use piki_core::shell_integration::parser::OscParser;

use crate::events::PtyAttentionPayload;
use crate::pty_output::{OutMsg, output_channel, spawn_emitter};
use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
struct PtyExitPayload {
    tab_id: String,
    exit_code: Option<i32>,
}

/// `pty-exit` goes out from the emitter thread, after the last output batch
/// (see `pty_output.rs`) — never straight from a reader.
fn emit_exit(app_handle: &AppHandle, tab_id: &str, exit_code: Option<i32>) {
    let _ = app_handle.emit(
        "pty-exit",
        PtyExitPayload {
            tab_id: tab_id.to_string(),
            exit_code,
        },
    );
}

/// Tauri event payload for shell-integration markers (OSC 133/7) extracted
/// from the PTY stream. The `kind` discriminator tells the frontend how to
/// interpret the optional fields.
#[derive(Serialize, Clone, Debug)]
struct PtyShellEventPayload {
    tab_id: String,
    /// One of `prompt-start`, `command-input-start`, `command-output-start`,
    /// `command-end`, `cwd-changed`.
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

/// Tauri event payload for a structured Claude Code lifecycle event
/// (OSC 777, Warp-style). Drives the per-tab agent status glyph + summary.
#[derive(Serialize, Clone, Debug)]
struct PtyAgentEventPayload {
    tab_id: String,
    /// Coarse status: `running`, `waiting-permission`, `idle`, `done`.
    status: &'static str,
    /// The cli-agent event name (`session_start`, `prompt_submit`,
    /// `tool_complete`, `permission_request`, `notification`, `stop`).
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    /// The tab has news the user hasn't looked at (permission / idle /
    /// done) — already false when the event landed on the tab on screen,
    /// which counts as seen. Cleared later by `pty-agent-ack`.
    attention: bool,
}

pub struct RawLocalPty {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    bytes_processed: Arc<AtomicU64>,
    shell: Option<Arc<Mutex<ShellSession>>>,
    /// Out-of-band FIFO reader for the structured cli-agent channel. `Some`
    /// only for Claude tabs spawned with a `cli_agent_sock` path. Its `Drop`
    /// stops the reader and unlinks the FIFO.
    #[cfg(unix)]
    _cli_agent_sock: Option<piki_core::cli_agent::sock::SockReader>,
}

impl RawLocalPty {
    /// Spawn a PTY child. `extra_env` is merged into the inherited login env
    /// (so callers can override defaults — e.g. `PIKI_SHELL_INTEGRATION=1`).
    /// `extra_args` is **prepended** to the command's normal args (needed for
    /// `bash --rcfile <bridge>` where the rcfile flag must come first). With
    /// `enable_shell_integration = true`, the reader spins up an [`OscParser`]
    /// that observes the byte stream, updates the session's [`ShellSession`]
    /// state, and emits `pty-shell-event` Tauri events.
    ///
    /// `cli_agent_sock` (when `Some` *and* `enable_shell_integration`) is the
    /// per-spawn FIFO path the Claude hook scripts write structured lifecycle
    /// events to out-of-band. We start a reader that feeds the same
    /// [`ShellSession`] and, via its callback, drives `handle_cli_agent` (the
    /// same handler the in-band OSC 777 path uses — kept as a fallback).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        app_handle: AppHandle,
        tab_id: String,
        worktree_path: &Path,
        rows: u16,
        cols: u16,
        command: &str,
        args: &[String],
        extra_env: &[(String, String)],
        extra_args: &[String],
        enable_shell_integration: bool,
        cli_agent_sock: Option<std::path::PathBuf>,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size)?;

        // Resolve the command to an absolute path using the user's login shell
        // environment.  portable-pty's built-in PATH search can fail when the
        // app inherits a minimal PATH from a .desktop entry, even after we
        // override the env vars on the CommandBuilder.
        let resolved = piki_core::shell_env::resolve_command(command);
        let mut cmd = CommandBuilder::new(&resolved);
        for prepend in extra_args {
            cmd.arg(prepend);
        }
        cmd.args(args);
        cmd.cwd(worktree_path);

        // Apply user's login shell environment so that PATH, LANG, and other
        // profile-configured variables are available even when launched from
        // a .desktop entry (which provides only a minimal environment).
        for (key, value) in piki_core::shell_env::user_login_env() {
            cmd.env(key, value);
        }
        // Ensure terminal type matches xterm.js capabilities
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // Caller overrides last so they win over inherited values.
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let bytes_processed = Arc::new(AtomicU64::new(0));
        let bytes_clone = Arc::clone(&bytes_processed);
        let shell = if enable_shell_integration {
            Some(Arc::new(Mutex::new(ShellSession::default())))
        } else {
            None
        };
        let shell_for_reader = shell.clone();

        // Clones for the out-of-band cli-agent FIFO callback (the byte reader
        // task below moves `app_handle`/`emit_tab_id`).
        #[cfg(unix)]
        let sock_app_handle = app_handle.clone();
        #[cfg(unix)]
        let sock_tab_id = tab_id.clone();

        // Output is coalesced by an emitter thread (pty_output.rs); the
        // reader only parses shell-integration markers and forwards bytes.
        let (out_tx, batcher) = output_channel();
        spawn_emitter(app_handle.clone(), tab_id.clone(), batcher, emit_exit)?;

        let emit_tab_id = tab_id.clone();
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            let mut osc_parser = OscParser::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = out_tx.send(OutMsg::Exit(Some(0)));
                        break;
                    }
                    Ok(n) => {
                        let chunk = &buf[..n];
                        bytes_clone.fetch_add(n as u64, Ordering::Relaxed);
                        if let Some(ref shell) = shell_for_reader {
                            let events = osc_parser.feed(chunk);
                            if !events.is_empty() {
                                {
                                    let mut s = shell.lock();
                                    for ev in &events {
                                        s.state.apply(ev);
                                    }
                                }
                                for ev in events {
                                    match &ev {
                                        ShellEvent::CommandEnd { exit_code, command } => {
                                            handle_shell_command_end(
                                                &app_handle,
                                                &emit_tab_id,
                                                *exit_code,
                                                command.clone(),
                                            );
                                        }
                                        ShellEvent::CliAgent(a) => {
                                            // Structured agent events ride their
                                            // own `pty-agent-event` channel, not
                                            // `pty-shell-event`.
                                            handle_cli_agent(&app_handle, &emit_tab_id, a);
                                            continue;
                                        }
                                        _ => {}
                                    }
                                    let _ = app_handle.emit(
                                        "pty-shell-event",
                                        shell_event_payload(&emit_tab_id, ev),
                                    );
                                }
                            }
                        }
                        if out_tx.send(OutMsg::Data(chunk.to_vec())).is_err() {
                            break; // emitter gone (app shutting down)
                        }
                    }
                    Err(_) => {
                        let _ = out_tx.send(OutMsg::Exit(None));
                        break;
                    }
                }
            }
        });

        // Out-of-band FIFO transport for the structured cli-agent channel.
        // Only meaningful when shell integration is on (so `shell` is `Some`)
        // and a per-spawn FIFO path was supplied (Claude tabs). The callback
        // mirrors the in-band OSC 777 arm: it drives `handle_cli_agent` for
        // the Tauri/status/notification side; the shared `ShellSession` is fed
        // by the reader itself.
        #[cfg(unix)]
        let cli_agent_sock = match (cli_agent_sock, shell.as_ref()) {
            (Some(path), Some(shell)) => {
                let cb_app = sock_app_handle;
                let cb_tab = sock_tab_id;
                let cb: piki_core::cli_agent::sock::CliAgentCallback =
                    Box::new(move |ev| handle_cli_agent(&cb_app, &cb_tab, ev));
                match piki_core::cli_agent::sock::spawn_reader(path, Arc::clone(shell), Some(cb)) {
                    Ok(reader) => Some(reader),
                    Err(e) => {
                        tracing::warn!(error = %e, "cli-agent FIFO reader failed to start; OSC 777 fallback only");
                        None
                    }
                }
            }
            _ => None,
        };
        #[cfg(not(unix))]
        let _ = cli_agent_sock;

        tracing::info!(command, path = %worktree_path.display(), rows, cols, shell_integration = enable_shell_integration, "Raw PTY spawned");

        Ok(Self {
            child,
            writer,
            reader_handle,
            master: pair.master,
            bytes_processed,
            shell,
            #[cfg(unix)]
            _cli_agent_sock: cli_agent_sock,
        })
    }

    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn kill(&mut self) -> anyhow::Result<()> {
        self.child.kill()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn peek_alive(&self) -> bool {
        !self.reader_handle.is_finished()
    }

    /// Total bytes read from the PTY since spawn.
    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed.load(Ordering::Relaxed)
    }

    /// Per-session shell-integration state (cwd, last command). `Some` only
    /// when the session was spawned with `enable_shell_integration = true`.
    pub fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        self.shell.as_ref()
    }
}

impl Drop for RawLocalPty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.reader_handle.abort();
    }
}

/// Emit the right Tauri events for one shell-integration event — the same
/// mapping the local reader does inline, factored out so the remote reader
/// stays in lock-step: `CliAgent` drives the agent-status rail, `CommandEnd`
/// the attention/notification rail, everything else a plain `pty-shell-event`.
fn dispatch_shell_event(app_handle: &AppHandle, tab_id: &str, ev: ShellEvent) {
    if let ShellEvent::CliAgent(a) = &ev {
        handle_cli_agent(app_handle, tab_id, a);
        return;
    }
    if let ShellEvent::CommandEnd { exit_code, command } = &ev {
        handle_shell_command_end(app_handle, tab_id, *exit_code, command.clone());
    }
    let _ = app_handle.emit("pty-shell-event", shell_event_payload(tab_id, ev));
}

/// A tab backed by the session daemon (persists across app restarts). Its
/// reader thread turns daemon frames into the same Tauri events the local
/// reader emits, so the frontend and xterm.js can't tell the difference.
/// Dropping it DETACHES (the session survives); `kill` kills the child.
pub struct RawRemotePty {
    sender: AttachmentSender,
    bytes_processed: Arc<AtomicU64>,
    alive: Arc<std::sync::atomic::AtomicBool>,
    shell: Option<Arc<Mutex<ShellSession>>>,
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

impl RawRemotePty {
    /// Wrap a daemon [`Attachment`] as a tab, streaming its output to the
    /// frontend as `pty-output` events (the restore buffer included, so the
    /// terminal repaints on attach). `integration_on` mirrors the session's
    /// flag: when set, a client-side [`ShellSession`] is kept in sync from the
    /// daemon's `ShellEvent` frames so the idle watcher / attention rail work.
    pub fn start(
        app_handle: AppHandle,
        tab_id: String,
        att: Attachment,
        integration_on: bool,
    ) -> Self {
        use std::sync::atomic::AtomicBool;

        let bytes_processed = Arc::new(AtomicU64::new(0));
        let alive = Arc::new(AtomicBool::new(att.info.state.is_live()));
        let shell = integration_on.then(|| {
            Arc::new(Mutex::new(ShellSession {
                state: piki_core::shell_integration::ShellTabState::from_snapshot(&att.shell),
                pending_events: Vec::new(),
            }))
        });
        let sender = att.sender();

        let (mut read, _sender) = att.into_read_half();
        // Same coalescing pipeline as the local reader; a failure to start
        // the emitter thread only means this tab never paints.
        let (out_tx, batcher) = output_channel();
        if let Err(e) = spawn_emitter(app_handle.clone(), tab_id.clone(), batcher, emit_exit) {
            tracing::error!(error = %e, "pty output emitter thread failed to start");
        }
        let reader_handle = {
            let bytes = Arc::clone(&bytes_processed);
            let alive = Arc::clone(&alive);
            let shell = shell.clone();
            std::thread::Builder::new()
                .name("raw-remote-pty-reader".into())
                .spawn(move || {
                    loop {
                        match read_frame(&mut read) {
                            Ok(Frame::Restore(b)) | Ok(Frame::Output(b)) => {
                                bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
                                if out_tx.send(OutMsg::Data(b)).is_err() {
                                    break;
                                }
                            }
                            Ok(Frame::ShellEvent { event, .. }) => {
                                if let Some(ref shell) = shell {
                                    let mut s = shell.lock();
                                    s.state.apply(&event);
                                    s.pending_events.push(event.clone());
                                }
                                dispatch_shell_event(&app_handle, &tab_id, event);
                            }
                            Ok(Frame::Exited { code }) => {
                                alive.store(false, Ordering::Relaxed);
                                let _ = out_tx.send(OutMsg::Exit(code));
                            }
                            Ok(Frame::Detached { .. }) => {
                                alive.store(false, Ordering::Relaxed);
                                let _ = out_tx.send(OutMsg::Exit(None));
                                break;
                            }
                            Ok(_) => {} // heartbeat / unexpected
                            Err(_) => {
                                alive.store(false, Ordering::Relaxed);
                                let _ = out_tx.send(OutMsg::Exit(None));
                                break;
                            }
                        }
                    }
                })
                .ok()
        };

        RawRemotePty {
            sender,
            bytes_processed,
            alive,
            shell,
            reader_handle,
        }
    }

    fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.sender.input(data)?;
        Ok(())
    }

    fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        self.sender.resize(rows, cols)?;
        Ok(())
    }

    fn kill(&mut self) -> anyhow::Result<()> {
        self.sender.kill()?;
        Ok(())
    }

    fn is_alive(&mut self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    fn peek_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    fn bytes_processed(&self) -> u64 {
        self.bytes_processed.load(Ordering::Relaxed)
    }

    fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        self.shell.as_ref()
    }

    fn resync(&self) -> anyhow::Result<()> {
        self.sender.resync()?;
        Ok(())
    }
}

impl Drop for RawRemotePty {
    fn drop(&mut self) {
        // Detach, never kill — the session must survive us.
        let _ = self.sender.detach();
        if let Some(h) = self.reader_handle.take() {
            drop(h); // exits on socket close; don't block
        }
    }
}

/// A desktop PTY tab, either owned in-process ([`Local`](Self::Local)) or
/// living in the session daemon ([`Remote`](Self::Remote)). Same public
/// surface either way; the app picks `Remote` when the daemon is reachable.
pub enum RawPtySession {
    Local(RawLocalPty),
    Remote(RawRemotePty),
}

impl RawPtySession {
    /// Spawn an in-process PTY (the fallback path). Signature unchanged from
    /// the original struct.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        app_handle: AppHandle,
        tab_id: String,
        worktree_path: &Path,
        rows: u16,
        cols: u16,
        command: &str,
        args: &[String],
        extra_env: &[(String, String)],
        extra_args: &[String],
        enable_shell_integration: bool,
        cli_agent_sock: Option<std::path::PathBuf>,
    ) -> anyhow::Result<Self> {
        Ok(RawPtySession::Local(RawLocalPty::spawn(
            app_handle,
            tab_id,
            worktree_path,
            rows,
            cols,
            command,
            args,
            extra_env,
            extra_args,
            enable_shell_integration,
            cli_agent_sock,
        )?))
    }

    /// Wrap a daemon attachment as a remote-backed tab.
    pub fn from_attachment(
        app_handle: AppHandle,
        tab_id: String,
        att: Attachment,
        integration_on: bool,
    ) -> Self {
        RawPtySession::Remote(RawRemotePty::start(app_handle, tab_id, att, integration_on))
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, RawPtySession::Remote(_))
    }

    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        match self {
            RawPtySession::Local(l) => l.write(data),
            RawPtySession::Remote(r) => r.write(data),
        }
    }

    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        match self {
            RawPtySession::Local(l) => l.resize(rows, cols),
            RawPtySession::Remote(r) => r.resize(rows, cols),
        }
    }

    pub fn kill(&mut self) -> anyhow::Result<()> {
        match self {
            RawPtySession::Local(l) => l.kill(),
            RawPtySession::Remote(r) => r.kill(),
        }
    }

    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        match self {
            RawPtySession::Local(l) => l.is_alive(),
            RawPtySession::Remote(r) => r.is_alive(),
        }
    }

    pub fn peek_alive(&self) -> bool {
        match self {
            RawPtySession::Local(l) => l.peek_alive(),
            RawPtySession::Remote(r) => r.peek_alive(),
        }
    }

    pub fn bytes_processed(&self) -> u64 {
        match self {
            RawPtySession::Local(l) => l.bytes_processed(),
            RawPtySession::Remote(r) => r.bytes_processed(),
        }
    }

    pub fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        match self {
            RawPtySession::Local(l) => l.shell(),
            RawPtySession::Remote(r) => r.shell(),
        }
    }

    /// Ask the daemon to re-send the restore buffer (a no-op for a local
    /// session). The frontend calls this when a terminal mounts so a
    /// re-attached tab repaints even though its restore arrived before the
    /// xterm existed.
    pub fn resync(&self) -> anyhow::Result<()> {
        match self {
            RawPtySession::Local(_) => Ok(()),
            RawPtySession::Remote(r) => r.resync(),
        }
    }
}

/// Handle an OSC 133 `command-end` marker on a shell tab: emit a
/// `pty-attention` event for the sidebar badge and fire an OS notification
/// (always, regardless of which workspace is active). Workspace lookup walks
/// `DesktopApp.workspaces` by `tab_id`; if the tab can't be found (e.g. it
/// was closed between read and dispatch) only the attention event is skipped.
fn handle_shell_command_end(
    app_handle: &AppHandle,
    tab_id: &str,
    exit_code: Option<i32>,
    command: Option<String>,
) {
    let Some(state) = app_handle.try_state::<Mutex<DesktopApp>>() else {
        return;
    };
    let (workspace_idx, workspace_name, from_active_view) = {
        let app = state.lock();
        let Some((idx, ws)) = app
            .workspaces
            .iter()
            .enumerate()
            .find(|(_, ws)| ws.tabs.iter().any(|t| t.id == tab_id))
        else {
            return;
        };
        let active_view = app.active_workspace == idx
            && ws.tabs.get(ws.active_tab).map(|t| t.id.as_str()) == Some(tab_id);
        (idx, ws.info.name.clone(), active_view)
    };
    let _ = app_handle.emit(
        "pty-attention",
        PtyAttentionPayload {
            workspace_idx,
            tab_id: tab_id.to_string(),
            source: "shell-command-end",
        },
    );
    // `tab_id` is the desktop UUID — globally unique → perfect mailbox origin.
    notifications::notify_command_end(
        tab_id,
        &workspace_name,
        exit_code,
        command.as_deref(),
        from_active_view,
    );
}

/// Map a structured cli-agent event to its `(status, kind, summary)` UI
/// triple and whether it warrants pulling the user's attention.
fn cli_agent_view(ev: &CliAgentEvent) -> (&'static str, &'static str, Option<String>, bool) {
    match ev {
        CliAgentEvent::SessionStart { .. } => ("running", "session_start", None, false),
        CliAgentEvent::UserPromptSubmit { .. } => ("running", "prompt_submit", None, false),
        CliAgentEvent::PostToolUse { .. } => ("running", "tool_complete", None, false),
        CliAgentEvent::PermissionRequest { summary, .. } => (
            "waiting-permission",
            "permission_request",
            Some(summary.clone()),
            true,
        ),
        CliAgentEvent::Notification { .. } => ("idle", "notification", None, true),
        CliAgentEvent::Stop { response, .. } => ("done", "stop", response.clone(), true),
    }
}

/// Where a cli-agent event's tab lives, resolved under the `DesktopApp` lock.
struct LocatedTab {
    workspace_idx: usize,
    workspace_name: String,
    icon: Option<String>,
    from_active_view: bool,
    /// The event's attention marker was cleared on the spot because the
    /// user is looking at this very tab (TUI semantics: seen as it lands).
    acked: bool,
}

/// Handle a structured Claude Code lifecycle event: always push a
/// `pty-agent-event` (per-tab status glyph + `attention` flag), and for the
/// attention-worthy ones (`permission_request`, `notification`, `stop`) also
/// ride the shared attention rail — `pty-attention` for the sidebar badge
/// plus a de-duped OS notification (regardless of which workspace is
/// active). News landing on the tab on screen is acknowledged immediately,
/// so the amber "needs you" only ever points somewhere the user isn't.
fn handle_cli_agent(app_handle: &AppHandle, tab_id: &str, ev: &CliAgentEvent) {
    let (status, kind, summary, needs_attention) = cli_agent_view(ev);

    // Workspace lookup walks `DesktopApp.workspaces` by `tab_id`; a tab that
    // was closed between read and dispatch still gets its status event but
    // skips the attention rail.
    let located = app_handle
        .try_state::<Mutex<DesktopApp>>()
        .and_then(|state| {
            let app = state.lock();
            let (idx, ws) = app
                .workspaces
                .iter()
                .enumerate()
                .find(|(_, ws)| ws.tabs.iter().any(|t| t.id == tab_id))?;
            let tab = ws.tabs.iter().find(|t| t.id == tab_id)?;
            let icon = app
                .provider_manager
                .get(tab.provider.label())
                .and_then(|c| c.icon.clone());
            let from_active_view = app.active_workspace == idx
                && ws.tabs.get(ws.active_tab).map(|t| t.id.as_str()) == Some(tab_id);
            let acked = from_active_view
                && needs_attention
                && crate::events::acknowledge_agent_attention(tab);
            Some(LocatedTab {
                workspace_idx: idx,
                workspace_name: ws.info.name.clone(),
                icon,
                from_active_view,
                acked,
            })
        });

    let _ = app_handle.emit(
        "pty-agent-event",
        PtyAgentEventPayload {
            tab_id: tab_id.to_string(),
            status,
            kind,
            summary: summary.clone(),
            attention: needs_attention && !located.as_ref().is_some_and(|l| l.acked),
        },
    );

    if !needs_attention {
        return;
    }
    let Some(located) = located else {
        return;
    };

    let _ = app_handle.emit(
        "pty-attention",
        PtyAttentionPayload {
            workspace_idx: located.workspace_idx,
            tab_id: tab_id.to_string(),
            source: "cli-agent",
        },
    );
    // `tab_id` is the desktop UUID — globally unique → perfect mailbox origin.
    notifications::notify_cli_agent(
        tab_id,
        &located.workspace_name,
        kind,
        summary.as_deref(),
        located.icon.as_deref(),
        located.from_active_view,
    );
}

fn shell_event_payload(tab_id: &str, event: ShellEvent) -> PtyShellEventPayload {
    let mut p = PtyShellEventPayload {
        tab_id: tab_id.to_string(),
        kind: "",
        exit_code: None,
        cwd: None,
    };
    match event {
        ShellEvent::PromptStart => p.kind = "prompt-start",
        ShellEvent::CommandInputStart => p.kind = "command-input-start",
        ShellEvent::CommandOutputStart => p.kind = "command-output-start",
        ShellEvent::CommandEnd { exit_code, .. } => {
            p.kind = "command-end";
            p.exit_code = exit_code;
        }
        ShellEvent::CwdChanged(path) => {
            p.kind = "cwd-changed";
            p.cwd = Some(path.display().to_string());
        }
        // M0: the structured agent event rides the same channel but the
        // frontend doesn't consume it yet (that's M1 — per-tab status UI).
        ShellEvent::CliAgent(_) => p.kind = "cli-agent",
        // Passive agent-state detection (`agent_state_detect`) is TUI-only
        // for now; desktop has no consumer for window-title events yet.
        ShellEvent::WindowTitle(_) => p.kind = "window-title",
    }
    p
}
