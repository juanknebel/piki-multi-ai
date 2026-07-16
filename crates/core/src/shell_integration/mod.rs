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

use crate::cli_agent::CliAgentState;

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
    /// `\x1b]133;D[;<exit_code>]\x07` — command finished. `command` is the
    /// ANSI-stripped text the user typed (captured between `B` and `C`),
    /// best-effort: terminal redraws and multi-line edits can degrade the
    /// fidelity. `None` when capture was disabled or yielded nothing.
    CommandEnd {
        exit_code: Option<i32>,
        command: Option<String>,
    },
    /// `\x1b]7;file://<host>/<path>\x07` — cwd changed.
    CwdChanged(PathBuf),
    /// `\x1b]777;notify;piki://cli-agent;<json>\x07` — a structured Claude
    /// Code lifecycle event (Warp-style). Emitted only for the
    /// `piki://cli-agent` target; foreign OSC 777 sequences are ignored.
    CliAgent(crate::cli_agent::CliAgentEvent),
    /// `\x1b]0;`/`\x1b]1;`/`\x1b]2;<text>\x07` — window/icon title update.
    /// Used passively by [`crate::agent_state_detect`] to read a provider's
    /// own spinner/title convention (e.g. Codex) when there's no hook bridge.
    WindowTitle(String),
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
    /// Structured Claude Code agent state, populated from
    /// [`ShellEvent::CliAgent`] on Claude tabs. `None` until the first
    /// cli-agent event arrives (shell-only tabs never set it).
    pub cli_agent: Option<CliAgentState>,
    /// Latest window/icon title reported via OSC 0/1/2, if any.
    pub window_title: Option<String>,
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
            ShellEvent::CommandEnd { exit_code, .. } => {
                let now = Instant::now();
                let started = self.in_flight_started_at.take().unwrap_or(now);
                self.last_command = Some(CommandRecord {
                    started_at: started,
                    duration: now.saturating_duration_since(started),
                    exit_code: *exit_code,
                });
                self.last_attention_at = Some(now);
                // A foreground command just returned to the shell prompt. If a
                // cli-agent (a manually-run `claude`) was reporting through
                // this shell, it has now exited — clear its state so it drops
                // off the Agents pane. Dedicated agent tabs run the agent
                // directly (no shell integration), never see CommandEnd, and
                // keep their state.
                self.cli_agent = None;
            }
            ShellEvent::CliAgent(ev) => {
                self.cli_agent
                    .get_or_insert_with(CliAgentState::new)
                    .apply(ev);
            }
            ShellEvent::WindowTitle(t) => {
                self.window_title = Some(t.clone());
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
        if let Some(agent) = self.cli_agent.as_mut() {
            agent.acknowledge();
        }
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
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(0), command: None });
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
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(1), command: None });
        assert!(s.last_attention_at.is_some());
        s.acknowledge();
        assert!(s.last_attention_at.is_none());
    }

    #[test]
    fn command_end_without_start_still_records() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(2), command: None });
        let cmd = s.last_command.expect("command recorded");
        assert_eq!(cmd.exit_code, Some(2));
        assert_eq!(cmd.duration, Duration::ZERO);
    }

    #[test]
    fn command_end_clears_cli_agent_state() {
        // A manually-run `claude` reports through a shell tab...
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CliAgent(
            crate::cli_agent::CliAgentEvent::SessionStart {
                session_id: "s1".to_string(),
                cwd: None,
                project: None,
            },
        ));
        assert!(s.cli_agent.is_some(), "cli-agent state set by the event");

        // ...and drops off when the CLI exits and the shell returns to its
        // prompt (OSC 133 command-end).
        s.apply(&ShellEvent::CommandEnd { exit_code: Some(0), command: None });
        assert!(
            s.cli_agent.is_none(),
            "cli-agent state cleared once claude exits"
        );
    }
}
