//! The persistent-session daemon: it owns every PTY and hands attached
//! clients a restore buffer so tabs survive the frontend. See
//! `docs/persistent-sessions.md`.
//!
//! [`Server`] is the connection engine (usable in-process for tests); [`run`]
//! is the process entry the binaries call — it daemonizes, takes a single
//! exclusive lock, cleans up a stale socket, installs signal cleanup and then
//! serves.

mod server;
mod session;

pub use server::Server;
pub use session::Session;

#[cfg(unix)]
mod launch;
#[cfg(unix)]
pub use launch::{DaemonPaths, run};
