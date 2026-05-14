use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Serialize;
use tauri::AppHandle;
use tauri::{Emitter, Manager};

use piki_core::notifications;
use piki_core::pty::ShellSession;
use piki_core::shell_integration::ShellEvent;
use piki_core::shell_integration::parser::OscParser;

use crate::events::PtyAttentionPayload;
use crate::state::DesktopApp;

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

pub struct RawPtySession {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    reader_handle: tokio::task::JoinHandle<()>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    bytes_processed: Arc<AtomicU64>,
    shell: Option<Arc<Mutex<ShellSession>>>,
}

impl RawPtySession {
    /// Spawn a PTY child. `extra_env` is merged into the inherited login env
    /// (so callers can override defaults — e.g. `PIKI_SHELL_INTEGRATION=1`).
    /// `extra_args` is **prepended** to the command's normal args (needed for
    /// `bash --rcfile <bridge>` where the rcfile flag must come first). With
    /// `enable_shell_integration = true`, the reader spins up an [`OscParser`]
    /// that observes the byte stream, updates the session's [`ShellSession`]
    /// state, and emits `pty-shell-event` Tauri events.
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

        let emit_tab_id = tab_id.clone();
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            let mut osc_parser = OscParser::new();
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
                                    if let ShellEvent::CommandEnd { exit_code } = &ev {
                                        handle_shell_command_end(
                                            &app_handle,
                                            &emit_tab_id,
                                            *exit_code,
                                        );
                                    }
                                    let _ = app_handle.emit(
                                        "pty-shell-event",
                                        shell_event_payload(&emit_tab_id, ev),
                                    );
                                }
                            }
                        }
                        let encoded = BASE64.encode(chunk);
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

        tracing::info!(command, path = %worktree_path.display(), rows, cols, shell_integration = enable_shell_integration, "Raw PTY spawned");

        Ok(Self {
            child,
            writer,
            reader_handle,
            master: pair.master,
            bytes_processed,
            shell,
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

impl Drop for RawPtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.reader_handle.abort();
    }
}

/// Handle an OSC 133 `command-end` marker on a shell tab: emit a
/// `pty-attention` event for the sidebar badge and fire an OS notification
/// (always, regardless of which workspace is active). Workspace lookup walks
/// `DesktopApp.workspaces` by `tab_id`; if the tab can't be found (e.g. it
/// was closed between read and dispatch) only the attention event is skipped.
fn handle_shell_command_end(app_handle: &AppHandle, tab_id: &str, exit_code: Option<i32>) {
    let Some(state) = app_handle.try_state::<Mutex<DesktopApp>>() else {
        return;
    };
    let (workspace_idx, workspace_name) = {
        let app = state.lock();
        let Some((idx, ws)) = app
            .workspaces
            .iter()
            .enumerate()
            .find(|(_, ws)| ws.tabs.iter().any(|t| t.id == tab_id))
        else {
            return;
        };
        (idx, ws.info.name.clone())
    };
    let _ = app_handle.emit(
        "pty-attention",
        PtyAttentionPayload {
            workspace_idx,
            tab_id: tab_id.to_string(),
            source: "shell-command-end",
        },
    );
    notifications::notify_command_end(&workspace_name, exit_code);
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
        ShellEvent::CommandEnd { exit_code } => {
            p.kind = "command-end";
            p.exit_code = exit_code;
        }
        ShellEvent::CwdChanged(path) => {
            p.kind = "cwd-changed";
            p.cwd = Some(path.display().to_string());
        }
    }
    p
}
