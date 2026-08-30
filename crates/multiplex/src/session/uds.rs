//! AF_UNIX bind/connect that tolerate a socket path longer than `sun_path`.
//!
//! The ~108-byte (Linux) / ~104-byte (macOS) `sun_path` cap applies ONLY to the
//! address string handed to `bind()`/`connect()` — never to filesystem
//! operations (create/remove/chmod/stat) on the same file. So we keep the
//! socket in its natural home (`<data-dir>/sessions/daemon.sock`) unconditionally
//! and, only when that absolute path would overflow the syscall, address it
//! through a short proxy:
//!
//! - **bind** (daemon): `chdir` into the parent and bind the bare filename.
//!   Safe because the daemon binds *before spawning any thread*, so mutating the
//!   process-global cwd races with nothing. The cwd is restored afterward.
//! - **connect** (client): on Linux, dial `/proc/self/fd/<dirfd>/<name>` — a
//!   short, stable path that needs no cwd change, which matters because the
//!   client runs inside the fully multi-threaded frontend. Other unix targets
//!   (no `/proc`) fall back to a mutex-guarded `chdir`.

use std::ffi::OsStr;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

/// Longest socket path we hand straight to `bind()`/`connect()`. Kept under the
/// real 108 (Linux) / 104 (macOS) `sun_path` cap with margin for the trailing
/// NUL and a little slack.
const SUN_PATH_BUDGET: usize = 100;

fn fits(path: &Path) -> bool {
    path.as_os_str().len() <= SUN_PATH_BUDGET
}

fn split(path: &Path) -> io::Result<(&Path, &OsStr)> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "socket path has no parent"))?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "socket path has no file name")
    })?;
    Ok((dir, name))
}

/// Bind a listener at `path`, keeping the socket file at `path` even when the
/// absolute path overflows `sun_path`.
///
/// MUST be called before the process spawns any thread: the overflow path
/// mutates the process-global cwd.
pub(crate) fn bind_listener(path: &Path) -> io::Result<UnixListener> {
    if fits(path) {
        return UnixListener::bind(path);
    }
    let (dir, name) = split(path)?;
    let prev = std::env::current_dir().ok();
    std::env::set_current_dir(dir)?;
    let result = UnixListener::bind(name);
    if let Some(prev) = prev {
        let _ = std::env::set_current_dir(prev);
    }
    result
}

/// Connect a stream to the socket at `path`, tolerating an overflowing path
/// without mutating the process cwd on Linux (safe for the multi-threaded
/// client).
pub(crate) fn connect_stream(path: &Path) -> io::Result<UnixStream> {
    if fits(path) {
        return UnixStream::connect(path);
    }
    connect_overflow(path)
}

#[cfg(target_os = "linux")]
fn connect_overflow(path: &Path) -> io::Result<UnixStream> {
    use std::os::fd::AsRawFd;
    let (dir, name) = split(path)?;
    let name = name
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non-utf8 socket name"))?;
    // Hold the directory fd open across connect(): the /proc/self/fd/<n> entry
    // is only valid while <n> refers to the open dir. connect() resolves the
    // whole path synchronously, so the fd can drop right after.
    let dirfd = std::fs::File::open(dir)?;
    let proxy = format!("/proc/self/fd/{}/{name}", dirfd.as_raw_fd());
    UnixStream::connect(proxy)
}

#[cfg(not(target_os = "linux"))]
fn connect_overflow(path: &Path) -> io::Result<UnixStream> {
    use std::sync::Mutex;
    // No /proc: serialize a scoped chdir so concurrent connects don't clobber
    // each other's cwd. Rare (only for pathologically deep data dirs).
    static CWD_LOCK: Mutex<()> = Mutex::new(());
    let (dir, name) = split(path)?;
    let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::current_dir().ok();
    std::env::set_current_dir(dir)?;
    let result = UnixStream::connect(name);
    if let Some(prev) = prev {
        let _ = std::env::set_current_dir(prev);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::thread;

    /// A socket path far longer than `sun_path` still binds and round-trips,
    /// with the socket file physically living at the requested (deep) path.
    #[test]
    fn overflowing_path_binds_and_connects_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        // Build a directory nesting whose socket path blows past the budget.
        let mut deep = tmp.path().to_path_buf();
        while deep.join("sessions/daemon.sock").as_os_str().len() <= SUN_PATH_BUDGET {
            deep = deep.join("nested-directory-segment");
        }
        let dir = deep.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("daemon.sock");
        assert!(
            !fits(&sock),
            "test needs an overflowing path, got {} bytes",
            sock.as_os_str().len()
        );

        let listener = bind_listener(&sock).expect("bind overflowing path");
        assert!(
            sock.exists(),
            "socket file lives at the deep path, not moved"
        );

        let server = thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4];
            conn.read_exact(&mut buf).unwrap();
            conn.write_all(&buf).unwrap();
        });

        let mut client = connect_stream(&sock).expect("connect overflowing path");
        client.write_all(b"ping").unwrap();
        let mut echo = [0u8; 4];
        client.read_exact(&mut echo).unwrap();
        assert_eq!(&echo, b"ping");
        server.join().unwrap();
    }

    /// The common case (short path) takes the plain bind/connect route.
    #[test]
    fn short_path_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("d.sock");
        assert!(fits(&sock));
        let listener = bind_listener(&sock).unwrap();
        let server = thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2];
            conn.read_exact(&mut buf).unwrap();
            conn.write_all(&buf).unwrap();
        });
        let mut client = connect_stream(&sock).unwrap();
        client.write_all(b"hi").unwrap();
        let mut echo = [0u8; 2];
        client.read_exact(&mut echo).unwrap();
        assert_eq!(&echo, b"hi");
        server.join().unwrap();
    }
}
