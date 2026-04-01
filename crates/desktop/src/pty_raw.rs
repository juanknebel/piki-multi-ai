use std::io::{Read, Write};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;
use tauri::AppHandle;
use tauri::Emitter;

#[derive(Serialize, Clone)]
struct PtyOutputPayload {
    tab_id: String,
    data: String,
}

#[derive(Serialize, Clone)]
struct PtyExitPayload {
    tab_id: String,
    exit_code: Option<i32>,
}

pub struct RawPtySession {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
}

impl RawPtySession {
    pub fn spawn(
        app_handle: AppHandle,
        tab_id: String,
        worktree_path: &Path,
        rows: u16,
        cols: u16,
        command: &str,
        args: &[String],
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
        cmd.args(args);
        cmd.cwd(worktree_path);

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let emit_tab_id = tab_id.clone();
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = app_handle.emit(
                            "pty-exit",
                            PtyExitPayload {
                                tab_id: emit_tab_id.clone(),
                                exit_code: Some(0),
                            },
                        );
                        break;
                    }
                    Ok(n) => {
                        let encoded = BASE64.encode(&buf[..n]);
                        let _ = app_handle.emit(
                            "pty-output",
                            PtyOutputPayload {
                                tab_id: emit_tab_id.clone(),
                                data: encoded,
                            },
                        );
                    }
                    Err(_) => {
                        let _ = app_handle.emit(
                            "pty-exit",
                            PtyExitPayload {
                                tab_id: emit_tab_id.clone(),
                                exit_code: None,
                            },
                        );
                        break;
                    }
                }
            }
        });

        tracing::info!(command, path = %worktree_path.display(), rows, cols, "Raw PTY spawned");

        Ok(Self {
            child,
            writer,
            reader_handle,
            master: pair.master,
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

    #[allow(dead_code)]
    pub fn peek_alive(&self) -> bool {
        !self.reader_handle.is_finished()
    }
}

impl Drop for RawPtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.reader_handle.abort();
    }
}
