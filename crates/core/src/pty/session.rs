use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// Manages an AI assistant process running in a pseudo-terminal
pub struct PtySession {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    bytes_processed: Arc<AtomicU64>,
}

impl PtySession {
    /// Spawn an AI assistant in a PTY inside the given worktree directory
    pub async fn spawn(
        worktree_path: &Path,
        rows: u16,
        cols: u16,
        command: &str,
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
        cmd.cwd(worktree_path);

        let child = pair.slave.spawn_command(cmd)?;
        // Drop the slave side — the child process owns it now
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let parser_clone = Arc::clone(&parser);
        let bytes_processed = Arc::new(AtomicU64::new(0));
        let bytes_clone = Arc::clone(&bytes_processed);

        // Spawn a blocking task to read PTY output and feed the vt100 parser.
        // Batches up to 64KB before locking the parser to reduce lock contention
        // with the render thread during heavy output.
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            let mut batch = Vec::with_capacity(65536);
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF — process exited
                    Ok(n) => {
                        batch.extend_from_slice(&buf[..n]);
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

        Ok(Self {
            child,
            writer,
            parser,
            reader_handle,
            master: pair.master,
            bytes_processed,
        })
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
        self.child.kill()?;
        Ok(())
    }

    /// Check if the child process is still running
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
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
