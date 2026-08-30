//! Integration tests for the persistent-session daemon, driven in-process:
//! a real `Server::serve` on a temp Unix socket (no fork) with a tiny test
//! client that speaks the wire protocol. This exercises the whole engine —
//! spawn, attach, fan-out, restore, resize, kill, list, remove, two
//! concurrent clients — without any of the daemonize machinery.

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use piki_core::session::daemon::Server;
use piki_core::session::protocol::{
    Frame, Hello, Reply, Request, SessionMeta, SessionState, SpawnRequest, read_frame, write_frame,
};

/// A connected test client: `Hello` already read, positioned to send its one
/// request.
struct Client {
    stream: UnixStream,
}

impl Client {
    fn connect(socket: &PathBuf) -> Client {
        let mut stream = UnixStream::connect(socket).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let hello = read_frame(&mut stream).expect("hello");
        assert!(matches!(hello, Frame::Hello(Hello { .. })), "got {hello:?}");
        Client { stream }
    }

    fn request(&mut self, req: Request) {
        write_frame(&mut self.stream, &Frame::Request(req)).unwrap();
        self.stream.flush().unwrap();
    }

    fn read(&mut self) -> Frame {
        read_frame(&mut self.stream).expect("frame")
    }

    fn reply(&mut self) -> Reply {
        match self.read() {
            Frame::Reply(r) => r,
            other => panic!("expected reply, got {other:?}"),
        }
    }

    fn send(&mut self, frame: Frame) {
        write_frame(&mut self.stream, &frame).unwrap();
        self.stream.flush().unwrap();
    }

    /// Read frames until one satisfies `pred` or a deadline passes, returning
    /// all frames seen. Used to wait for specific output/exit frames while
    /// tolerating interleaved heartbeats.
    fn drain_until(&mut self, secs: u64, mut pred: impl FnMut(&Frame) -> bool) -> Vec<Frame> {
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            self.stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            match read_frame(&mut self.stream) {
                Ok(f) => {
                    let stop = pred(&f);
                    seen.push(f);
                    if stop {
                        return seen;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => break,
            }
        }
        seen
    }
}

/// Concatenate the text of every `Output`/`Restore` frame in `frames`.
fn text_of(frames: &[Frame]) -> String {
    let mut out = Vec::new();
    for f in frames {
        match f {
            Frame::Output(b) | Frame::Restore(b) => out.extend_from_slice(b),
            _ => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct Harness {
    socket: PathBuf,
    server: std::sync::Arc<Server>,
    _dir: tempfile::TempDir,
    handle: Option<thread::JoinHandle<()>>,
}

impl Harness {
    fn start() -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = Server::new();
        let serve = std::sync::Arc::clone(&server);
        let handle = thread::spawn(move || {
            let _ = serve.serve(listener);
        });
        // Wait for the accept loop to be up.
        for _ in 0..100 {
            if UnixStream::connect(&socket).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        Harness {
            socket,
            server,
            _dir: dir,
            handle: Some(handle),
        }
    }

    fn client(&self) -> Client {
        Client::connect(&self.socket)
    }

    fn spawn_req(&self, id: &str, command: &str, args: &[&str], attach: bool) -> SpawnRequest {
        SpawnRequest {
            id: id.to_string(),
            command: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: std::env::temp_dir(),
            env: vec![
                ("TERM".into(), "xterm-256color".into()),
                (
                    "PATH".into(),
                    std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
                ),
            ],
            rows: 24,
            cols: 80,
            integration_on: false,
            cli_agent_sock: None,
            meta: SessionMeta {
                provider: "Shell".into(),
                ..Default::default()
            },
            attach,
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.server.kill_all();
        self.server.request_shutdown();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[test]
fn spawn_attach_write_detach_reattach_restores_output() {
    let h = Harness::start();

    // Spawn `cat` with attach in one connection.
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("s1", "cat", &[], true)));
    match a.reply() {
        Reply::Spawned { info } => assert_eq!(info.id, "s1"),
        other => panic!("{other:?}"),
    }
    // First frame after Spawned is the restore (empty screen).
    let restore = a.read();
    assert!(matches!(restore, Frame::Restore(_)), "got {restore:?}");

    // Type a line; cat echoes it back as Output.
    a.send(Frame::Input(b"hello daemon\n".to_vec()));
    let seen = a.drain_until(
        3,
        |f| matches!(f, Frame::Output(b) if b.windows(5).any(|w| w == b"hello")),
    );
    assert!(
        text_of(&seen).contains("hello daemon"),
        "echo not seen: {seen:?}"
    );

    // Detach; the session (and cat) keep running.
    a.send(Frame::Detach);
    drop(a);
    thread::sleep(Duration::from_millis(100));

    // Re-attach on a fresh connection: the restore buffer must carry the
    // earlier echo, proving the daemon kept the terminal state.
    let mut b = h.client();
    b.request(Request::Attach {
        id: "s1".into(),
        rows: 24,
        cols: 80,
    });
    match b.reply() {
        Reply::Attached { info, .. } => {
            assert_eq!(info.id, "s1");
            assert!(info.state.is_live());
            assert_eq!(info.attached, 1);
        }
        other => panic!("{other:?}"),
    }
    let restore = b.drain_until(15, |f| matches!(f, Frame::Restore(_)));
    assert!(
        text_of(&restore).contains("hello daemon"),
        "restore missing prior output: {:?}",
        text_of(&restore)
    );
}

#[test]
fn list_reflects_sessions_and_attachment_count() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("live", "cat", &[], true)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));
    let _ = a.read(); // restore

    let mut lister = h.client();
    lister.request(Request::List);
    match lister.reply() {
        Reply::Sessions { sessions } => {
            let s = sessions.iter().find(|s| s.id == "live").expect("listed");
            assert!(s.state.is_live());
            assert_eq!(s.attached, 1, "one client attached");
            assert_eq!(s.meta.provider, "Shell");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn kill_marks_session_exited_and_notifies_clients() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("k", "cat", &[], true)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));
    let _ = a.read(); // restore

    // Kill over a separate control connection.
    let mut ctl = h.client();
    ctl.request(Request::Kill { id: "k".into() });
    assert!(matches!(ctl.reply(), Reply::Ok));

    // The attached client is told the child exited.
    let seen = a.drain_until(15, |f| matches!(f, Frame::Exited { .. }));
    assert!(
        seen.iter().any(|f| matches!(f, Frame::Exited { .. })),
        "no Exited frame: {seen:?}"
    );

    // And List now reports it exited (retained, not dropped).
    let mut lister = h.client();
    lister.request(Request::List);
    match lister.reply() {
        Reply::Sessions { sessions } => {
            let s = sessions.iter().find(|s| s.id == "k").expect("still listed");
            assert!(matches!(s.state, SessionState::Exited { .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn remove_drops_the_session() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("r", "cat", &[], false)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));

    let mut rm = h.client();
    rm.request(Request::Remove { id: "r".into() });
    assert!(matches!(rm.reply(), Reply::Ok));

    let mut lister = h.client();
    lister.request(Request::List);
    match lister.reply() {
        Reply::Sessions { sessions } => {
            assert!(!sessions.iter().any(|s| s.id == "r"), "should be gone");
        }
        other => panic!("{other:?}"),
    }

    // Removing a gone session is an error, not a panic.
    let mut c = h.client();
    c.request(Request::Remove { id: "r".into() });
    assert!(matches!(c.reply(), Reply::Error { .. }));
}

#[test]
fn two_clients_both_receive_output() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("multi", "cat", &[], true)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));
    let _ = a.read(); // restore

    let mut b = h.client();
    b.request(Request::Attach {
        id: "multi".into(),
        rows: 24,
        cols: 80,
    });
    assert!(matches!(b.reply(), Reply::Attached { .. }));
    let _ = b.drain_until(15, |f| matches!(f, Frame::Restore(_)));

    // Input from a is echoed by cat and must reach BOTH attached clients.
    a.send(Frame::Input(b"broadcast\n".to_vec()));
    let sa = a.drain_until(
        3,
        |f| matches!(f, Frame::Output(x) if x.windows(9).any(|w| w == b"broadcast")),
    );
    let sb = b.drain_until(
        3,
        |f| matches!(f, Frame::Output(x) if x.windows(9).any(|w| w == b"broadcast")),
    );
    assert!(text_of(&sa).contains("broadcast"), "client a missed it");
    assert!(text_of(&sb).contains("broadcast"), "client b missed it");
}

#[test]
fn shutdown_is_refused_while_a_live_session_exists_without_force() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("busy", "cat", &[], false)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));

    let mut ctl = h.client();
    ctl.request(Request::Shutdown { force: false });
    assert!(
        matches!(ctl.reply(), Reply::Error { .. }),
        "live session must block an un-forced shutdown"
    );

    // Ping still works — the daemon is up.
    let mut p = h.client();
    p.request(Request::Ping);
    assert!(matches!(p.reply(), Reply::Pong));
}

#[test]
fn set_meta_updates_are_visible_in_list() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("m", "cat", &[], false)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));

    let mut sm = h.client();
    sm.request(Request::SetMeta(
        piki_core::session::protocol::SetMetaRequest {
            id: "m".into(),
            set_title: true,
            title: Some("renamed".into()),
            order: Some(9),
            workspace_path: Some(PathBuf::from("/ws/x")),
            closable: Some(false),
        },
    ));
    assert!(matches!(sm.reply(), Reply::Ok));

    let mut lister = h.client();
    lister.request(Request::List);
    match lister.reply() {
        Reply::Sessions { sessions } => {
            let s = sessions.iter().find(|s| s.id == "m").unwrap();
            assert_eq!(s.meta.title.as_deref(), Some("renamed"));
            assert_eq!(s.meta.order, 9);
            assert_eq!(s.meta.workspace_path, PathBuf::from("/ws/x"));
            assert!(!s.meta.closable);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn attaching_to_a_missing_session_errors() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Attach {
        id: "ghost".into(),
        rows: 24,
        cols: 80,
    });
    assert!(matches!(a.reply(), Reply::Error { .. }));
}

#[test]
fn resync_resends_the_restore_buffer() {
    let h = Harness::start();
    let mut a = h.client();
    a.request(Request::Spawn(h.spawn_req("rs", "cat", &[], true)));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));
    let _ = a.read(); // initial restore

    a.send(Frame::Input(b"resync me\n".to_vec()));
    let _ = a.drain_until(
        3,
        |f| matches!(f, Frame::Output(b) if b.windows(6).any(|w| w == b"resync")),
    );

    a.send(Frame::Resync);
    let seen = a.drain_until(15, |f| matches!(f, Frame::Restore(_)));
    assert!(
        text_of(&seen).contains("resync me"),
        "resync restore missing earlier output: {:?}",
        text_of(&seen)
    );
}

/// Exercise the real launcher `run(paths, foreground=true)` — the lock, bind,
/// stale-socket cleanup, signal install, serve and cleanup — end to end,
/// stopping it with a forced `Shutdown` (the one path that doesn't fork).
#[test]
fn foreground_run_serves_spawns_and_shuts_down() {
    use piki_core::paths::DataPaths;

    let dir = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(dir.path().to_path_buf());
    let socket = paths.session_socket();

    let run_paths = paths.daemon_paths();
    let handle = thread::spawn(move || {
        let _ = piki_core::session::daemon::run(&run_paths, true, None);
    });

    // Wait for the daemon to bind.
    for _ in 0..200 {
        if UnixStream::connect(&socket).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(socket.exists(), "daemon should have bound its socket");
    assert!(paths.session_pid_file().exists(), "pid file written");

    // A real spawn + attach + restore round-trip against the forked-style
    // daemon (minus the fork).
    let mut a = Client::connect(&socket);
    let mut req = SpawnRequest {
        id: "fg".into(),
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
        meta: SessionMeta::default(),
        attach: true,
    };
    req.meta.provider = "Shell".into();
    a.request(Request::Spawn(req));
    assert!(matches!(a.reply(), Reply::Spawned { .. }));
    let _ = a.read(); // restore
    a.send(Frame::Input(b"launcher\n".to_vec()));
    let seen = a.drain_until(
        3,
        |f| matches!(f, Frame::Output(b) if b.windows(8).any(|w| w == b"launcher")),
    );
    assert!(text_of(&seen).contains("launcher"));

    // Force shutdown and confirm the daemon stops and cleans up.
    let mut ctl = Client::connect(&socket);
    ctl.request(Request::Shutdown { force: true });
    assert!(matches!(ctl.reply(), Reply::Ok));

    let joined = {
        let start = Instant::now();
        loop {
            if handle.is_finished() {
                break true;
            }
            if start.elapsed() > Duration::from_secs(5) {
                break false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    };
    assert!(joined, "run() should return after a forced shutdown");
    let _ = handle.join();
    assert!(!socket.exists(), "socket cleaned up on shutdown");
    assert!(
        !paths.session_pid_file().exists(),
        "pid file cleaned up on shutdown"
    );
}
