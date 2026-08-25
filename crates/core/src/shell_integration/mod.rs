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

use serde::{Deserialize, Serialize};

use crate::cli_agent::CliAgentState;
use crate::session::protocol::{CliAgentSnapshot, CommandSnapshot, ShellStateSnapshot};

pub mod install;
pub mod parser;

/// Structured event extracted from the PTY stream by [`parser::OscParser`].
///
/// Serializable because the session daemon forwards these to attached
/// clients over the wire (`session::protocol::Frame::ShellEvent`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Wire form for the session daemon → client handoff. `Instant`s become
    /// durations relative to now; [`from_snapshot`](Self::from_snapshot)
    /// turns them back.
    pub fn snapshot(&self) -> ShellStateSnapshot {
        let now = Instant::now();
        ShellStateSnapshot {
            cwd: self.cwd.clone(),
            last_command: self.last_command.as_ref().map(|c| CommandSnapshot {
                exit_code: c.exit_code,
                duration_ms: c.duration.as_millis() as u64,
                finished_ago_ms: now
                    .saturating_duration_since(c.started_at + c.duration)
                    .as_millis() as u64,
            }),
            attention_pending: self.last_attention_at.is_some(),
            in_flight_for_ms: self
                .in_flight_started_at
                .map(|t| now.saturating_duration_since(t).as_millis() as u64),
            cli_agent: self.cli_agent.as_ref().map(|a| CliAgentSnapshot {
                session_id: a.session_id.clone(),
                status: a.status,
                last_summary: a.last_summary.clone(),
                attention_pending: a.last_attention_at.is_some(),
                run_for_ms: a.elapsed().map(|d| d.as_millis() as u64),
            }),
            window_title: self.window_title.clone(),
        }
    }

    /// Rebuild the state a daemon-side tab had at snapshot time. Attention
    /// markers that were pending are re-armed as of now, which is what the
    /// UIs need (they only test `is_some()`).
    pub fn from_snapshot(s: &ShellStateSnapshot) -> Self {
        let now = Instant::now();
        let last_command = s.last_command.as_ref().map(|c| {
            let duration = Duration::from_millis(c.duration_ms);
            let finished_at = now
                .checked_sub(Duration::from_millis(c.finished_ago_ms))
                .unwrap_or(now);
            CommandRecord {
                started_at: finished_at.checked_sub(duration).unwrap_or(finished_at),
                duration,
                exit_code: c.exit_code,
            }
        });
        let cli_agent = s.cli_agent.as_ref().map(|a| CliAgentState {
            session_id: a.session_id.clone(),
            status: a.status,
            last_summary: a.last_summary.clone(),
            last_attention_at: a.attention_pending.then_some(now),
            run_started_at: a
                .run_for_ms
                .map(|ms| now.checked_sub(Duration::from_millis(ms)).unwrap_or(now)),
        });
        Self {
            cwd: s.cwd.clone(),
            last_command,
            last_attention_at: s.attention_pending.then_some(now),
            in_flight_started_at: s
                .in_flight_for_ms
                .map(|ms| now.checked_sub(Duration::from_millis(ms)).unwrap_or(now)),
            cli_agent,
            window_title: s.window_title.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_agent::CliAgentStatus;

    #[test]
    fn apply_command_lifecycle_records_exit_code_and_duration() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandOutputStart);
        std::thread::sleep(Duration::from_millis(5));
        s.apply(&ShellEvent::CommandEnd {
            exit_code: Some(0),
            command: None,
        });
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
        s.apply(&ShellEvent::CommandEnd {
            exit_code: Some(1),
            command: None,
        });
        assert!(s.last_attention_at.is_some());
        s.acknowledge();
        assert!(s.last_attention_at.is_none());
    }

    #[test]
    fn command_end_without_start_still_records() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandEnd {
            exit_code: Some(2),
            command: None,
        });
        let cmd = s.last_command.expect("command recorded");
        assert_eq!(cmd.exit_code, Some(2));
        assert_eq!(cmd.duration, Duration::ZERO);
    }

    #[test]
    fn snapshot_round_trips_everything_the_ui_reads() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CwdChanged(PathBuf::from("/work/here")));
        s.apply(&ShellEvent::WindowTitle("codex ⠋".to_string()));
        s.apply(&ShellEvent::CommandOutputStart);
        std::thread::sleep(Duration::from_millis(5));
        s.apply(&ShellEvent::CommandEnd {
            exit_code: Some(7),
            command: Some("make".to_string()),
        });
        s.apply(&ShellEvent::CliAgent(
            crate::cli_agent::CliAgentEvent::PermissionRequest {
                session_id: "s9".to_string(),
                tool_name: "Bash".to_string(),
                summary: "Wants to run Bash: ls".to_string(),
            },
        ));

        let snap = s.snapshot();
        assert_eq!(snap.cwd, Some(PathBuf::from("/work/here")));
        assert_eq!(snap.window_title.as_deref(), Some("codex ⠋"));
        let cmd = snap.last_command.as_ref().unwrap();
        assert_eq!(cmd.exit_code, Some(7));
        assert!(cmd.duration_ms >= 5);
        assert!(snap.attention_pending);
        assert!(snap.in_flight_for_ms.is_none());
        let agent = snap.cli_agent.as_ref().unwrap();
        assert_eq!(agent.status, CliAgentStatus::WaitingPermission);
        assert!(agent.attention_pending);
        assert_eq!(agent.session_id.as_deref(), Some("s9"));

        let back = ShellTabState::from_snapshot(&snap);
        assert_eq!(back.cwd, s.cwd);
        assert_eq!(back.window_title, s.window_title);
        let rec = back.last_command.unwrap();
        assert_eq!(rec.exit_code, Some(7));
        assert_eq!(rec.duration.as_millis() as u64, cmd.duration_ms);
        assert!(back.last_attention_at.is_some());
        let agent = back.cli_agent.unwrap();
        assert_eq!(agent.status, CliAgentStatus::WaitingPermission);
        assert_eq!(agent.last_summary.as_deref(), Some("Wants to run Bash: ls"));
        assert!(agent.last_attention_at.is_some());

        // And a quiet tab stays quiet.
        let quiet = ShellTabState::from_snapshot(&ShellTabState::new().snapshot());
        assert!(quiet.last_attention_at.is_none());
        assert!(quiet.cli_agent.is_none());
        assert!(quiet.last_command.is_none());
    }

    #[test]
    fn in_flight_command_survives_the_snapshot() {
        let mut s = ShellTabState::new();
        s.apply(&ShellEvent::CommandOutputStart);
        std::thread::sleep(Duration::from_millis(5));
        let snap = s.snapshot();
        assert!(snap.in_flight_for_ms.unwrap() >= 5);
        let mut back = ShellTabState::from_snapshot(&snap);
        back.apply(&ShellEvent::CommandEnd {
            exit_code: Some(0),
            command: None,
        });
        assert!(back.last_command.unwrap().duration >= Duration::from_millis(5));
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
        s.apply(&ShellEvent::CommandEnd {
            exit_code: Some(0),
            command: None,
        });
        assert!(
            s.cli_agent.is_none(),
            "cli-agent state cleared once claude exits"
        );
    }
}
