//! Structured CLI-agent integration (Warp-style).
//!
//! Claude Code exposes lifecycle *hooks*. We ship tiny hook scripts (see
//! [`install`]) that, on every lifecycle transition, emit an **in-band** OSC
//! 777 escape sequence into the PTY byte stream:
//!
//! ```text
//! ESC ] 777 ; notify ; piki://cli-agent ; <json> BEL
//! ```
//!
//! [`crate::shell_integration::parser::OscParser`] already observes the PTY
//! stream for OSC 133/7; it grows one extra arm that recognises the
//! `piki://cli-agent` target, parses the JSON here, and emits a
//! [`crate::shell_integration::ShellEvent::CliAgent`]. The agent itself stays
//! a raw PTY passthrough — this layer is purely additive and self-disabling
//! (the hook is a no-op unless `PIKI_CLI_AGENT` is set in its env).
//!
//! JSON payload base shape: `{v, agent, event, session_id, cwd, project,
//! ...event-specific}`. `v` is a protocol version negotiated as
//! `min(script_version, piki_version)`; payloads whose major we don't
//! understand are dropped (the tab falls back to the heuristic idle watcher).

use std::path::PathBuf;
use std::time::Instant;

use serde::Deserialize;

pub mod install;
#[cfg(unix)]
pub mod sock;

/// Protocol version this build of piki understands. The hook script sends
/// `min(its_version, $PIKI_CLI_AGENT_V)`, so a payload with `v` greater than
/// this is rejected rather than mis-parsed.
pub const CLI_AGENT_PROTOCOL_VERSION: u32 = 1;

/// A single structured lifecycle event decoded from a `piki://cli-agent`
/// OSC 777 payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAgentEvent {
    /// Session started (Claude Code launched / resumed).
    SessionStart {
        session_id: String,
        cwd: Option<PathBuf>,
        project: Option<String>,
    },
    /// User submitted a prompt — the agent is working again.
    UserPromptSubmit { session_id: String },
    /// A tool call finished — the agent is no longer blocked on that tool.
    PostToolUse {
        session_id: String,
        tool_name: Option<String>,
    },
    /// The agent wants to run a tool and needs the user's approval.
    PermissionRequest {
        session_id: String,
        tool_name: String,
        /// Human-readable one-liner built by the hook (e.g.
        /// `Wants to run Bash: rm -rf build`).
        summary: String,
    },
    /// The agent emitted a notification (e.g. it has been idle and is
    /// waiting for input). `kind` mirrors Claude Code's notification matcher
    /// (e.g. `idle_prompt`).
    Notification { session_id: String, kind: String },
    /// The agent finished its turn. `response`/`query` are truncated previews;
    /// `transcript_path` points at the full JSONL transcript for lazy reads.
    Stop {
        session_id: String,
        query: Option<String>,
        response: Option<String>,
        transcript_path: Option<PathBuf>,
    },
}

impl CliAgentEvent {
    pub fn session_id(&self) -> &str {
        match self {
            CliAgentEvent::SessionStart { session_id, .. }
            | CliAgentEvent::UserPromptSubmit { session_id }
            | CliAgentEvent::PostToolUse { session_id, .. }
            | CliAgentEvent::PermissionRequest { session_id, .. }
            | CliAgentEvent::Notification { session_id, .. }
            | CliAgentEvent::Stop { session_id, .. } => session_id,
        }
    }

    /// For events that warrant pulling the user's attention (a notification +
    /// sidebar badge), returns the notification `kind` and an optional
    /// human-readable summary. Purely informational lifecycle events
    /// (session start, prompt submit, tool complete) return `None`.
    ///
    /// Single source of truth shared by the TUI badge path and (indirectly)
    /// the desktop notification path so the two frontends stay consistent.
    pub fn attention(&self) -> Option<(&'static str, Option<&str>)> {
        match self {
            CliAgentEvent::PermissionRequest { summary, .. } => {
                Some(("permission_request", Some(summary.as_str())))
            }
            CliAgentEvent::Notification { .. } => Some(("notification", None)),
            CliAgentEvent::Stop { response, .. } => Some(("stop", response.as_deref())),
            CliAgentEvent::SessionStart { .. }
            | CliAgentEvent::UserPromptSubmit { .. }
            | CliAgentEvent::PostToolUse { .. } => None,
        }
    }
}

/// Coarse per-tab agent status derived from the event stream. UIs render a
/// per-tab glyph / attention badge from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CliAgentStatus {
    /// Working (prompt submitted, tool running, just started).
    #[default]
    Running,
    /// Blocked on a permission prompt — needs the user.
    WaitingPermission,
    /// Idle and waiting for the user to type.
    Idle,
    /// Finished its turn.
    Done,
}

/// Per-tab state derived from the [`CliAgentEvent`] stream. Mirrors the role
/// of [`crate::shell_integration::ShellTabState`] for shell tabs.
#[derive(Debug, Default)]
pub struct CliAgentState {
    pub session_id: Option<String>,
    pub status: CliAgentStatus,
    /// Last human-relevant text to surface (permission summary, or the
    /// agent's final response preview on `Stop`).
    pub last_summary: Option<String>,
    /// Set when the user should look at this tab (permission / idle / done).
    /// Cleared by [`acknowledge`](Self::acknowledge) on focus.
    pub last_attention_at: Option<Instant>,
}

impl CliAgentState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: &CliAgentEvent) {
        self.session_id = Some(event.session_id().to_string());
        match event {
            CliAgentEvent::SessionStart { .. }
            | CliAgentEvent::UserPromptSubmit { .. }
            | CliAgentEvent::PostToolUse { .. } => {
                self.status = CliAgentStatus::Running;
            }
            CliAgentEvent::PermissionRequest { summary, .. } => {
                self.status = CliAgentStatus::WaitingPermission;
                self.last_summary = Some(summary.clone());
                self.last_attention_at = Some(Instant::now());
            }
            CliAgentEvent::Notification { .. } => {
                self.status = CliAgentStatus::Idle;
                self.last_attention_at = Some(Instant::now());
            }
            CliAgentEvent::Stop { response, .. } => {
                self.status = CliAgentStatus::Done;
                self.last_summary = response.clone();
                self.last_attention_at = Some(Instant::now());
            }
        }
    }

    /// Drop the attention marker (e.g. when the user focuses this tab).
    pub fn acknowledge(&mut self) {
        self.last_attention_at = None;
    }
}

/// Loosely-typed mirror of the JSON the hook scripts emit. Every field is
/// optional because the payload is built by shell + `jq` and we never want a
/// missing/extra field to wedge the whole channel.
#[derive(Debug, Deserialize)]
struct RawPayload {
    #[serde(default)]
    v: Option<u32>,
    #[serde(default)]
    event: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    response: Option<String>,
    #[serde(default)]
    transcript_path: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

/// Parse a `piki://cli-agent` JSON body into a [`CliAgentEvent`].
///
/// Returns `None` (and logs at `debug`/`warn`) on malformed JSON, a protocol
/// version we don't understand, or an unknown event name — the caller treats
/// `None` as "ignore this OSC", so a bad payload can never wedge the stream.
pub fn parse_cli_agent_payload(json: &str) -> Option<CliAgentEvent> {
    let raw: RawPayload = match serde_json::from_str(json) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(error = %e, "cli-agent: malformed OSC 777 payload");
            return None;
        }
    };

    let v = raw.v.unwrap_or(0);
    if v == 0 || v > CLI_AGENT_PROTOCOL_VERSION {
        tracing::warn!(
            payload_version = v,
            supported = CLI_AGENT_PROTOCOL_VERSION,
            "cli-agent: unsupported protocol version, ignoring payload"
        );
        return None;
    }

    let session_id = raw.session_id;
    let event = match raw.event.as_str() {
        "session_start" => CliAgentEvent::SessionStart {
            session_id,
            cwd: non_empty(raw.cwd).map(PathBuf::from),
            project: non_empty(raw.project),
        },
        "prompt_submit" => CliAgentEvent::UserPromptSubmit { session_id },
        "tool_complete" => CliAgentEvent::PostToolUse {
            session_id,
            tool_name: non_empty(raw.tool_name),
        },
        "permission_request" => CliAgentEvent::PermissionRequest {
            session_id,
            tool_name: raw.tool_name.unwrap_or_default(),
            summary: raw.summary.unwrap_or_default(),
        },
        "notification" => CliAgentEvent::Notification {
            session_id,
            kind: raw.kind.unwrap_or_default(),
        },
        "stop" => CliAgentEvent::Stop {
            session_id,
            query: non_empty(raw.query),
            response: non_empty(raw.response),
            transcript_path: non_empty(raw.transcript_path).map(PathBuf::from),
        },
        other => {
            tracing::debug!(event = other, "cli-agent: unknown event, ignoring");
            return None;
        }
    };
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stop_event() {
        let json = r#"{"v":1,"agent":"claude","event":"stop","session_id":"abc",
            "cwd":"/tmp/p","project":"p","query":"do it","response":"done",
            "transcript_path":"/t/x.jsonl"}"#;
        let ev = parse_cli_agent_payload(json).expect("parsed");
        assert_eq!(
            ev,
            CliAgentEvent::Stop {
                session_id: "abc".into(),
                query: Some("do it".into()),
                response: Some("done".into()),
                transcript_path: Some(PathBuf::from("/t/x.jsonl")),
            }
        );
    }

    #[test]
    fn parses_permission_request() {
        let json = r#"{"v":1,"agent":"claude","event":"permission_request",
            "session_id":"s1","tool_name":"Bash","summary":"Wants to run Bash: ls"}"#;
        let ev = parse_cli_agent_payload(json).expect("parsed");
        assert_eq!(
            ev,
            CliAgentEvent::PermissionRequest {
                session_id: "s1".into(),
                tool_name: "Bash".into(),
                summary: "Wants to run Bash: ls".into(),
            }
        );
    }

    #[test]
    fn parses_lifecycle_events() {
        for (name, want) in [
            (
                "prompt_submit",
                CliAgentEvent::UserPromptSubmit {
                    session_id: "s".into(),
                },
            ),
            (
                "tool_complete",
                CliAgentEvent::PostToolUse {
                    session_id: "s".into(),
                    tool_name: None,
                },
            ),
            (
                "notification",
                CliAgentEvent::Notification {
                    session_id: "s".into(),
                    kind: String::new(),
                },
            ),
        ] {
            let json = format!(r#"{{"v":1,"event":"{name}","session_id":"s"}}"#);
            assert_eq!(parse_cli_agent_payload(&json), Some(want));
        }
    }

    #[test]
    fn rejects_unknown_event() {
        let json = r#"{"v":1,"event":"teleport","session_id":"s"}"#;
        assert!(parse_cli_agent_payload(json).is_none());
    }

    #[test]
    fn rejects_unsupported_version() {
        let too_new = r#"{"v":999,"event":"stop","session_id":"s"}"#;
        assert!(parse_cli_agent_payload(too_new).is_none());
        let missing = r#"{"event":"stop","session_id":"s"}"#;
        assert!(parse_cli_agent_payload(missing).is_none());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_cli_agent_payload("not json").is_none());
        assert!(parse_cli_agent_payload("").is_none());
    }

    #[test]
    fn json_with_semicolons_in_strings_is_fine() {
        // The OSC framing splits on ';' but the JSON body must survive
        // embedded semicolons (this is parsed *after* the framing split).
        let json = r#"{"v":1,"event":"stop","session_id":"s","response":"a; b; c"}"#;
        let ev = parse_cli_agent_payload(json).expect("parsed");
        match ev {
            CliAgentEvent::Stop { response, .. } => {
                assert_eq!(response, Some("a; b; c".into()))
            }
            _ => panic!("expected Stop"),
        }
    }

    #[test]
    fn attention_only_for_permission_notification_stop() {
        let perm = CliAgentEvent::PermissionRequest {
            session_id: "s".into(),
            tool_name: "Bash".into(),
            summary: "Wants to run Bash: ls".into(),
        };
        assert_eq!(
            perm.attention(),
            Some(("permission_request", Some("Wants to run Bash: ls")))
        );
        assert_eq!(
            CliAgentEvent::Notification {
                session_id: "s".into(),
                kind: "idle_prompt".into(),
            }
            .attention(),
            Some(("notification", None))
        );
        assert_eq!(
            CliAgentEvent::Stop {
                session_id: "s".into(),
                query: None,
                response: Some("done".into()),
                transcript_path: None,
            }
            .attention(),
            Some(("stop", Some("done")))
        );
        assert_eq!(
            CliAgentEvent::UserPromptSubmit {
                session_id: "s".into()
            }
            .attention(),
            None
        );
        assert_eq!(
            CliAgentEvent::PostToolUse {
                session_id: "s".into(),
                tool_name: None,
            }
            .attention(),
            None
        );
    }

    #[test]
    fn state_transitions_track_status_and_attention() {
        let mut s = CliAgentState::new();
        s.apply(&CliAgentEvent::UserPromptSubmit {
            session_id: "s".into(),
        });
        assert_eq!(s.status, CliAgentStatus::Running);
        assert!(s.last_attention_at.is_none());

        s.apply(&CliAgentEvent::PermissionRequest {
            session_id: "s".into(),
            tool_name: "Bash".into(),
            summary: "Wants to run Bash: ls".into(),
        });
        assert_eq!(s.status, CliAgentStatus::WaitingPermission);
        assert_eq!(s.last_summary.as_deref(), Some("Wants to run Bash: ls"));
        assert!(s.last_attention_at.is_some());

        s.acknowledge();
        assert!(s.last_attention_at.is_none());

        s.apply(&CliAgentEvent::Stop {
            session_id: "s".into(),
            query: None,
            response: Some("all done".into()),
            transcript_path: None,
        });
        assert_eq!(s.status, CliAgentStatus::Done);
        assert_eq!(s.last_summary.as_deref(), Some("all done"));
        assert_eq!(s.session_id.as_deref(), Some("s"));
    }
}
