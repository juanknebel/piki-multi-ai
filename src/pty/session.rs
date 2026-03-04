use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// Manages an AI assistant process running in a pseudo-terminal
pub struct PtySession {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
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

        // Spawn a blocking task to read PTY output and feed the vt100 parser
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF — process exited
                    Ok(n) => {
                        let mut p = parser_clone.lock().unwrap();
                        p.process(&buf[..n]);
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            child,
            writer,
            parser,
            reader_handle,
            master: pair.master,
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

    /// Resize the PTY when the terminal panel changes size
    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut p = self.parser.lock().unwrap();
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
