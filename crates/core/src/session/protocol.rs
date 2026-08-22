//! Wire protocol between the session daemon and its clients.
//!
//! Transport is a Unix stream socket. Every message is one frame:
//!
//! ```text
//! [kind: u8][len: u32 little-endian][payload: len bytes]
//! ```
//!
//! `Input`, `Output` and `Restore` carry raw terminal bytes; every other
//! payload is a JSON document (serde). JSON was chosen over a binary codec
//! because control traffic is tiny, it is debuggable with `socat`, and it
//! needs no new dependency. Every struct field has `#[serde(default)]` so an
//! additive change never breaks an older peer; anything incompatible bumps
//! [`PROTOCOL_VERSION`], which the daemon advertises in [`Hello`] and the
//! client checks before doing anything else.
//!
//! Connection lifecycle (see `docs/persistent-sessions.md`):
//! 1. daemon → client: [`Frame::Hello`]
//! 2. client → daemon: one [`Frame::Request`]; daemon → client: one
//!    [`Frame::Reply`]
//! 3. for `Attach` / `Spawn { attach: true }` the connection then becomes an
//!    attach stream carrying the remaining frame kinds in both directions
//!    until either side detaches.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli_agent::CliAgentStatus;
use crate::shell_integration::ShellEvent;

/// Bump on any incompatible change. The daemon refuses nothing itself — the
/// *client* decides what to do with a mismatch (`docs/persistent-sessions.md`,
/// "Failure modes").
pub const PROTOCOL_VERSION: u32 = 1;

/// Upper bound on a single frame payload. A restore buffer for a 5000-line
/// scrollback at 300 columns with attributes is well under 4 MiB; anything
/// larger than this is a corrupt stream, not a real message.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// Largest `Output` chunk the daemon emits; mirrors the PTY read buffer.
pub const OUTPUT_CHUNK: usize = 64 * 1024;

// ── Handshake ────────────────────────────────────────────────────────────

/// First frame on every connection, daemon → client.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Hello {
    #[serde(default)]
    pub protocol: u32,
    /// Daemon binary version (`CARGO_PKG_VERSION`), for display only.
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub pid: u32,
}

// ── Requests / replies ───────────────────────────────────────────────────

/// The one request a client sends after `Hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Presence probe.
    Ping,
    /// Every session the daemon knows about (live and exited).
    List,
    /// Create a session. With `attach: true` the connection turns into an
    /// attach stream right after the `Spawned` reply.
    Spawn(SpawnRequest),
    /// Attach to an existing session at the given terminal size; the
    /// connection turns into an attach stream after the `Attached` reply.
    Attach { id: String, rows: u16, cols: u16 },
    /// SIGHUP the child (SIGKILL after a grace period). The session stays,
    /// as *exited*, until `Remove`d.
    Kill { id: String },
    /// Drop a session entirely (kills it first if it is still live).
    Remove { id: String },
    /// Update the tab metadata the frontends persist on the session.
    SetMeta(SetMetaRequest),
    /// Stop the daemon. Refused while live sessions exist unless `force`.
    Shutdown {
        #[serde(default)]
        force: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Pong,
    Ok,
    Sessions {
        #[serde(default)]
        sessions: Vec<SessionInfo>,
    },
    Spawned {
        info: SessionInfo,
    },
    Attached {
        info: SessionInfo,
        #[serde(default)]
        shell: ShellStateSnapshot,
    },
    Error {
        #[serde(default)]
        message: String,
    },
}

/// Everything the daemon needs to start a child. The client sends the
/// **complete** environment and the daemon `env_clear()`s before applying
/// it, so spawns are identical whichever binary started the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Session id chosen by the client (uuid v4); doubles as the tab id.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// Run the OSC 133/7/777 parser over this session's output.
    #[serde(default)]
    pub integration_on: bool,
    /// Per-session FIFO for the structured cli-agent channel; the daemon
    /// creates, reads and eventually unlinks it.
    #[serde(default)]
    pub cli_agent_sock: Option<PathBuf>,
    #[serde(default)]
    pub meta: SessionMeta,
    /// Become an attach stream after the `Spawned` reply.
    #[serde(default)]
    pub attach: bool,
}

fn default_rows() -> u16 {
    24
}

fn default_cols() -> u16 {
    80
}

fn default_true() -> bool {
    true
}

/// Tab metadata the frontends keep on the session so the daemon is the
/// single source of truth for PTY tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Workspace this tab belongs to (its worktree path).
    #[serde(default)]
    pub workspace_path: PathBuf,
    /// `AIProvider` label (`Shell`, `Git`) or the custom provider name.
    #[serde(default)]
    pub provider: String,
    /// User-set tab title (`Tab::custom_title`).
    #[serde(default)]
    pub title: Option<String>,
    /// Position within the workspace's tab bar.
    #[serde(default)]
    pub order: u32,
    /// `Tab::closable`.
    #[serde(default = "default_true")]
    pub closable: bool,
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self {
            workspace_path: PathBuf::new(),
            provider: String::new(),
            title: None,
            order: 0,
            closable: true,
        }
    }
}

/// Partial update of [`SessionMeta`]; `None` leaves a field alone. The
/// title needs a separate `set_title` flag because clearing it is a real
/// update (`title: None` + `set_title: true`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SetMetaRequest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub set_title: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub order: Option<u32>,
    #[serde(default)]
    pub workspace_path: Option<PathBuf>,
    #[serde(default)]
    pub closable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionState {
    #[default]
    Live,
    Exited {
        #[serde(default)]
        code: Option<i32>,
    },
}

impl SessionState {
    pub fn is_live(&self) -> bool {
        matches!(self, SessionState::Live)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub meta: SessionMeta,
    #[serde(default)]
    pub cwd: PathBuf,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub state: SessionState,
    /// Number of clients currently attached.
    #[serde(default)]
    pub attached: usize,
    #[serde(default)]
    pub rows: u16,
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub created_at_unix_ms: u64,
    #[serde(default)]
    pub exited_at_unix_ms: Option<u64>,
}

// ── Shell / agent state ──────────────────────────────────────────────────

/// Wire form of [`crate::shell_integration::ShellTabState`]: what a client
/// needs to rebuild its per-tab status the moment it attaches. `Instant`s
/// become relative durations.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ShellStateSnapshot {
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub last_command: Option<CommandSnapshot>,
    /// A command finished and the user has not looked at the tab since.
    #[serde(default)]
    pub attention_pending: bool,
    /// How long the in-flight command has been running, if any.
    #[serde(default)]
    pub in_flight_for_ms: Option<u64>,
    #[serde(default)]
    pub cli_agent: Option<CliAgentSnapshot>,
    #[serde(default)]
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CommandSnapshot {
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: u64,
    /// How long ago the command finished.
    #[serde(default)]
    pub finished_ago_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CliAgentSnapshot {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: CliAgentStatus,
    #[serde(default)]
    pub last_summary: Option<String>,
    #[serde(default)]
    pub attention_pending: bool,
}

// ── Frames ───────────────────────────────────────────────────────────────

/// Every message that can cross the socket, in either direction. Which
/// frames are meaningful when is defined by the connection lifecycle in the
/// module docs; a peer ignores frames it does not expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Hello(Hello),
    Request(Request),
    Reply(Reply),
    /// Client → daemon: keystrokes for the PTY.
    Input(Vec<u8>),
    /// Daemon → client: live PTY output.
    Output(Vec<u8>),
    /// Daemon → client: restore buffer (screen + scrollback + modes). Sent
    /// right after `Attached`/`Spawned` and in reply to `Resync`.
    Restore(Vec<u8>),
    /// Client → daemon.
    Resize {
        rows: u16,
        cols: u16,
    },
    /// Client → daemon: send `Restore` again at the current size.
    Resync,
    /// Client → daemon: leave the session running.
    Detach,
    /// Client → daemon: same as `Request::Kill` for this session.
    Kill,
    /// Daemon → client: full shell/agent state, sent once after attach.
    ShellState(ShellStateSnapshot),
    /// Daemon → client: one shell-integration event. `replayed` marks
    /// events buffered while nobody was attached.
    ShellEvent {
        event: ShellEvent,
        replayed: bool,
    },
    /// Daemon → client: the child exited.
    Exited {
        code: Option<i32>,
    },
    /// Daemon → client: liveness probe; a failed write drops the client.
    Heartbeat,
    /// Daemon → client: the daemon is closing this attachment.
    Detached {
        reason: String,
    },
}

mod kind {
    pub const HELLO: u8 = 1;
    pub const REQUEST: u8 = 2;
    pub const REPLY: u8 = 3;
    pub const INPUT: u8 = 10;
    pub const OUTPUT: u8 = 11;
    pub const RESTORE: u8 = 12;
    pub const RESIZE: u8 = 13;
    pub const RESYNC: u8 = 14;
    pub const DETACH: u8 = 15;
    pub const KILL: u8 = 16;
    pub const SHELL_STATE: u8 = 20;
    pub const SHELL_EVENT: u8 = 21;
    pub const EXITED: u8 = 22;
    pub const HEARTBEAT: u8 = 23;
    pub const DETACHED: u8 = 24;
}

#[derive(Serialize, Deserialize)]
struct ResizeMsg {
    #[serde(default)]
    rows: u16,
    #[serde(default)]
    cols: u16,
}

#[derive(Serialize, Deserialize)]
struct ShellEventMsg {
    event: ShellEvent,
    #[serde(default)]
    replayed: bool,
}

#[derive(Serialize, Deserialize)]
struct ExitedMsg {
    #[serde(default)]
    code: Option<i32>,
}

#[derive(Serialize, Deserialize)]
struct DetachedMsg {
    #[serde(default)]
    reason: String,
}

fn json<T: Serialize>(v: &T) -> io::Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn parse<T: for<'de> Deserialize<'de>>(payload: &[u8]) -> io::Result<T> {
    serde_json::from_slice(payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

impl Frame {
    /// Frame kind tag and payload bytes.
    fn encode(&self) -> io::Result<(u8, Vec<u8>)> {
        Ok(match self {
            Frame::Hello(h) => (kind::HELLO, json(h)?),
            Frame::Request(r) => (kind::REQUEST, json(r)?),
            Frame::Reply(r) => (kind::REPLY, json(r)?),
            Frame::Input(b) => (kind::INPUT, b.clone()),
            Frame::Output(b) => (kind::OUTPUT, b.clone()),
            Frame::Restore(b) => (kind::RESTORE, b.clone()),
            Frame::Resize { rows, cols } => (
                kind::RESIZE,
                json(&ResizeMsg {
                    rows: *rows,
                    cols: *cols,
                })?,
            ),
            Frame::Resync => (kind::RESYNC, Vec::new()),
            Frame::Detach => (kind::DETACH, Vec::new()),
            Frame::Kill => (kind::KILL, Vec::new()),
            Frame::ShellState(s) => (kind::SHELL_STATE, json(s)?),
            Frame::ShellEvent { event, replayed } => (
                kind::SHELL_EVENT,
                json(&ShellEventMsg {
                    event: event.clone(),
                    replayed: *replayed,
                })?,
            ),
            Frame::Exited { code } => (kind::EXITED, json(&ExitedMsg { code: *code })?),
            Frame::Heartbeat => (kind::HEARTBEAT, Vec::new()),
            Frame::Detached { reason } => (
                kind::DETACHED,
                json(&DetachedMsg {
                    reason: reason.clone(),
                })?,
            ),
        })
    }

    fn decode(tag: u8, payload: Vec<u8>) -> io::Result<Frame> {
        Ok(match tag {
            kind::HELLO => Frame::Hello(parse(&payload)?),
            kind::REQUEST => Frame::Request(parse(&payload)?),
            kind::REPLY => Frame::Reply(parse(&payload)?),
            kind::INPUT => Frame::Input(payload),
            kind::OUTPUT => Frame::Output(payload),
            kind::RESTORE => Frame::Restore(payload),
            kind::RESIZE => {
                let m: ResizeMsg = parse(&payload)?;
                Frame::Resize {
                    rows: m.rows,
                    cols: m.cols,
                }
            }
            kind::RESYNC => Frame::Resync,
            kind::DETACH => Frame::Detach,
            kind::KILL => Frame::Kill,
            kind::SHELL_STATE => Frame::ShellState(parse(&payload)?),
            kind::SHELL_EVENT => {
                let m: ShellEventMsg = parse(&payload)?;
                Frame::ShellEvent {
                    event: m.event,
                    replayed: m.replayed,
                }
            }
            kind::EXITED => {
                let m: ExitedMsg = parse(&payload)?;
                Frame::Exited { code: m.code }
            }
            kind::HEARTBEAT => Frame::Heartbeat,
            kind::DETACHED => {
                let m: DetachedMsg = parse(&payload)?;
                Frame::Detached { reason: m.reason }
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown frame kind {other}"),
                ));
            }
        })
    }

    /// Short human label for logs.
    pub fn label(&self) -> &'static str {
        match self {
            Frame::Hello(_) => "hello",
            Frame::Request(_) => "request",
            Frame::Reply(_) => "reply",
            Frame::Input(_) => "input",
            Frame::Output(_) => "output",
            Frame::Restore(_) => "restore",
            Frame::Resize { .. } => "resize",
            Frame::Resync => "resync",
            Frame::Detach => "detach",
            Frame::Kill => "kill",
            Frame::ShellState(_) => "shell_state",
            Frame::ShellEvent { .. } => "shell_event",
            Frame::Exited { .. } => "exited",
            Frame::Heartbeat => "heartbeat",
            Frame::Detached { .. } => "detached",
        }
    }
}

/// Serialize `frame` as one wire frame.
pub fn encode_frame(frame: &Frame) -> io::Result<Vec<u8>> {
    let (tag, payload) = frame.encode()?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame payload of {} bytes exceeds the limit", payload.len()),
        ));
    }
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(tag);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Write one frame. Does not flush — callers batch and flush themselves.
pub fn write_frame<W: Write>(w: &mut W, frame: &Frame) -> io::Result<()> {
    w.write_all(&encode_frame(frame)?)
}

/// Read one frame. A clean EOF *between* frames surfaces as
/// [`io::ErrorKind::UnexpectedEof`] with an empty message, so callers can
/// distinguish "peer hung up" from a torn frame if they care.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Frame> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head)?;
    let tag = head[0];
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame payload of {len} bytes exceeds the limit"),
        ));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Frame::decode(tag, payload)
}

/// `true` for the errors [`read_frame`] returns when the peer simply closed
/// the connection (EOF, reset, broken pipe) rather than sent garbage.
pub fn is_disconnect(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_agent::CliAgentEvent;
    use std::io::Cursor;

    fn round_trip(frame: Frame) {
        let bytes = encode_frame(&frame).unwrap();
        let decoded = read_frame(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn every_frame_kind_round_trips() {
        round_trip(Frame::Hello(Hello {
            protocol: PROTOCOL_VERSION,
            version: "2.10.0".into(),
            pid: 4242,
        }));
        round_trip(Frame::Request(Request::Ping));
        round_trip(Frame::Request(Request::List));
        round_trip(Frame::Request(Request::Spawn(SpawnRequest {
            id: "abc".into(),
            command: "/bin/zsh".into(),
            args: vec!["-l".into()],
            cwd: PathBuf::from("/tmp/ws"),
            env: vec![("TERM".into(), "xterm".into())],
            rows: 40,
            cols: 120,
            integration_on: true,
            cli_agent_sock: Some(PathBuf::from("/tmp/x.sock")),
            meta: SessionMeta {
                workspace_path: PathBuf::from("/tmp/ws"),
                provider: "Shell".into(),
                title: Some("build".into()),
                order: 3,
                closable: false,
            },
            attach: true,
        })));
        round_trip(Frame::Request(Request::Attach {
            id: "abc".into(),
            rows: 24,
            cols: 80,
        }));
        round_trip(Frame::Request(Request::Kill { id: "abc".into() }));
        round_trip(Frame::Request(Request::Remove { id: "abc".into() }));
        round_trip(Frame::Request(Request::SetMeta(SetMetaRequest {
            id: "abc".into(),
            set_title: true,
            title: None,
            order: Some(7),
            workspace_path: None,
            closable: Some(true),
        })));
        round_trip(Frame::Request(Request::Shutdown { force: true }));
        round_trip(Frame::Reply(Reply::Pong));
        round_trip(Frame::Reply(Reply::Ok));
        round_trip(Frame::Reply(Reply::Sessions {
            sessions: vec![SessionInfo {
                id: "abc".into(),
                state: SessionState::Exited { code: Some(1) },
                attached: 2,
                ..Default::default()
            }],
        }));
        round_trip(Frame::Reply(Reply::Spawned {
            info: SessionInfo::default(),
        }));
        round_trip(Frame::Reply(Reply::Attached {
            info: SessionInfo::default(),
            shell: ShellStateSnapshot {
                cwd: Some(PathBuf::from("/x")),
                last_command: Some(CommandSnapshot {
                    exit_code: Some(0),
                    duration_ms: 12,
                    finished_ago_ms: 3000,
                }),
                attention_pending: true,
                in_flight_for_ms: None,
                cli_agent: Some(CliAgentSnapshot {
                    session_id: Some("s1".into()),
                    status: CliAgentStatus::WaitingPermission,
                    last_summary: Some("Wants to run Bash".into()),
                    attention_pending: true,
                }),
                window_title: Some("codex".into()),
            },
        }));
        round_trip(Frame::Reply(Reply::Error {
            message: "nope".into(),
        }));
        round_trip(Frame::Input(b"ls\r".to_vec()));
        round_trip(Frame::Output(b"\x1b[31mhi\x1b[m".to_vec()));
        round_trip(Frame::Restore(vec![0, 255, 1, 2]));
        round_trip(Frame::Resize {
            rows: 50,
            cols: 200,
        });
        round_trip(Frame::Resync);
        round_trip(Frame::Detach);
        round_trip(Frame::Kill);
        round_trip(Frame::ShellState(ShellStateSnapshot::default()));
        round_trip(Frame::ShellEvent {
            event: ShellEvent::CommandEnd {
                exit_code: Some(2),
                command: Some("make".into()),
            },
            replayed: true,
        });
        round_trip(Frame::ShellEvent {
            event: ShellEvent::CliAgent(CliAgentEvent::Stop {
                session_id: "s".into(),
                query: None,
                response: Some("done".into()),
                transcript_path: Some(PathBuf::from("/t.jsonl")),
            }),
            replayed: false,
        });
        round_trip(Frame::ShellEvent {
            event: ShellEvent::CwdChanged(PathBuf::from("/somewhere")),
            replayed: false,
        });
        round_trip(Frame::Exited { code: Some(130) });
        round_trip(Frame::Exited { code: None });
        round_trip(Frame::Heartbeat);
        round_trip(Frame::Detached {
            reason: "shutdown".into(),
        });
    }

    #[test]
    fn several_frames_back_to_back() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &Frame::Heartbeat).unwrap();
        write_frame(&mut buf, &Frame::Output(b"abc".to_vec())).unwrap();
        write_frame(&mut buf, &Frame::Exited { code: Some(0) }).unwrap();
        let mut cur = Cursor::new(buf);
        assert_eq!(read_frame(&mut cur).unwrap(), Frame::Heartbeat);
        assert_eq!(
            read_frame(&mut cur).unwrap(),
            Frame::Output(b"abc".to_vec())
        );
        assert_eq!(
            read_frame(&mut cur).unwrap(),
            Frame::Exited { code: Some(0) }
        );
        let eof = read_frame(&mut cur).unwrap_err();
        assert_eq!(eof.kind(), io::ErrorKind::UnexpectedEof);
        assert!(is_disconnect(&eof));
    }

    #[test]
    fn torn_frame_is_unexpected_eof() {
        let bytes = encode_frame(&Frame::Output(b"hello".to_vec())).unwrap();
        let err = read_frame(&mut Cursor::new(&bytes[..bytes.len() - 2])).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn oversized_and_unknown_frames_are_invalid_data() {
        let mut bytes = vec![kind::OUTPUT];
        bytes.extend_from_slice(&((MAX_FRAME_LEN as u32) + 1).to_le_bytes());
        let err = read_frame(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        let mut bytes = vec![99u8];
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let err = read_frame(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        let mut bytes = vec![kind::REQUEST];
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"{{{");
        let err = read_frame(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// Additive protocol changes must not break older peers: unknown fields
    /// are ignored and missing ones take their defaults.
    #[test]
    fn json_payloads_are_forward_and_backward_compatible() {
        let hello: Hello =
            serde_json::from_str(r#"{"protocol":1,"version":"x","pid":1,"future":true}"#).unwrap();
        assert_eq!(hello.protocol, 1);
        let hello: Hello = serde_json::from_str("{}").unwrap();
        assert_eq!(hello, Hello::default());

        let req: Request = serde_json::from_str(
            r#"{"type":"spawn","id":"s","command":"sh","meta":{"provider":"Shell"}}"#,
        )
        .unwrap();
        match req {
            Request::Spawn(s) => {
                assert_eq!(s.rows, 24);
                assert_eq!(s.cols, 80);
                assert!(s.meta.closable, "closable defaults to true");
                assert_eq!(s.meta.provider, "Shell");
                assert!(!s.attach);
            }
            other => panic!("unexpected {other:?}"),
        }

        let info: SessionInfo =
            serde_json::from_str(r#"{"id":"a","state":{"state":"exited"}}"#).unwrap();
        assert_eq!(info.state, SessionState::Exited { code: None });
        let info: SessionInfo = serde_json::from_str(r#"{"id":"a"}"#).unwrap();
        assert!(info.state.is_live());
    }

    #[test]
    fn request_tags_are_stable_snake_case() {
        let s = serde_json::to_string(&Request::Shutdown { force: false }).unwrap();
        assert_eq!(s, r#"{"type":"shutdown","force":false}"#);
        let s = serde_json::to_string(&Reply::Pong).unwrap();
        assert_eq!(s, r#"{"type":"pong"}"#);
        let s = serde_json::to_string(&SessionState::Exited { code: Some(3) }).unwrap();
        assert_eq!(s, r#"{"state":"exited","code":3}"#);
    }
}
