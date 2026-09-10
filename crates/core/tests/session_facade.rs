//! `PtySession::Remote` (the client-side facade) driven against an in-process
//! session daemon: it must behave like a local PTY — a live vt100 parser, a
//! byte counter, working input/resize, liveness that flips on exit — while
//! surviving a detach so a re-attach restores the same screen.

use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use piki_core::pty::{PtyOutputSignal, PtySession};
use piki_core::session::client::Daemon;
use piki_core::session::daemon::Server;
use piki_core::session::protocol::{SessionMeta, SpawnRequest};

struct Guard {
    server: Arc<Server>,
    handle: Option<thread::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.server.kill_all();
        self.server.request_shutdown();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn daemon() -> (Daemon, Guard) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("d.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = Server::new();
    let serve = server.clone();
    let handle = thread::spawn(move || {
        let _ = serve.serve(listener);
    });
    for _ in 0..200 {
        if UnixStream::connect(&socket).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    (
        Daemon::new(socket),
        Guard {
            server,
            handle: Some(handle),
            _dir: dir,
        },
    )
}

fn cat(id: &str) -> SpawnRequest {
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
        meta: SessionMeta {
            provider: "Shell".into(),
            ..Default::default()
        },
        attach: false,
    }
}

/// Poll `f` until it returns true or the deadline passes.
fn wait_until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    f()
}

fn screen_text(pty: &PtySession) -> String {
    pty.parser().lock().screen().contents()
}

#[test]
fn remote_pty_echoes_into_its_local_parser() {
    let (daemon, _g) = daemon();
    let att = daemon.spawn_attach(cat("f1")).unwrap();
    let signal = PtyOutputSignal::new();
    let mut pty = PtySession::from_attachment(att, false, None, Some(signal));
    assert!(pty.is_remote());
    assert!(pty.peek_alive());

    pty.write(b"hello facade\n").unwrap();
    assert!(
        wait_until(15, || screen_text(&pty).contains("hello facade")),
        "parser never showed the echo: {:?}",
        screen_text(&pty)
    );
    assert!(pty.bytes_processed() > 0);
}

#[test]
fn dropping_a_remote_pty_detaches_and_the_session_survives() {
    let (daemon, _g) = daemon();
    let att = daemon.spawn_attach(cat("f2")).unwrap();
    let mut pty = PtySession::from_attachment(att, false, None, Some(PtyOutputSignal::new()));
    pty.write(b"persist me\n").unwrap();
    assert!(wait_until(15, || screen_text(&pty).contains("persist me")));

    // Drop detaches; the daemon must still have the (live) session.
    drop(pty);
    assert!(
        wait_until(15, || {
            daemon
                .list()
                .unwrap()
                .iter()
                .any(|s| s.id == "f2" && s.state.is_live() && s.attached == 0)
        }),
        "session should survive the drop, detached"
    );

    // Re-attach: the fresh parser is rebuilt from the restore buffer.
    let att2 = daemon.attach("f2", 24, 80).unwrap();
    let pty2 = PtySession::from_attachment(att2, false, None, Some(PtyOutputSignal::new()));
    assert!(
        wait_until(15, || screen_text(&pty2).contains("persist me")),
        "restore didn't rebuild the screen: {:?}",
        screen_text(&pty2)
    );
}

#[test]
fn killing_a_remote_pty_flips_liveness() {
    let (daemon, _g) = daemon();
    let att = daemon.spawn_attach(cat("f3")).unwrap();
    let mut pty = PtySession::from_attachment(att, false, None, Some(PtyOutputSignal::new()));
    assert!(pty.peek_alive());

    pty.kill().unwrap();
    assert!(
        wait_until(15, || !pty.peek_alive()),
        "kill should end the session's life"
    );
    assert!(!pty.is_alive());
    // The daemon retains it as exited.
    assert!(
        daemon
            .list()
            .unwrap()
            .iter()
            .any(|s| s.id == "f3" && !s.state.is_live())
    );
}

#[test]
fn resize_is_reflected_in_the_local_parser() {
    let (daemon, _g) = daemon();
    let att = daemon.spawn_attach(cat("f4")).unwrap();
    let pty = PtySession::from_attachment(att, false, None, Some(PtyOutputSignal::new()));
    pty.resize(40, 100).unwrap();
    assert_eq!(pty.parser().lock().screen().size(), (40, 100));
}
