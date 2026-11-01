//! Wire vocabulary for the `open` UI action: "open this in your own UI",
//! sent by an external tool running inside a tab's PTY (`piki-ctl` today;
//! any agent-agnostic tool tomorrow) rather than by the coding agent's own
//! hook system. Unlike the rest of [`super`] — which observes an agent's
//! *lifecycle* (idle/running/done) through its native hooks — this is an
//! imperative, one-shot request that works the same from any tab: a bare
//! shell, Claude Code, Codex, muse-cli, anything that can run a command.
//!
//! It still rides the same `piki://cli-agent` OSC 777 target as the
//! lifecycle channel (today's `OscParser` only matches one target per
//! daemon — see `piki-multiplex`), hence the `CliAgentEvent::Open` variant
//! in [`super`]. This module is the single source of truth for that
//! sub-protocol's shape so the emitter (`piki-ctl`) and the parser
//! (`super::parse_cli_agent_payload`) can't drift apart.
//!
//! ```text
//! ESC ] 777 ; notify ; piki://cli-agent ; {"v":1,"event":"open","target_kind":"file","path":"/abs/ble.md"} BEL
//! ```

use super::CLI_AGENT_TARGET;

/// `event` value for an open action.
pub const OPEN_EVENT: &str = "open";

/// Known `target_kind` values. Deliberately plain strings on the wire, not a
/// closed enum — a newer `piki-ctl` sending a kind this build doesn't
/// recognize should just be ignored by the frontend, not fail to parse.
pub mod target_kind {
    /// A file to preview (markdown rendered, anything else shown as text/code).
    pub const FILE: &str = "file";
}

/// Build the full in-band OSC 777 escape sequence for an `open` action,
/// ready to write to the invoking tab's own stdout — which *is* the PTY, so
/// piki's OSC parser (active on every tab regardless of agent bridge) sniffs
/// it out of the byte stream. No FIFO, no per-agent hook install, no
/// `jq`: the whole point is that this works identically from any tab.
pub fn build_open_osc(target_kind: &str, path: &str) -> String {
    let body = serde_json::json!({
        "v": 1,
        "event": OPEN_EVENT,
        "target_kind": target_kind,
        "path": path,
    });
    format!("\x1b]777;notify;{CLI_AGENT_TARGET};{body}\x07")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_agent::{CliAgentEvent, parse_cli_agent_payload};

    #[test]
    fn built_osc_round_trips_through_the_parser() {
        let osc = build_open_osc(target_kind::FILE, "/abs/ble.md");
        // Strip the OSC 777 framing the way the real OscParser would, leaving
        // just the JSON body this module's own parser branch consumes.
        let prefix = format!("\x1b]777;notify;{CLI_AGENT_TARGET};");
        let json = osc
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix('\x07'))
            .expect("expected OSC 777 framing");

        let ev = parse_cli_agent_payload(json).expect("parses");
        assert_eq!(
            ev,
            CliAgentEvent::Open {
                session_id: String::new(),
                target_kind: target_kind::FILE.to_string(),
                path: "/abs/ble.md".to_string(),
            }
        );
    }
}
