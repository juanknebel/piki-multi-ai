//! The daemon's connection server: one thread per client connection, a table
//! of live and exited sessions, and a reaper that drops stale exited sessions
//! and stops an idle daemon.
//!
//! An attach connection is split in two after the reply: the accepting thread
//! keeps reading client frames (input, resize, resync, detach, kill) while a
//! writer thread drains that client's outbound queue to the socket, emitting a
//! heartbeat when idle so a dead client is noticed promptly.

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::session::protocol::{
    Frame, Hello, PROTOCOL_VERSION, Reply, Request, read_frame, write_frame,
};

use super::session::Session;

/// How often a detached client is sent a heartbeat so a hung peer is noticed.
const HEARTBEAT: Duration = Duration::from_secs(5);
/// How long an exited session is kept so a restarting frontend can still show
/// its final screen and exit code.
const EXITED_TTL: Duration = Duration::from_secs(3600);
/// How long the daemon stays up with no sessions and no clients before it
/// stops on its own.
const IDLE_EXIT: Duration = Duration::from_secs(60);
/// Reaper cadence.
const REAP_INTERVAL: Duration = Duration::from_secs(30);

pub struct Server {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    shutdown: Arc<AtomicBool>,
    idle_since: Mutex<Option<Instant>>,
    pid: u32,
}

impl Server {
    pub fn new() -> Arc<Server> {
        Arc::new(Server {
            sessions: Mutex::new(HashMap::new()),
            shutdown: Arc::new(AtomicBool::new(false)),
            idle_since: Mutex::new(Some(Instant::now())),
            pid: std::process::id(),
        })
    }

    /// Flag the accept loop and reaper to stop.
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Kill every session (used on daemon exit).
    pub fn kill_all(&self) {
        for s in self.sessions.lock().unwrap().values() {
            s.kill();
        }
    }

    /// Serve until [`request_shutdown`](Self::request_shutdown) is called.
    /// The reaper runs on its own thread for the duration.
    pub fn serve(self: &Arc<Self>, listener: UnixListener) -> std::io::Result<()> {
        listener.set_nonblocking(true)?;
        let reaper = {
            let server = Arc::clone(self);
            thread::Builder::new()
                .name("session-reaper".into())
                .spawn(move || server.reap_loop())?
        };

        while !self.is_shutting_down() {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let server = Arc::clone(self);
                    let _ = thread::Builder::new()
                        .name("session-conn".into())
                        .spawn(move || {
                            if let Err(e) = server.handle_conn(stream) {
                                tracing::debug!(error = %e, "session connection ended");
                            }
                        });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "accept failed");
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }

        let _ = reaper.join();
        Ok(())
    }

    fn peer_ok(stream: &UnixStream) -> bool {
        // Only the daemon's own user may connect. `SO_PEERCRED` (Linux) /
        // `LOCAL_PEERCRED` (macOS) is checked via `peer_cred`.
        match peer_uid(stream) {
            Some(uid) => uid == current_uid(),
            None => false,
        }
    }

    fn handle_conn(self: &Arc<Self>, mut stream: UnixStream) -> std::io::Result<()> {
        stream.set_nonblocking(false)?;
        if !Self::peer_ok(&stream) {
            return Ok(());
        }

        write_frame(
            &mut stream,
            &Frame::Hello(Hello {
                protocol: PROTOCOL_VERSION,
                version: env!("CARGO_PKG_VERSION").to_string(),
                pid: self.pid,
            }),
        )?;
        stream.flush()?;

        let req = match read_frame(&mut stream)? {
            Frame::Request(r) => r,
            other => {
                tracing::debug!(frame = other.label(), "expected a request first");
                return Ok(());
            }
        };

        match req {
            Request::Ping => reply(&mut stream, Reply::Pong),
            Request::List => {
                let sessions = self
                    .sessions
                    .lock()
                    .unwrap()
                    .values()
                    .map(|s| s.info())
                    .collect();
                reply(&mut stream, Reply::Sessions { sessions })
            }
            Request::Spawn(sr) => {
                let attach = sr.attach;
                let (rows, cols) = (sr.rows, sr.cols);
                match Session::spawn(&sr) {
                    Ok(session) => {
                        let info = session.info();
                        self.sessions
                            .lock()
                            .unwrap()
                            .insert(session.id.clone(), Arc::clone(&session));
                        self.mark_active();
                        if attach {
                            // Register before the reply so the connection is
                            // an attach stream with no gap after `Spawned`.
                            let handle = self.begin_attach(&session, &stream, rows, cols);
                            reply(&mut stream, Reply::Spawned { info })?;
                            if let Some((client_id, my_tx, rx)) = handle {
                                self.run_attach(&session, stream, client_id, my_tx, rx);
                            }
                        } else {
                            reply(&mut stream, Reply::Spawned { info })?;
                        }
                        Ok(())
                    }
                    Err(e) => reply(
                        &mut stream,
                        Reply::Error {
                            message: format!("spawn failed: {e}"),
                        },
                    ),
                }
            }
            Request::Attach { id, rows, cols } => match self.get(&id) {
                Some(session) => {
                    // Register before the reply so `info.attached` counts this
                    // client and no output between reply and registration is
                    // lost.
                    let handle = self.begin_attach(&session, &stream, rows, cols);
                    reply(
                        &mut stream,
                        Reply::Attached {
                            info: session.info(),
                            shell: session.shell_snapshot(),
                        },
                    )?;
                    if let Some((client_id, my_tx, rx)) = handle {
                        self.run_attach(&session, stream, client_id, my_tx, rx);
                    }
                    Ok(())
                }
                None => reply(
                    &mut stream,
                    Reply::Error {
                        message: format!("no session '{id}'"),
                    },
                ),
            },
            Request::Kill { id } => match self.get(&id) {
                Some(session) => {
                    session.kill();
                    reply(&mut stream, Reply::Ok)
                }
                None => reply(
                    &mut stream,
                    Reply::Error {
                        message: format!("no session '{id}'"),
                    },
                ),
            },
            Request::Remove { id } => {
                let removed = self.sessions.lock().unwrap().remove(&id);
                match removed {
                    Some(session) => {
                        session.kill();
                        self.mark_active();
                        reply(&mut stream, Reply::Ok)
                    }
                    None => reply(
                        &mut stream,
                        Reply::Error {
                            message: format!("no session '{id}'"),
                        },
                    ),
                }
            }
            Request::SetMeta(sm) => match self.get(&sm.id) {
                Some(session) => {
                    session.set_meta(|m| {
                        if sm.set_title {
                            m.title = sm.title;
                        }
                        if let Some(order) = sm.order {
                            m.order = order;
                        }
                        if let Some(wp) = sm.workspace_path {
                            m.workspace_path = wp;
                        }
                        if let Some(c) = sm.closable {
                            m.closable = c;
                        }
                    });
                    reply(&mut stream, Reply::Ok)
                }
                None => reply(
                    &mut stream,
                    Reply::Error {
                        message: format!("no session '{}'", sm.id),
                    },
                ),
            },
            Request::Shutdown { force } => {
                let live = self
                    .sessions
                    .lock()
                    .unwrap()
                    .values()
                    .filter(|s| s.is_live())
                    .count();
                if live > 0 && !force {
                    reply(
                        &mut stream,
                        Reply::Error {
                            message: format!("{live} live session(s); pass force to shut down"),
                        },
                    )
                } else {
                    reply(&mut stream, Reply::Ok)?;
                    self.kill_all();
                    self.request_shutdown();
                    Ok(())
                }
            }
        }
    }

    /// Register a client on `session` for `stream`. Returns the client id, a
    /// sender for `Resync`, and the receiver its writer thread drains — or
    /// `None` if the socket could not be cloned.
    #[allow(clippy::type_complexity)]
    fn begin_attach(
        &self,
        session: &Arc<Session>,
        stream: &UnixStream,
        rows: u16,
        cols: u16,
    ) -> Option<(u64, SyncSender<Frame>, Receiver<Frame>)> {
        let shutdown_clone = stream.try_clone().ok()?;
        let handle = session.attach(rows, cols, shutdown_clone);
        self.mark_active();
        Some(handle)
    }

    /// Drive an attach stream: a writer thread drains the client's outbound
    /// queue while this thread reads its input, until detach/EOF.
    fn run_attach(
        &self,
        session: &Arc<Session>,
        stream: UnixStream,
        client_id: u64,
        my_tx: SyncSender<Frame>,
        rx: Receiver<Frame>,
    ) {
        let mut write_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(_) => {
                session.detach(client_id);
                return;
            }
        };

        let writer = thread::Builder::new()
            .name("session-writer".into())
            .spawn(move || writer_loop(&mut write_stream, &rx))
            .ok();

        let mut read_stream = stream;
        loop {
            match read_frame(&mut read_stream) {
                Ok(Frame::Input(b)) => {
                    let _ = session.write_input(&b);
                }
                Ok(Frame::Resize { rows, cols }) => session.resize(rows, cols),
                Ok(Frame::Resync) => {
                    let _ = my_tx.try_send(Frame::Restore(session.restore()));
                }
                Ok(Frame::Kill) => {
                    session.kill();
                    break;
                }
                Ok(Frame::Detach) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }

        session.detach(client_id);
        drop(my_tx);
        if let Some(w) = writer {
            let _ = w.join();
        }
        self.mark_active();
    }

    fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().unwrap().get(id).cloned()
    }

    /// Reset the idle timer whenever the session/client population changes.
    fn mark_active(&self) {
        let idle = self.is_idle();
        *self.idle_since.lock().unwrap() = idle.then(Instant::now);
    }

    fn is_idle(&self) -> bool {
        let sessions = self.sessions.lock().unwrap();
        sessions.is_empty() || sessions.values().all(|s| s.attached_count() == 0)
    }

    fn reap_loop(self: &Arc<Self>) {
        // Poll the shutdown flag frequently so `serve` can return promptly;
        // only run the actual sweep every `REAP_INTERVAL`.
        let mut since_reap = Duration::ZERO;
        let step = Duration::from_millis(200);
        while !self.is_shutting_down() {
            thread::sleep(step);
            since_reap += step;
            if since_reap >= REAP_INTERVAL {
                since_reap = Duration::ZERO;
                self.reap_once();
            }
        }
    }

    fn reap_once(&self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let ttl_ms = EXITED_TTL.as_millis() as u64;
        self.sessions.lock().unwrap().retain(|_, s| {
            match s.exited_at_unix_ms() {
                Some(at) => now_ms.saturating_sub(at) < ttl_ms,
                None => true, // live
            }
        });

        // Idle-exit: nothing to serve for IDLE_EXIT → stop the daemon.
        if self.is_idle() {
            let mut idle_since = self.idle_since.lock().unwrap();
            let since = idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= IDLE_EXIT {
                tracing::info!("session daemon idle; shutting down");
                self.request_shutdown();
            }
        } else {
            *self.idle_since.lock().unwrap() = None;
        }
    }
}

/// Writer thread: forward queued frames to the socket, heart-beating when the
/// queue is quiet so a dead client is noticed within a heartbeat interval.
fn writer_loop(stream: &mut UnixStream, rx: &Receiver<Frame>) {
    loop {
        let frame = match rx.recv_timeout(HEARTBEAT) {
            Ok(f) => f,
            Err(RecvTimeoutError::Timeout) => Frame::Heartbeat,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if write_frame(stream, &frame)
            .and_then(|()| stream.flush())
            .is_err()
        {
            return;
        }
    }
}

fn reply(stream: &mut UnixStream, reply: Reply) -> std::io::Result<()> {
    write_frame(stream, &Frame::Reply(reply))?;
    stream.flush()
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}

#[cfg(unix)]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: fd is a valid connected socket; cred/len are live for the call.
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut len,
            )
        };
        (rc == 0).then_some(cred.uid)
    }
    #[cfg(target_os = "macos")]
    {
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        // SAFETY: fd is a valid connected socket; uid/gid are live.
        let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
        (rc == 0).then_some(uid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = stream;
        Some(current_uid())
    }
}
