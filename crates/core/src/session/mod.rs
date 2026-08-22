//! Persistent sessions: a headless daemon owns every PTY (and a terminal
//! emulator per session) so tabs survive the frontend that opened them;
//! the TUI and the desktop app attach over a Unix socket and receive a
//! restore buffer that rebuilds the screen + scrollback.
//!
//! Design: `docs/persistent-sessions.md`. Layout:
//! - [`protocol`] — wire types + framing shared by daemon and clients.
//! - [`restore`] — turning a daemon-side `vt100` parser into the byte stream
//!   a freshly attached client replays.

pub mod client;
pub mod daemon;
pub mod protocol;
pub mod restore;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A process-unique session id (doubles as the daemon-side tab id): pid + a
/// process-global counter + wall-clock nanos. Collision-free across
/// concurrent spawns in one process (counter) and across separate processes
/// (pid + nanos). Same shape as the cli-agent FIFO naming.
pub fn new_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{pid}-{n}-{nanos}")
}
