//! Shell integration: inject init scripts that emit OSC 133 (prompt/command
//! markers + exit code) and OSC 7 (cwd reporting) into the user's shell, and
//! parse those sequences out of the PTY stream as structured events.
//!
//! Architecture:
//! - [`install`] resolves the user's shell, lays out a temporary init dir and
//!   returns env vars + extra args to feed `CommandBuilder` so the shell sources
//!   our integration script *and* the user's own dotfiles.
//! - [`parser::OscParser`] is a streaming state-machine that observes PTY
//!   bytes and emits [`ShellEvent`]s without modifying the stream.
//! - [`ShellTabState`] is the per-tab record kept in sync from those events
//!   (cwd, last command exit code, attention timestamp).
//!
//! Only `AIProvider::Shell` tabs use this — provider tabs (Claude/etc.) run
//! their binary directly without a shell wrapper, so OSC 133 doesn't apply.

use std::path::PathBuf;
use std::time::{Duration, Instant};

pub mod install;
pub mod parser;

/// Structured event extracted from the PTY stream by [`parser::OscParser`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// `\x1b]133;A\x07` — prompt is about to be drawn.
    PromptStart,
    /// `\x1b]133;B\x07` — prompt is drawn, user input begins.
    CommandInputStart,
    /// `\x1b]133;C\x07` — command output begins.
    CommandOutputStart,
    /// `\x1b]133;D[;<exit_code>]\x07` — command finished.
    CommandEnd { exit_code: Option<i32> },
    /// `\x1b]7;file://<host>/<path>\x07` — cwd changed.
    CwdChanged(PathBuf),
}

/// One executed command, captured between [`ShellEvent::CommandOutputStart`]
/// and [`ShellEvent::CommandEnd`].
#[derive(Debug, Clone)]
pub struct CommandRecord {
    pub started_at: Instant,
    pub duration: Duration,
    pub exit_code: Option<i32>,
}

impl CommandRecord {
    pub fn ok(&self) -> bool {
        matches!(self.exit_code, Some(0))
    }
}

/// Per-tab state derived from [`ShellEvent`] stream. UIs render badges, status
/// bars, and "needs attention" markers from this.
#[derive(Debug, Default)]
pub struct ShellTabState {
    pub cwd: Option<PathBuf>,
    pub last_command: Option<CommandRecord>,
    /// Set when a command finishes; UIs treat this as "user should look at
    /// this tab". Cleared when the user focuses the tab/workspace.
    pub last_attention_at: Option<Instant>,
    /// Wall-clock start of the in-flight command, if any. Set on
    /// `CommandOutputStart`, consumed on `CommandEnd`.
    in_flight_started_at: Option<Instant>,
}

impl ShellTabState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mutate the state in response to a parsed event.
    pub fn apply(&mut self, event: &ShellEvent) {
        match event {
            ShellEvent::CwdChanged(path) => {
                self.cwd = Some(path.clone());
            }
            ShellEvent::CommandOutputStart => {
                self.in_flight_started_at = Some(Instant::now());
            }
            ShellEvent::CommandEnd { exit_code } => {
                let now = Instant::now();
                let started = self.in_flight_started_at.take().unwrap_or(now);
                self.last_command = Some(CommandRecord {
                    started_at: started,
                    duration: now.saturating_duration_since(started),
                    exit_code: *exit_code,
                });
                self.last_attention_at = Some(now);
            }
            ShellEvent::PromptStart | ShellEvent::CommandInputStart => {
                // No-op — markers we keep for future UX (e.g. "scroll to
                // previous prompt"), no state change today.
            }
        }
    }

    /// Drop the attention marker (e.g. when the user focuses this tab).
    pub fn acknowledge(&mut self) {
        self.last_attention_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_command_lifecycle_records_exit_code_and_duration() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandOutputStart);
        std::thread::sleep(Duration::from_millis(5));
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(0) });
        let cmd = s.last_command.expect("command recorded");
        assert!(cmd.ok());
        assert!(cmd.duration >= Duration::from_millis(5));
        assert!(s.last_attention_at.is_some());
    }

    #[test]
    fn apply_cwd_changed_updates_cwd() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CwdChanged(PathBuf::from("/tmp/foo")));
        assert_eq!(s.cwd, Some(PathBuf::from("/tmp/foo")));
    }

    #[test]
    fn acknowledge_clears_attention() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandOutputStart);
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(1) });
        assert!(s.last_attention_at.is_some());
        s.acknowledge();
        assert!(s.last_attention_at.is_none());
    }

    #[test]
    fn command_end_without_start_still_records() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(2) });
        let cmd = s.last_command.expect("command recorded");
        assert_eq!(cmd.exit_code, Some(2));
        assert_eq!(cmd.duration, Duration::ZERO);
    }
}
