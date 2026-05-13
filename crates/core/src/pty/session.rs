use parking_lot::Mutex;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

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

/// Manages an AI assistant process running in a pseudo-terminal
pub struct PtySession {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    bytes_processed: Arc<AtomicU64>,
    /// Present iff this session was spawned with shell integration enabled.
    /// Reader thread parses OSC sequences and mutates this; UI threads read.
    shell: Option<Arc<Mutex<ShellSession>>>,
}

impl PtySession {
    /// Spawn an AI assistant in a PTY inside the given worktree directory.
    ///
    /// `extra_env` is merged into the child environment after the inherited
    /// vars (so callers can override defaults). `extra_args` is prepended to
    /// `args` — useful for `bash --rcfile <bridge>` where the rcfile flag
    /// must come before any user-supplied args. Pass `enable_shell_integration =
    /// true` to spin up an OSC parser that observes the byte stream and
    /// updates the per-tab [`ShellTabState`].
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
        let writer = pair.master.take_writer()?;

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
                            let mut p = parser_clone.lock();
                            p.process(&batch);
                            batch.clear();
                        }
                    }
                    Err(_) => break,
                }
            }
            // Flush remaining bytes
            if !batch.is_empty() {
                let mut p = parser_clone.lock();
                p.process(&batch);
            }
        });

        tracing::info!(command = command, path = %worktree_path.display(), rows, cols, shell_integration = enable_shell_integration, "PTY spawned");

        Ok(Self {
            child,
            writer,
            parser,
            reader_handle,
            master: pair.master,
            bytes_processed,
            shell,
        })
    }

    /// Per-session shell integration state, if this session was spawned with it
    /// enabled. Lock to read `state` (cwd, last_command) or drain
    /// `pending_events` for forwarding.
    pub fn shell(&self) -> Option<&Arc<Mutex<ShellSession>>> {
        self.shell.as_ref()
    }

    /// Send input bytes to the PTY (user keystrokes)
    pub fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
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

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.reader_handle.abort();
    }
}
