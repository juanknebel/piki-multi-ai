use parking_lot::Mutex;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::session::client::{Attachment, AttachmentSender};
use crate::session::protocol::{Frame, read_frame};
use crate::shell_integration::parser::OscParser;
use crate::shell_integration::{ShellEvent, ShellTabState};

/// Per-session shell-integration state shared between the PTY reader thread
/// (which mutates) and consumers (which read and drain pending events).
#[derive(Debug, Default)]
pub struct ShellSession {
    pub state: ShellTabState,
    pub pending_events: Vec<ShellEvent>,
}

impl ShellSession {
    /// Drain accumulated events for forwarding (e.g. as Tauri events).
    pub fn drain_events(&mut self) -> Vec<ShellEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

/// Coalesced "some PTY produced output" signal, shared by every session of a
/// frontend: reader threads call [`raise`](Self::raise) after flushing bytes
/// into their parser, and only the first raise after a consumer
/// [`take`](Self::take) actually notifies — so an event loop gets exactly one
/// wakeup per batch of output no matter how many sessions are streaming.
#[derive(Clone, Default)]
pub struct PtyOutputSignal {
    dirty: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl PtyOutputSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Producer side (reader threads): flag new output, waking the consumer
    /// only on the first raise since its last `take`.
    pub fn raise(&self) {
        if !self.dirty.swap(true, Ordering::AcqRel) {
            self.notify.notify_one();
        }
    }

    /// Consumer side: clear the flag (re-arming `raise` to notify again) and
    /// report whether output had arrived since the last call.
    pub fn take(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// Resolves on the next [`raise`](Self::raise) after a
    /// [`take`](Self::take). Cancel-safe: `tokio::sync::Notify` stores the
    /// permit, so a raise landing between polls is not lost.
    pub async fn notified(&self) {
        self.notify.notified().await
    }
}

/// A PTY session owned in this process (the classic path). Backs
/// [`PtySession::Local`].
pub struct LocalPty {
    child: Box<dyn portable_pty::Child + Send>,
    /// Outbound bytes for the child. A dedicated writer thread drains this
    /// queue, so neither the UI thread nor the reader thread ever blocks on
    /// a full PTY input queue (see [`Self::write`]).
    writer_tx: std::sync::mpsc::Sender<Vec<u8>>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    bytes_processed: Arc<AtomicU64>,
    /// Present iff this session was spawned with shell integration enabled.
    /// Reader thread parses OSC sequences and mutates this; UI threads read.
    shell: Option<Arc<Mutex<ShellSession>>>,
    /// Out-of-band FIFO reader for the structured cli-agent channel. `Some`
    /// only for tabs spawned with a `cli_agent_sock` path (Claude tabs, and
    /// shell tabs so a manually-run `claude` can report too). Its `Drop`
    /// stops the reader and unlinks the FIFO.
    #[cfg(unix)]
    _cli_agent_sock: Option<crate::cli_agent::sock::SockReader>,
}

/// Start the per-session writer thread and return its input queue.
///
/// The thread exits when every sender is dropped (session + reader thread
/// gone) or on the first write error (child closed the slave side).
fn spawn_writer_thread(
    mut writer: Box<dyn Write + Send>,
) -> anyhow::Result<std::sync::mpsc::Sender<Vec<u8>>> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new()
        .name("pty-writer".to_string())
        .spawn(move || {
            for chunk in rx {
                if let Err(e) = writer.write_all(&chunk).and_then(|_| writer.flush()) {
                    tracing::debug!(error = %e, "PTY writer: write failed, stopping");
                    break;
                }
            }
        })?;
    Ok(tx)
}

impl LocalPty {
    /// Spawn an AI assistant in a PTY inside the given worktree directory.
    ///
    /// `extra_env` is merged into the child environment after the inherited
    /// vars (so callers can override defaults). `extra_args` is prepended to
    /// `args` — useful for `bash --rcfile <bridge>` where the rcfile flag
    /// must come before any user-supplied args. Pass `enable_shell_integration =
    /// true` to spin up an OSC parser that observes the byte stream and
    /// updates the per-tab [`ShellTabState`].
    ///
    /// `cli_agent_sock` (when `Some` *and* `enable_shell_integration`) is the
    /// per-spawn FIFO path the Claude hook scripts write structured lifecycle
    /// events to out-of-band; we start a reader that feeds the same
    /// [`ShellSession`] the OSC parser feeds (the in-band OSC 777 path stays as
    /// a fallback).
    ///
    /// `output_signal` (usually one shared instance per frontend) is raised by
    /// the reader thread after each parser flush, so an event loop can sleep
    /// until output actually arrives instead of polling byte counters.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        worktree_path: &Path,
        rows: u16,
        cols: u16,
        command: &str,
        args: &[String],
        extra_env: &[(String, String)],
        extra_args: &[String],
        enable_shell_integration: bool,
        cli_agent_sock: Option<std::path::PathBuf>,
        output_signal: Option<PtyOutputSignal>,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size)?;

        let mut cmd = CommandBuilder::new(command);
        for prepend in extra_args {
            cmd.arg(prepend);
        }
        cmd.args(args);
        cmd.cwd(worktree_path);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd)?;
        // Drop the slave side — the child process owns it now
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer_tx = spawn_writer_thread(pair.master.take_writer()?)?;
        let writer_for_reader = writer_tx.clone();

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let parser_clone = Arc::clone(&parser);
        let bytes_processed = Arc::new(AtomicU64::new(0));
        let bytes_clone = Arc::clone(&bytes_processed);

        let shell = if enable_shell_integration {
            Some(Arc::new(Mutex::new(ShellSession::default())))
        } else {
            None
        };
        let shell_for_reader = shell.clone();

        // Spawn a blocking task to read PTY output and feed the vt100 parser.
        // Batches up to 64KB before locking the parser to reduce lock contention
        // with the render thread during heavy output.
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            let mut batch = Vec::with_capacity(65536);
            // Streaming OSC parser keeps state across PTY chunks. Lives only
            // inside this task; the main thread reads results via `shell`.
            let mut osc_parser = OscParser::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        tracing::debug!("PTY reader EOF");
                        break;
                    }
                    Ok(n) => {
                        let chunk = &buf[..n];
                        if let Some(ref shell) = shell_for_reader {
                            let events = osc_parser.feed(chunk);
                            if !events.is_empty() {
                                let mut s = shell.lock();
                                for ev in &events {
                                    s.state.apply(ev);
                                }
                                s.pending_events.extend(events);
                            }
                        }
                        batch.extend_from_slice(chunk);
                        bytes_clone.fetch_add(n as u64, Ordering::Relaxed);
                        // Flush when batch is full or PTY buffer is likely drained
                        if batch.len() >= 65536 || n < buf.len() {
                            let answerback = {
                                let mut p = parser_clone.lock();
                                p.process(&batch);
                                p.screen_mut().take_answerback()
                            };
                            batch.clear();
                            // Reply to terminal queries (DSR, DA) outside the
                            // parser lock. Apps like vim or agent CLIs probe
                            // the terminal at startup and may hang or exit if
                            // nothing answers. Queued, not written inline:
                            // this thread must keep draining the master or
                            // the child stalls on its own output.
                            if !answerback.is_empty() {
                                let _ = writer_for_reader.send(answerback);
                            }
                            // Raise after releasing the parser lock so a woken
                            // renderer never contends with this thread.
                            if let Some(ref sig) = output_signal {
                                sig.raise();
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            // Flush remaining bytes
            if !batch.is_empty() {
                {
                    let mut p = parser_clone.lock();
                    p.process(&batch);
                }
                if let Some(ref sig) = output_signal {
                    sig.raise();
                }
            }
        });

        // Out-of-band FIFO transport for the structured cli-agent channel.
        // Only meaningful when shell integration is on (so `shell` is `Some`)
        // and a per-spawn FIFO path was supplied (Claude tabs).
        #[cfg(unix)]
        let cli_agent_sock = match (cli_agent_sock, shell.as_ref()) {
            (Some(path), Some(shell)) => {
                match crate::cli_agent::sock::spawn_reader(path, Arc::clone(shell), None) {
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

        tracing::info!(command = command, path = %worktree_path.display(), rows, cols, shell_integration = enable_shell_integration, "PTY spawned");

        Ok(Self {
            child,
            writer_tx,
            parser,
            reader_handle,
            master: pair.master,
            bytes_processed,
            shell,
            #[cfg(unix)]
            _cli_agent_sock: cli_agent_sock,
        })
    }

    /// Per-session shell integration state, if this session was spawned with it
    /// enabled. Lock to read `state` (cwd, last_command) or drain
    /// `pending_events` for forwarding.
    pub fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        self.shell.as_ref()
    }

    /// Send input bytes to the PTY (user keystrokes, pastes, mouse reports).
    ///
    /// Never blocks: bytes are queued for the session's writer thread. A
    /// direct `write(2)` on the master would block once the kernel's PTY
    /// input queue (~4 KiB) fills — i.e. whenever the child stops reading
    /// stdin for a moment — and the UI thread has no business waiting on a
    /// child's scheduling. Worse, the reader thread also writes here
    /// (terminal-query answerbacks); if it blocked it would stop draining
    /// the child's output, the child would stall on its write, never read
    /// its input, and the whole app would deadlock with the terminal still
    /// in raw/alt-screen mode.
    ///
    /// Errors only if the writer thread has already exited, which happens
    /// after the first failed write (the child closed its side).
    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.writer_tx
            .send(data.to_vec())
            .map_err(|_| anyhow::anyhow!("PTY writer thread has exited"))
    }

    /// Get a reference to the vt100 parser for rendering
    pub fn parser(&self) -> &Arc<Mutex<vt100::Parser>> {
        &self.parser
    }

    /// Total bytes read from PTY (for auto-scroll detection)
    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed.load(Ordering::Relaxed)
    }

    /// Resize the PTY when the terminal panel changes size
    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut p = self.parser.lock();
        p.screen_mut().set_size(rows, cols);
        Ok(())
    }

    /// Kill the child process
    pub fn kill(&mut self) -> anyhow::Result<()> {
        tracing::info!("PTY killed");
        self.child.kill()?;
        Ok(())
    }

    /// Check if the child process is still running
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Non-mutating liveness check: returns true if the reader task is still running.
    /// Suitable for use during rendering where only `&self` is available.
    pub fn peek_alive(&self) -> bool {
        !self.reader_handle.is_finished()
    }

    /// Abort the reader task (call on cleanup)
    pub fn abort_reader(&self) {
        self.reader_handle.abort();
    }
}

impl Drop for LocalPty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.reader_handle.abort();
    }
}

/// Scrollback the client-side restore parser keeps (mirrors the daemon's).
const REMOTE_SCROLLBACK: usize = 5000;

/// A PTY session that lives in the [session daemon](crate::session::daemon):
/// this end holds a `vt100` parser fed by the daemon's restore + output
/// frames, mirrors the shell/agent state, and sends input/resize/detach over
/// the socket. Backs [`PtySession::Remote`].
///
/// Killing it kills the child in the daemon (the session is retained as
/// exited); dropping it *detaches* — the whole point is that the session
/// survives this process.
pub struct RemotePty {
    parser: Arc<Mutex<vt100::Parser>>,
    shell: Option<Arc<Mutex<ShellSession>>>,
    bytes: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    size: Arc<Mutex<(u16, u16)>>,
    sender: AttachmentSender,
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

impl RemotePty {
    /// Build a remote-backed session from a fresh [`Attachment`]. Spawns a
    /// reader thread that keeps the local parser + shell state in sync and
    /// raises `output_signal` (if given) so an event loop wakes on output.
    ///
    /// `integration_on` mirrors the flag the session was spawned with: when
    /// set, a client-side [`ShellSession`] is kept so the Agents pane can read
    /// live status; the daemon only sends shell/agent frames for such
    /// sessions.
    pub fn start(
        att: Attachment,
        integration_on: bool,
        output_signal: Option<PtyOutputSignal>,
    ) -> Self {
        let (rows, cols) = (att.rows, att.cols);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            rows,
            cols,
            REMOTE_SCROLLBACK,
        )));
        let shell = integration_on.then(|| {
            Arc::new(Mutex::new(ShellSession {
                state: ShellTabState::from_snapshot(&att.shell),
                pending_events: Vec::new(),
            }))
        });
        let bytes = Arc::new(AtomicU64::new(0));
        let alive = Arc::new(AtomicBool::new(att.info.state.is_live()));
        let size = Arc::new(Mutex::new((rows, cols)));
        let sender = att.sender();

        let (read, _sender) = att.into_read_half();
        let reader_handle = {
            let parser = Arc::clone(&parser);
            let shell = shell.clone();
            let bytes = Arc::clone(&bytes);
            let alive = Arc::clone(&alive);
            let size = Arc::clone(&size);
            std::thread::Builder::new()
                .name("remote-pty-reader".into())
                .spawn(move || {
                    remote_reader_loop(
                        read,
                        &parser,
                        shell.as_ref(),
                        &bytes,
                        &alive,
                        &size,
                        output_signal.as_ref(),
                    );
                })
                .ok()
        };

        RemotePty {
            parser,
            shell,
            bytes,
            alive,
            size,
            sender,
            reader_handle,
        }
    }

    fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.sender.input(data)?;
        Ok(())
    }

    fn parser(&self) -> &Arc<Mutex<vt100::Parser>> {
        &self.parser
    }

    fn bytes_processed(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        *self.size.lock() = (rows, cols);
        self.sender.resize(rows, cols)?;
        self.parser.lock().screen_mut().set_size(rows, cols);
        Ok(())
    }

    /// Kill the child in the daemon. The session stays (as exited) until the
    /// frontend removes it, so a killed tab can still show its final screen.
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

    fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        self.shell.as_ref()
    }

    /// Detach from the daemon (leaving the session running) and let the reader
    /// thread wind down.
    fn abort_reader(&self) {
        let _ = self.sender.detach();
    }
}

impl Drop for RemotePty {
    fn drop(&mut self) {
        // Detach, never kill: the session must survive us.
        let _ = self.sender.detach();
        if let Some(h) = self.reader_handle.take() {
            // The reader exits on the resulting socket close; don't block.
            drop(h);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn remote_reader_loop(
    mut read: std::os::unix::net::UnixStream,
    parser: &Arc<Mutex<vt100::Parser>>,
    shell: Option<&Arc<Mutex<ShellSession>>>,
    bytes: &Arc<AtomicU64>,
    alive: &Arc<AtomicBool>,
    size: &Arc<Mutex<(u16, u16)>>,
    output_signal: Option<&PtyOutputSignal>,
) {
    loop {
        match read_frame(&mut read) {
            Ok(Frame::Restore(b)) => {
                // A restore rebuilds the whole terminal, so start from a fresh
                // parser of the current size before replaying it.
                let (rows, cols) = *size.lock();
                {
                    let mut p = parser.lock();
                    *p = vt100::Parser::new(rows, cols, REMOTE_SCROLLBACK);
                    p.process(&b);
                }
                bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
                if let Some(sig) = output_signal {
                    sig.raise();
                }
            }
            Ok(Frame::Output(b)) => {
                parser.lock().process(&b);
                bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
                if let Some(sig) = output_signal {
                    sig.raise();
                }
            }
            Ok(Frame::ShellState(snap)) => {
                if let Some(shell) = shell {
                    shell.lock().state = ShellTabState::from_snapshot(&snap);
                    if let Some(sig) = output_signal {
                        sig.raise();
                    }
                }
            }
            Ok(Frame::ShellEvent { event, .. }) => {
                if let Some(shell) = shell {
                    // Apply to state AND push to pending_events so the existing
                    // frontend drain (notifications, badges) treats a remote
                    // tab exactly like a local one.
                    let mut s = shell.lock();
                    s.state.apply(&event);
                    s.pending_events.push(event);
                }
                if let Some(sig) = output_signal {
                    sig.raise();
                }
            }
            Ok(Frame::Exited { .. }) => {
                alive.store(false, Ordering::Relaxed);
                if let Some(sig) = output_signal {
                    sig.raise();
                }
            }
            Ok(Frame::Detached { .. }) => {
                alive.store(false, Ordering::Relaxed);
                if let Some(sig) = output_signal {
                    sig.raise();
                }
                break;
            }
            Ok(_) => {} // heartbeat and unexpected frames
            Err(_) => {
                // Daemon gone or connection dropped → the tab is dead.
                alive.store(false, Ordering::Relaxed);
                if let Some(sig) = output_signal {
                    sig.raise();
                }
                break;
            }
        }
    }
}

/// A PTY session, either owned in this process ([`Local`](Self::Local)) or
/// living in the [session daemon](crate::session::daemon)
/// ([`Remote`](Self::Remote)). The public surface is identical so callers
/// (the TUI's `Tab`, the desktop's tab) don't care which backing they got —
/// the frontend picks `Remote` when the daemon is reachable and falls back to
/// `Local` otherwise.
pub enum PtySession {
    Local(LocalPty),
    Remote(RemotePty),
}

impl PtySession {
    /// Spawn a session owned by this process (the fallback path, and what the
    /// tests use). Signature unchanged from the original struct.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        worktree_path: &Path,
        rows: u16,
        cols: u16,
        command: &str,
        args: &[String],
        extra_env: &[(String, String)],
        extra_args: &[String],
        enable_shell_integration: bool,
        cli_agent_sock: Option<std::path::PathBuf>,
        output_signal: Option<PtyOutputSignal>,
    ) -> anyhow::Result<Self> {
        let local = LocalPty::spawn(
            worktree_path,
            rows,
            cols,
            command,
            args,
            extra_env,
            extra_args,
            enable_shell_integration,
            cli_agent_sock,
            output_signal,
        )
        .await?;
        Ok(PtySession::Local(local))
    }

    /// Wrap a daemon [`Attachment`] as a remote-backed session.
    pub fn from_attachment(
        att: Attachment,
        integration_on: bool,
        output_signal: Option<PtyOutputSignal>,
    ) -> Self {
        PtySession::Remote(RemotePty::start(att, integration_on, output_signal))
    }

    /// True for a daemon-backed session (dropping it detaches rather than
    /// kills). Frontends use this to decide quit/close semantics.
    pub fn is_remote(&self) -> bool {
        matches!(self, PtySession::Remote(_))
    }

    pub fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        match self {
            PtySession::Local(l) => l.shell(),
            PtySession::Remote(r) => r.shell(),
        }
    }

    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        match self {
            PtySession::Local(l) => l.write(data),
            PtySession::Remote(r) => r.write(data),
        }
    }

    pub fn parser(&self) -> &Arc<Mutex<vt100::Parser>> {
        match self {
            PtySession::Local(l) => l.parser(),
            PtySession::Remote(r) => r.parser(),
        }
    }

    pub fn bytes_processed(&self) -> u64 {
        match self {
            PtySession::Local(l) => l.bytes_processed(),
            PtySession::Remote(r) => r.bytes_processed(),
        }
    }

    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        match self {
            PtySession::Local(l) => l.resize(rows, cols),
            PtySession::Remote(r) => r.resize(rows, cols),
        }
    }

    pub fn kill(&mut self) -> anyhow::Result<()> {
        match self {
            PtySession::Local(l) => l.kill(),
            PtySession::Remote(r) => r.kill(),
        }
    }

    pub fn is_alive(&mut self) -> bool {
        match self {
            PtySession::Local(l) => l.is_alive(),
            PtySession::Remote(r) => r.is_alive(),
        }
    }

    pub fn peek_alive(&self) -> bool {
        match self {
            PtySession::Local(l) => l.peek_alive(),
            PtySession::Remote(r) => r.peek_alive(),
        }
    }

    pub fn abort_reader(&self) {
        match self {
            PtySession::Local(l) => l.abort_reader(),
            PtySession::Remote(r) => r.abort_reader(),
        }
    }
}
