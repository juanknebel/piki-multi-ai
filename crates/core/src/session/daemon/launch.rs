//! Daemon process lifecycle: double-fork into the background, hold a single
//! exclusive lock so only one daemon per data dir exists, clean up a stale
//! socket, install signal-based socket cleanup, and serve.
//!
//! Split from the engine so [`super::Server`] stays testable in-process
//! without any of this.

use std::fs;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;

use crate::paths::DataPaths;

use super::Server;

/// Run the session daemon for `paths`. Unless `foreground`, it detaches from
/// the controlling terminal first. Returns once the daemon stops (idle-exit,
/// a `Shutdown` request, or a termination signal).
pub fn run(paths: &DataPaths, foreground: bool) -> anyhow::Result<()> {
    fs::create_dir_all(paths.sessions_dir()).context("creating sessions dir")?;
    fs::create_dir_all(paths.log_dir()).ok();

    if !foreground {
        // SAFETY: called before any threads are spawned in this process.
        unsafe { daemonize() }.context("daemonizing")?;
    }

    // Only now (post-fork) set up logging: to a file when daemonized (stdio is
    // /dev/null), to stderr when foreground. `try_init` so an embedding caller
    // that already has a subscriber (tests) isn't clobbered.
    init_logging(paths, foreground);

    // Single-instance lock. Held for the life of the process; a second daemon
    // for the same data dir fails to acquire it and exits quietly.
    let _lock = match acquire_lock(&paths.session_lock()) {
        Ok(lock) => lock,
        Err(LockError::Busy) => {
            tracing::info!("another session daemon is already running");
            return Ok(());
        }
        Err(LockError::Io(e)) => return Err(e).context("locking daemon"),
    };
    write_pid(&paths.session_pid_file());

    let socket = paths.session_socket();
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent).ok();
    }
    remove_stale_socket(&socket);
    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("binding session socket at {}", socket.display()))?;
    restrict_permissions(&socket);

    let server = Server::new();
    install_signal_cleanup(&server);

    tracing::info!(socket = %socket.display(), "session daemon listening");
    let result = server.serve(listener);

    server.kill_all();
    let _ = fs::remove_file(&socket);
    let _ = fs::remove_file(paths.session_pid_file());
    result.context("serving")
}

/// Install the daemon's own tracing subscriber. Uses a plain file writer (no
/// background thread — this runs right after a fork) when daemonized; stderr
/// when foreground. `PIKI_SESSION_LOG` overrides the default `info` level.
fn init_logging(paths: &DataPaths, foreground: bool) {
    use tracing::level_filters::LevelFilter;
    let level = match std::env::var("PIKI_SESSION_LOG")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "trace" => LevelFilter::TRACE,
        "debug" => LevelFilter::DEBUG,
        "warn" => LevelFilter::WARN,
        "error" => LevelFilter::ERROR,
        "off" => LevelFilter::OFF,
        _ => LevelFilter::INFO,
    };
    if foreground {
        let _ = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(level)
            .try_init();
        return;
    }
    if let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.session_log_path())
    {
        let _ = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .with_max_level(level)
            .try_init();
    }
}

/// A held exclusive lock; releasing (drop) unlocks the file.
struct LockGuard {
    _file: fs::File,
}

enum LockError {
    Busy,
    Io(std::io::Error),
}

fn acquire_lock(path: &Path) -> Result<LockGuard, LockError> {
    use std::os::fd::AsRawFd;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(LockError::Io)?;
    // SAFETY: flock on a valid fd; LOCK_NB returns EWOULDBLOCK rather than
    // blocking when another daemon holds it.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        Ok(LockGuard { _file: file })
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Err(LockError::Busy)
        } else {
            Err(LockError::Io(err))
        }
    }
}

fn write_pid(path: &Path) {
    if let Ok(mut f) = fs::File::create(path) {
        let _ = writeln!(f, "{}", std::process::id());
    }
}

/// Remove the socket file if nothing is listening on it (a leftover from a
/// crashed daemon); leave it alone if a live daemon answers.
fn remove_stale_socket(socket: &Path) {
    if !socket.exists() {
        return;
    }
    match UnixStream::connect(socket) {
        Ok(_) => {} // a live daemon owns it; our bind will fail and that's right
        Err(_) => {
            let _ = fs::remove_file(socket);
        }
    }
}

fn restrict_permissions(socket: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(socket, fs::Permissions::from_mode(0o600));
    if let Some(dir) = socket.parent() {
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
}

/// On SIGTERM/SIGINT, ask the server to stop; the main thread then runs its
/// normal socket/pid cleanup. Uses a self-pipe-free approach: the handler
/// only flips the atomic flag the accept loop already polls.
fn install_signal_cleanup(server: &Arc<Server>) {
    use std::sync::OnceLock;
    static SERVER: OnceLock<Arc<Server>> = OnceLock::new();
    let _ = SERVER.set(Arc::clone(server));

    extern "C" fn on_term(_sig: libc::c_int) {
        if let Some(s) = SERVER.get() {
            s.request_shutdown();
        }
    }
    // SAFETY: on_term only touches an atomic through an Arc stored in a
    // OnceLock, which is async-signal-safe here (no allocation, no locks).
    unsafe {
        libc::signal(libc::SIGTERM, on_term as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_term as *const () as libc::sighandler_t);
    }
}

/// Classic double-fork + setsid, redirecting std fds to `/dev/null`.
///
/// SAFETY: the caller must not have spawned any threads yet.
unsafe fn daemonize() -> anyhow::Result<()> {
    unsafe {
        // First fork: parent exits so we are not a process-group leader.
        match libc::fork() {
            -1 => return Err(std::io::Error::last_os_error()).context("first fork"),
            0 => {}
            _ => std::process::exit(0),
        }
        if libc::setsid() == -1 {
            return Err(std::io::Error::last_os_error()).context("setsid");
        }
        // Second fork: ensure we can never reacquire a controlling terminal.
        match libc::fork() {
            -1 => return Err(std::io::Error::last_os_error()).context("second fork"),
            0 => {}
            _ => std::process::exit(0),
        }
        let devnull = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
        if devnull >= 0 {
            libc::dup2(devnull, libc::STDIN_FILENO);
            libc::dup2(devnull, libc::STDOUT_FILENO);
            libc::dup2(devnull, libc::STDERR_FILENO);
            if devnull > libc::STDERR_FILENO {
                libc::close(devnull);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_on_the_same_path_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("daemon.lock");
        let held = match acquire_lock(&lock) {
            Ok(g) => g,
            Err(_) => panic!("first lock should succeed"),
        };
        // A second daemon for the same data dir must observe the lock as busy
        // rather than blocking or double-binding.
        assert!(
            matches!(acquire_lock(&lock), Err(LockError::Busy)),
            "second lock must be Busy while the first is held"
        );
        drop(held);
        // Once released, the lock is acquirable again.
        assert!(acquire_lock(&lock).is_ok(), "lock reusable after release");
    }

    #[test]
    fn stale_socket_with_no_listener_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        // A plain file standing in for a leftover socket node: nothing is
        // listening, so it must be cleared before we bind.
        std::fs::write(&socket, b"stale").unwrap();
        remove_stale_socket(&socket);
        assert!(!socket.exists(), "stale socket file should be removed");
    }

    #[test]
    fn live_socket_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("live.sock");
        let _listener = UnixListener::bind(&socket).unwrap();
        remove_stale_socket(&socket);
        assert!(
            socket.exists(),
            "a socket with a live listener must be kept"
        );
    }
}
