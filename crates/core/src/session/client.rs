//! Client side of the session daemon: a [`Daemon`] handle for short-lived
//! control requests, an [`Attachment`] for the long-lived duplex stream, and
//! [`ensure_daemon`] which starts one on demand.
//!
//! Everything here is synchronous `std::os::unix::net`; callers in an async
//! context wrap the blocking calls in `spawn_blocking`. The frontends drive an
//! [`Attachment`] from a dedicated reader thread (see the `Remote` backend in
//! [`crate::pty`]).

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::session::protocol::{
    Frame, Hello, PROTOCOL_VERSION, Reply, Request, SessionInfo, SetMetaRequest,
    ShellStateSnapshot, SpawnRequest, is_disconnect, read_frame, write_frame,
};

/// Why a control request or attach failed.
#[derive(Debug)]
pub enum ClientError {
    /// Could not reach the daemon (no socket, connection refused).
    NotRunning,
    /// The daemon speaks a different protocol version.
    Incompatible { daemon_protocol: u32 },
    /// The daemon returned an `Error` reply.
    Daemon(String),
    /// The reply frame was not the one expected for the request.
    Unexpected(&'static str),
    /// Transport error.
    Io(io::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotRunning => write!(f, "session daemon is not running"),
            ClientError::Incompatible { daemon_protocol } => write!(
                f,
                "session daemon protocol {daemon_protocol} != client {PROTOCOL_VERSION}"
            ),
            ClientError::Daemon(m) => write!(f, "daemon error: {m}"),
            ClientError::Unexpected(what) => write!(f, "unexpected reply to {what}"),
            ClientError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::NotFound || e.kind() == io::ErrorKind::ConnectionRefused {
            ClientError::NotRunning
        } else {
            ClientError::Io(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// Read the daemon's `Hello` and verify the protocol version.
fn handshake(stream: &mut UnixStream) -> Result<Hello> {
    match read_frame(stream)? {
        Frame::Hello(h) => {
            if h.protocol == PROTOCOL_VERSION {
                Ok(h)
            } else {
                Err(ClientError::Incompatible {
                    daemon_protocol: h.protocol,
                })
            }
        }
        _ => Err(ClientError::Unexpected("hello")),
    }
}

/// Open a fresh connection, complete the handshake, and send `req`.
fn dial(socket: &Path, req: Request) -> Result<UnixStream> {
    let mut stream = crate::session::uds::connect_stream(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    handshake(&mut stream)?;
    write_frame(&mut stream, &Frame::Request(req))?;
    stream.flush()?;
    Ok(stream)
}

/// A handle to a running daemon. Cheap to clone/keep; each method opens its
/// own short-lived connection (the daemon serves one request per control
/// connection).
#[derive(Clone)]
pub struct Daemon {
    socket: PathBuf,
}

impl Daemon {
    pub fn new(socket: PathBuf) -> Self {
        Daemon { socket }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    fn request(&self, req: Request, what: &'static str) -> Result<Reply> {
        let mut stream = dial(&self.socket, req)?;
        match read_frame(&mut stream)? {
            Frame::Reply(r) => Ok(r),
            _ => Err(ClientError::Unexpected(what)),
        }
    }

    /// Daemon liveness + protocol check.
    pub fn ping(&self) -> Result<()> {
        match self.request(Request::Ping, "ping")? {
            Reply::Pong => Ok(()),
            Reply::Error { message } => Err(ClientError::Daemon(message)),
            _ => Err(ClientError::Unexpected("ping")),
        }
    }

    /// Every session the daemon knows about (live and exited).
    pub fn list(&self) -> Result<Vec<SessionInfo>> {
        match self.request(Request::List, "list")? {
            Reply::Sessions { sessions } => Ok(sessions),
            Reply::Error { message } => Err(ClientError::Daemon(message)),
            _ => Err(ClientError::Unexpected("list")),
        }
    }

    /// Create a session without attaching.
    pub fn spawn(&self, mut req: SpawnRequest) -> Result<SessionInfo> {
        req.attach = false;
        match self.request(Request::Spawn(req), "spawn")? {
            Reply::Spawned { info } => Ok(info),
            Reply::Error { message } => Err(ClientError::Daemon(message)),
            _ => Err(ClientError::Unexpected("spawn")),
        }
    }

    /// Create a session and attach to it in one round-trip.
    pub fn spawn_attach(&self, mut req: SpawnRequest) -> Result<Attachment> {
        req.attach = true;
        let rows = req.rows;
        let cols = req.cols;
        let mut stream = dial(&self.socket, Request::Spawn(req))?;
        let info = match read_frame(&mut stream)? {
            Frame::Reply(Reply::Spawned { info }) => info,
            Frame::Reply(Reply::Error { message }) => return Err(ClientError::Daemon(message)),
            _ => return Err(ClientError::Unexpected("spawn_attach")),
        };
        Attachment::start(stream, info, ShellStateSnapshot::default(), rows, cols)
    }

    /// Attach to an existing session at the given size.
    pub fn attach(&self, id: &str, rows: u16, cols: u16) -> Result<Attachment> {
        let mut stream = dial(
            &self.socket,
            Request::Attach {
                id: id.to_string(),
                rows,
                cols,
            },
        )?;
        let (info, shell) = match read_frame(&mut stream)? {
            Frame::Reply(Reply::Attached { info, shell }) => (info, shell),
            Frame::Reply(Reply::Error { message }) => return Err(ClientError::Daemon(message)),
            _ => return Err(ClientError::Unexpected("attach")),
        };
        Attachment::start(stream, info, shell, rows, cols)
    }

    pub fn kill(&self, id: &str) -> Result<()> {
        self.expect_ok(Request::Kill { id: id.to_string() }, "kill")
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        self.expect_ok(Request::Remove { id: id.to_string() }, "remove")
    }

    pub fn set_meta(&self, req: SetMetaRequest) -> Result<()> {
        self.expect_ok(Request::SetMeta(req), "set_meta")
    }

    /// Ask the daemon to stop. Refused (as a `Daemon` error) when live
    /// sessions exist unless `force`.
    pub fn shutdown(&self, force: bool) -> Result<()> {
        self.expect_ok(Request::Shutdown { force }, "shutdown")
    }

    fn expect_ok(&self, req: Request, what: &'static str) -> Result<()> {
        match self.request(req, what)? {
            Reply::Ok => Ok(()),
            Reply::Error { message } => Err(ClientError::Daemon(message)),
            _ => Err(ClientError::Unexpected(what)),
        }
    }
}

/// A live attachment to one session: the read half (for the caller's reader
/// thread) plus a shared write half for sending input/resize/resync/detach.
pub struct Attachment {
    /// Session metadata as of attach time.
    pub info: SessionInfo,
    /// Shell/agent state as of attach time (empty for a fresh spawn).
    pub shell: ShellStateSnapshot,
    /// Size requested at attach — the size the restore buffer is drawn for.
    pub rows: u16,
    pub cols: u16,
    read: UnixStream,
    write: Arc<Mutex<UnixStream>>,
}

impl Attachment {
    fn start(
        stream: UnixStream,
        info: SessionInfo,
        shell: ShellStateSnapshot,
        rows: u16,
        cols: u16,
    ) -> Result<Attachment> {
        // A long-lived attach must not time out on quiet periods.
        stream.set_read_timeout(None)?;
        let write = stream.try_clone()?;
        Ok(Attachment {
            info,
            shell,
            rows,
            cols,
            read: stream,
            write: Arc::new(Mutex::new(write)),
        })
    }

    /// A cloneable sender for input/resize/resync/detach/kill frames.
    pub fn sender(&self) -> AttachmentSender {
        AttachmentSender {
            write: Arc::clone(&self.write),
        }
    }

    /// Take the read half so the caller can drive a reader loop
    /// ([`read_frame`] on it until it errors). The write half stays available
    /// via [`sender`](Self::sender).
    pub fn into_read_half(self) -> (UnixStream, AttachmentSender) {
        (self.read, AttachmentSender { write: self.write })
    }
}

/// The write side of an [`Attachment`]; cloneable and `Send`.
#[derive(Clone)]
pub struct AttachmentSender {
    write: Arc<Mutex<UnixStream>>,
}

impl AttachmentSender {
    fn send(&self, frame: &Frame) -> io::Result<()> {
        let mut w = self.write.lock();
        write_frame(&mut *w, frame)?;
        w.flush()
    }

    pub fn input(&self, data: &[u8]) -> io::Result<()> {
        self.send(&Frame::Input(data.to_vec()))
    }

    pub fn resize(&self, rows: u16, cols: u16) -> io::Result<()> {
        self.send(&Frame::Resize { rows, cols })
    }

    pub fn resync(&self) -> io::Result<()> {
        self.send(&Frame::Resync)
    }

    pub fn detach(&self) -> io::Result<()> {
        self.send(&Frame::Detach)
    }

    pub fn kill(&self) -> io::Result<()> {
        self.send(&Frame::Kill)
    }
}

/// `true` for the read error a reader loop gets when the attachment ends
/// cleanly (daemon detached us, or the socket closed).
pub fn is_attachment_end(err: &io::Error) -> bool {
    is_disconnect(err)
}

/// Ensure a daemon is reachable at `socket`, starting one with `launch` if
/// not. `launch` should spawn the daemon process (`current_exe … serve
/// --data-dir …`) detached; it is only called when no daemon answers.
///
/// Returns a [`Daemon`] once the socket answers a ping within ~5s, or
/// [`ClientError::Incompatible`] if a daemon is up but speaks another
/// protocol (the caller decides whether to fall back to an in-process PTY or
/// ask the user to restart the daemon).
pub fn ensure_daemon(socket: &Path, launch: impl FnOnce() -> io::Result<()>) -> Result<Daemon> {
    let daemon = Daemon::new(socket.to_path_buf());
    match daemon.ping() {
        Ok(()) => return Ok(daemon),
        Err(ClientError::Incompatible { daemon_protocol }) => {
            return Err(ClientError::Incompatible { daemon_protocol });
        }
        Err(ClientError::NotRunning) => {}
        // A malformed/half-open daemon: treat as not running and (re)launch.
        Err(_) => {}
    }

    launch().map_err(ClientError::Io)?;

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut backoff = Duration::from_millis(20);
    loop {
        match daemon.ping() {
            Ok(()) => return Ok(daemon),
            Err(ClientError::Incompatible { daemon_protocol }) => {
                return Err(ClientError::Incompatible { daemon_protocol });
            }
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(400));
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// Start an in-process daemon on a temp socket and return its `Daemon`
    /// handle plus a guard that stops it.
    fn in_process_daemon() -> (Daemon, DaemonGuard) {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = crate::session::daemon::Server::new();
        let serve = server.clone();
        let handle = std::thread::spawn(move || {
            let _ = serve.serve(listener);
        });
        for _ in 0..200 {
            if UnixStream::connect(&socket).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        (
            Daemon::new(socket),
            DaemonGuard {
                server,
                handle: Some(handle),
                _dir: dir,
            },
        )
    }

    struct DaemonGuard {
        server: Arc<crate::session::daemon::Server>,
        handle: Option<std::thread::JoinHandle<()>>,
        _dir: tempfile::TempDir,
    }
    impl Drop for DaemonGuard {
        fn drop(&mut self) {
            self.server.kill_all();
            self.server.request_shutdown();
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    fn cat_req(id: &str) -> SpawnRequest {
        SpawnRequest {
            id: id.into(),
            command: "cat".into(),
            args: vec![],
            cwd: std::env::temp_dir(),
            env: vec![(
                "PATH".into(),
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
            )],
            rows: 24,
            cols: 80,
            integration_on: false,
            cli_agent_sock: None,
            meta: Default::default(),
            attach: false,
        }
    }

    #[test]
    fn ping_list_spawn_kill_remove() {
        let (daemon, _g) = in_process_daemon();
        daemon.ping().unwrap();
        assert!(daemon.list().unwrap().is_empty());

        let info = daemon.spawn(cat_req("c1")).unwrap();
        assert_eq!(info.id, "c1");
        assert!(info.state.is_live());

        let listed = daemon.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "c1");

        daemon.kill("c1").unwrap();
        // Killed session is retained as exited.
        assert!(daemon.list().unwrap().iter().any(|s| s.id == "c1"));

        daemon.remove("c1").unwrap();
        assert!(daemon.list().unwrap().is_empty());

        // Errors surface as ClientError::Daemon, not panics.
        assert!(matches!(daemon.kill("nope"), Err(ClientError::Daemon(_))));
    }

    #[test]
    fn spawn_attach_round_trips_output_and_restore() {
        let (daemon, _g) = in_process_daemon();
        let att = daemon.spawn_attach(cat_req("a1")).unwrap();
        assert_eq!(att.info.id, "a1");
        let sender = att.sender();
        let (mut read, _s2) = att.into_read_half();

        sender.input(b"echo-me\n").unwrap();
        // Read frames until we see the echo in an Output frame.
        read.set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let mut got = false;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            match read_frame(&mut read) {
                Ok(Frame::Output(b)) if b.windows(7).any(|w| w == b"echo-me") => {
                    got = true;
                    break;
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
        assert!(got, "echo should reach the attachment");

        // Detach, then re-attach and confirm restore carries the echo.
        sender.detach().unwrap();
        drop(read);
        std::thread::sleep(Duration::from_millis(100));

        let att2 = daemon.attach("a1", 24, 80).unwrap();
        let (mut read2, _s) = att2.into_read_half();
        read2
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buf = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            match read_frame(&mut read2) {
                Ok(Frame::Restore(b)) => {
                    buf.extend_from_slice(&b);
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(
            String::from_utf8_lossy(&buf).contains("echo-me"),
            "restore should carry earlier output"
        );
    }

    #[test]
    fn ensure_daemon_returns_existing_without_launching() {
        let (daemon, _g) = in_process_daemon();
        let launched = std::cell::Cell::new(false);
        let got = ensure_daemon(daemon.socket(), || {
            launched.set(true);
            Ok(())
        })
        .unwrap();
        assert_eq!(got.socket(), daemon.socket());
        assert!(
            !launched.get(),
            "must not launch when a daemon already answers"
        );
    }

    #[test]
    fn ensure_daemon_launches_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("late.sock");
        let socket_for_launch = socket.clone();
        // The "launch" starts an in-process daemon a beat later, mimicking a
        // forked daemon coming up.
        type Started = Arc<
            Mutex<
                Option<(
                    Arc<crate::session::daemon::Server>,
                    std::thread::JoinHandle<()>,
                )>,
            >,
        >;
        let started: Started = Arc::new(Mutex::new(None));
        let started_for_launch = Arc::clone(&started);
        let daemon = ensure_daemon(&socket, move || {
            let listener = UnixListener::bind(&socket_for_launch).unwrap();
            let server = crate::session::daemon::Server::new();
            let serve = server.clone();
            let handle = std::thread::spawn(move || {
                let _ = serve.serve(listener);
            });
            *started_for_launch.lock() = Some((server, handle));
            Ok(())
        })
        .expect("daemon should come up");
        daemon.ping().unwrap();

        // Cleanup.
        if let Some((server, handle)) = started.lock().take() {
            server.request_shutdown();
            let _ = handle.join();
        }
    }
}
