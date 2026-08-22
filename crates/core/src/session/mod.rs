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
