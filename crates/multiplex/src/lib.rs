//! `piki-multiplex`: a terminal multiplexer engine.
//!
//! - [`pty`] — spawn a PTY, feed a `vt100::Parser`, batch output for a render
//!   loop. Local (in-process) and remote (daemon-attached) variants share the
//!   same [`pty::PtySession`] facade.
//! - [`session`] — a headless daemon that owns every PTY so tabs survive the
//!   frontend that opened them; thin clients attach over a Unix socket and
//!   receive a restore buffer that rebuilds the screen + scrollback. See
//!   `docs/persistent-sessions.md` in the workspace root for the design.
//! - [`shell_integration`] — OSC 133 (prompt/command markers) and OSC 7 (cwd)
//!   parsing, turning a PTY byte stream into structured [`shell_integration::ShellEvent`]s.
//! - [`cli_agent`] — an optional, opinionated out-of-band channel (FIFO +
//!   OSC 777) for a structured agent-lifecycle event stream. Inert unless a
//!   caller opts in by passing a `cli_agent_sock` path at spawn time.

pub mod cli_agent;
pub mod pty;
pub mod session;
pub mod shell_integration;
