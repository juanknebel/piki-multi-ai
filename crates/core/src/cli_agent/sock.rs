//! Out-of-band per-tab FIFO transport for the structured cli-agent channel.
//!
//! Claude Code spawns its hook subprocesses `setsid`-detached with **no
//! controlling terminal**, so the hook's in-band `printf … > /dev/tty` OSC 777
//! write (see [`super::install`] / `build-payload.sh`) never reaches piki's
//! PTY stream. Env vars on the `claude` child *do* propagate to its hooks, so
//! we advertise a per-spawn FIFO path via `PIKI_CLI_AGENT_SOCK`; the hook
//! writes newline-delimited JSON there and this reader feeds it into the same
//! [`ShellSession`] the OSC parser feeds — a purely additive second producer.
//!
//! The OSC 777 path stays as a compat/graceful-degradation fallback (the hook
//! uses it only when the FIFO env var is absent); nothing on the consumer side
//! changes.
//!
//! Unix only (gated at the `pub mod sock;` declaration). The crate already
//! depends on `libc` (see `shell_env.rs`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;

use crate::pty::ShellSession;
use crate::shell_integration::ShellEvent;

/// Optional per-event callback (e.g. the desktop frontend bridge that emits a
/// Tauri event). Runs *after* the [`ShellSession`] has been updated.
pub type CliAgentCallback = Box<dyn Fn(&crate::cli_agent::CliAgentEvent) + Send>;

/// RAII guard for a running FIFO reader. Dropping it stops the reader thread,
/// aborts the blocking task, and unlinks the FIFO.
pub struct SockReader {
    handle: tokio::task::JoinHandle<()>,
    stop: Arc<AtomicBool>,
    path: PathBuf,
}

impl Drop for SockReader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.abort();
        let _ = unlink(&self.path);
    }
}

/// Spawn the FIFO reader on a blocking task (mirrors the PTY reader pattern in
/// [`crate::pty::PtySession`]).
///
/// `mkfifo`s `path` mode `0o600` (unlinking + recreating if it already exists),
/// opens it `O_RDWR | O_NONBLOCK | O_CLOEXEC`, and loops reading
/// newline-delimited JSON payloads. For each complete line that
/// [`crate::cli_agent::parse_cli_agent_payload`] accepts, it locks `shell` and
/// applies the event to **both** `state` (so
/// [`crate::shell_integration::ShellTabState::apply`] sets `cli_agent: Some`,
/// which the idle-watcher guard checks) and `pending_events` (what frontends
/// drain) — exactly what the OSC reader does in `pty/session.rs`. Then the
/// optional `callback` runs with the event.
///
/// O_RDWR on a FIFO never blocks on open and never EOFs even when every writer
/// (each short-lived hook process) closes, so the reader stays valid for the
/// whole session regardless of writer churn.
pub fn spawn_reader(
    path: PathBuf,
    shell: Arc<Mutex<ShellSession>>,
    callback: Option<CliAgentCallback>,
) -> std::io::Result<SockReader> {
    use std::os::unix::ffi::OsStrExt;

    // (Re)create the FIFO. Unlink first so a stale node from a crashed prior
    // run (or a regular file at that path) can't wedge us.
    let _ = unlink(&path);
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    // SAFETY: `c_path` is a valid NUL-terminated C string for the duration of
    // the call; `mkfifo` only reads it.
    let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_task = Arc::clone(&stop);
    let path_for_task = path.clone();

    let handle = tokio::task::spawn_blocking(move || {
        run(path_for_task, shell, callback, stop_for_task);
    });

    Ok(SockReader { handle, stop, path })
}

fn run(
    path: PathBuf,
    shell: Arc<Mutex<ShellSession>>,
    callback: Option<CliAgentCallback>,
    stop: Arc<AtomicBool>,
) {
    use std::io::Read;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;

    let c_path = match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "cli-agent sock: non-UTF8/NUL path");
            return;
        }
    };
    // O_RDWR keeps the FIFO readable across writer churn (a plain O_RDONLY
    // would EOF every time the last hook process exits). O_NONBLOCK so reads
    // return WouldBlock instead of parking the blocking task forever, letting
    // us observe `stop`. O_CLOEXEC so spawned children never inherit this fd.
    // SAFETY: `c_path` outlives the call; `open` only reads it.
    let fd = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        tracing::warn!(
            error = %std::io::Error::last_os_error(),
            path = %path.display(),
            "cli-agent sock: open failed"
        );
        let _ = unlink(&path);
        return;
    }
    // SAFETY: `fd` is a fresh, owned, valid file descriptor; `File` takes sole
    // ownership and will close it on drop.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };

    let mut buf = [0u8; 8192];
    let mut acc: Vec<u8> = Vec::with_capacity(8192);

    loop {
        match file.read(&mut buf) {
            Ok(0) => {
                // O_RDWR on a FIFO never reports real EOF; a 0-length read here
                // means "no writer right now". Back off and keep watching.
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(n) => {
                acc.extend_from_slice(&buf[..n]);
                drain_lines(&mut acc, &shell, callback.as_deref());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) if e.raw_os_error() == Some(libc::EINTR) => {
                // Interrupted syscall — just retry.
                continue;
            }
            Err(e) => {
                tracing::debug!(error = %e, "cli-agent sock: read error, stopping reader");
                break;
            }
        }
    }

    let _ = unlink(&path);
}

/// Split the accumulator on `\n` and dispatch each complete line. Any trailing
/// partial line stays in `acc` for the next read.
fn drain_lines(
    acc: &mut Vec<u8>,
    shell: &Arc<Mutex<ShellSession>>,
    callback: Option<&(dyn Fn(&crate::cli_agent::CliAgentEvent) + Send)>,
) {
    while let Some(nl) = acc.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = acc.drain(..=nl).collect();
        let line = &line[..line.len() - 1]; // drop the '\n'
        let Ok(text) = std::str::from_utf8(line) else {
            tracing::debug!("cli-agent sock: non-UTF8 line, skipping");
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Some(ev) = crate::cli_agent::parse_cli_agent_payload(text) else {
            continue;
        };
        {
            let mut s = shell.lock();
            s.state.apply(&ShellEvent::CliAgent(ev.clone()));
            s.pending_events.push(ShellEvent::CliAgent(ev.clone()));
        }
        if let Some(cb) = callback {
            cb(&ev);
        }
    }
}

fn unlink(path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    // SAFETY: `c_path` outlives the call; `unlink` only reads it.
    let rc = unsafe { libc::unlink(c_path.as_ptr()) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::NotFound {
            return Ok(());
        }
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn round_trip_two_payloads_reach_pending_and_state() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("rt.sock");

        let shell = Arc::new(Mutex::new(ShellSession::default()));
        let reader = spawn_reader(sock.clone(), Arc::clone(&shell), None)
            .expect("reader starts");

        // Reader mkfifo's on a blocking task; wait for the FIFO to appear.
        for _ in 0..200 {
            if sock.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(sock.exists(), "FIFO should have been created by the reader");

        // Write two newline-delimited JSON payloads (a `stop` and a
        // `permission_request`) into the FIFO, the way a hook script would.
        let stop = r#"{"v":1,"agent":"claude","event":"stop","session_id":"s1","response":"done"}"#;
        let perm = r#"{"v":1,"agent":"claude","event":"permission_request","session_id":"s1","tool_name":"Bash","summary":"Wants to run Bash: ls"}"#;
        {
            let mut w = std::fs::OpenOptions::new()
                .write(true)
                .open(&sock)
                .expect("open FIFO for writing");
            writeln!(w, "{stop}").unwrap();
            writeln!(w, "{perm}").unwrap();
            w.flush().unwrap();
        }

        // Poll until both events have been consumed into pending_events.
        let mut got = Vec::new();
        for _ in 0..200 {
            {
                let mut s = shell.lock();
                if !s.pending_events.is_empty() {
                    got.extend(s.drain_events());
                }
            }
            if got.len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(got.len(), 2, "both payloads should arrive: got {got:?}");
        assert!(matches!(
            got[0],
            ShellEvent::CliAgent(crate::cli_agent::CliAgentEvent::Stop { .. })
        ));
        assert!(matches!(
            got[1],
            ShellEvent::CliAgent(crate::cli_agent::CliAgentEvent::PermissionRequest { .. })
        ));

        // State must also have been updated (load-bearing: the idle-watcher
        // guard checks `state.cli_agent.is_some()`).
        {
            let s = shell.lock();
            assert!(s.state.cli_agent.is_some(), "ShellTabState.cli_agent set");
        }

        drop(reader);
        // Drop unlinks the FIFO.
        for _ in 0..100 {
            if !sock.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!sock.exists(), "FIFO should be unlinked on guard drop");
    }
}
