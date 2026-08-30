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
//! The event types, JSON parsing, and the per-tab FIFO transport
//! (`piki_multiplex::cli_agent`, re-exported below) live in `piki-multiplex`
//! next to the PTY/session code that consumes them. This module keeps only
//! what's genuinely piki-specific: which binary bridges to which agent
//! ([`bridge_for_command`]) and how to install each agent's hooks
//! ([`install`], [`install_antigravity`]).

pub mod install;
pub mod install_antigravity;

#[cfg(unix)]
pub use piki_multiplex::cli_agent::sock;
pub use piki_multiplex::cli_agent::{
    CLI_AGENT_PROTOCOL_VERSION, CLI_AGENT_TARGET, CliAgentEvent, CliAgentState, CliAgentStatus,
    format_elapsed, parse_cli_agent_payload, status_severity,
};

/// Which CLI agent's hook protocol a provider tab speaks, if any.
///
/// Both bridges land on the same [`CliAgentEvent`] stream and the same per-tab
/// FIFO; they differ only in how the hooks get installed (Claude Code takes a
/// per-spawn `--settings` file, Antigravity needs a plugin in its own config
/// root — see [`install_antigravity`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBridge {
    Claude,
    Antigravity,
}

impl AgentBridge {
    /// Human-readable agent name, for warnings the user reads.
    pub fn label(self) -> &'static str {
        match self {
            AgentBridge::Claude => "Claude Code",
            AgentBridge::Antigravity => "Antigravity",
        }
    }
}

/// Pick the bridge for a provider's command, matching on the binary name so a
/// `command` of `claude`, `/usr/local/bin/claude` or `agy` all resolve.
/// `None` means "no structured integration" — the tab spawns bare and the
/// heuristic [`crate::idle_watcher::IdleWatcher`] stays as the fallback.
pub fn bridge_for_command(command: &str) -> Option<AgentBridge> {
    let bin = std::path::Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command);
    match bin {
        "claude" => Some(AgentBridge::Claude),
        "agy" | "antigravity" => Some(AgentBridge::Antigravity),
        _ => None,
    }
}

/// External tools a bridge needs that are missing from the user's PATH.
///
/// Empty means the tab will get the structured channel. Otherwise the tab still
/// spawns and works — it just falls back to the byte-silence idle heuristic, so
/// the Agents pane shows bare liveness instead of running/idle/done. Both hook
/// bridges build their JSON payloads with `jq`, so that's the whole list today;
/// frontends warn on a non-empty result rather than blocking the spawn.
pub fn missing_prerequisites(bridge: AgentBridge) -> Vec<&'static str> {
    let _ = bridge;
    if install::jq_available() {
        Vec::new()
    } else {
        vec!["jq"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_resolves_by_binary_name() {
        assert_eq!(bridge_for_command("claude"), Some(AgentBridge::Claude));
        assert_eq!(
            bridge_for_command("/usr/local/bin/claude"),
            Some(AgentBridge::Claude)
        );
        assert_eq!(bridge_for_command("agy"), Some(AgentBridge::Antigravity));
        assert_eq!(
            bridge_for_command("/home/u/.local/bin/agy"),
            Some(AgentBridge::Antigravity)
        );
        assert_eq!(
            bridge_for_command("antigravity"),
            Some(AgentBridge::Antigravity)
        );
        assert_eq!(bridge_for_command("gemini"), None);
        assert_eq!(bridge_for_command(""), None);
    }
}
