//! Passive agent-state detection for providers with no hook bridge (e.g.
//! Codex): read the provider's own OSC window-title convention and known
//! blocking-prompt text off the screen instead of an in-band protocol.
//! Mirrors `herdr`'s manifest approach, sized down to a static table since
//! Codex is the only entry today.

use crate::cli_agent::CliAgentStatus;

pub struct StateManifest {
    pub working_title_chars: &'static [char],
    pub blocked_needles: &'static [&'static str],
}

const CODEX: StateManifest = StateManifest {
    working_title_chars: &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
    blocked_needles: &[
        "press enter to confirm",
        "esc to cancel",
        "enter to submit answer",
        "allow command?",
        "[y/n]",
        "action required",
    ],
};

/// Basename-match a provider's command against a known passive-detection
/// manifest, mirroring `cli_agent::bridge_for_command`'s matching style.
pub fn manifest_for_command(command: &str) -> Option<&'static StateManifest> {
    let bin = std::path::Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command);
    match bin {
        "codex" => Some(&CODEX),
        _ => None,
    }
}

/// Classify a tab's status from its window title and a screen-tail sample.
/// `None` means no signal yet — callers must not overwrite existing state
/// with a guess.
pub fn detect(
    manifest: &StateManifest,
    title: Option<&str>,
    screen_tail: &str,
) -> Option<CliAgentStatus> {
    let title_lower = title.map(|t| t.to_lowercase());
    if let Some(ref t) = title_lower
        && t.chars().any(|c| manifest.working_title_chars.contains(&c))
    {
        return Some(CliAgentStatus::Running);
    }

    let tail_lower = screen_tail.to_lowercase();
    let blocked = manifest
        .blocked_needles
        .iter()
        .any(|n| tail_lower.contains(n) || title_lower.as_deref().is_some_and(|t| t.contains(n)));
    if blocked {
        return Some(CliAgentStatus::WaitingPermission);
    }

    if let Some(t) = title_lower
        && !t.trim().is_empty()
    {
        return Some(CliAgentStatus::Idle);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_title_is_running() {
        let status = detect(&CODEX, Some("⠋ Thinking"), "");
        assert_eq!(status, Some(CliAgentStatus::Running));
    }

    #[test]
    fn blocked_needle_in_screen_text_is_waiting_permission() {
        let status = detect(&CODEX, Some("codex"), "some output\nAllow command?\n");
        assert_eq!(status, Some(CliAgentStatus::WaitingPermission));
    }

    #[test]
    fn blocked_needle_in_title_is_waiting_permission() {
        let status = detect(&CODEX, Some("codex - [y/n]"), "");
        assert_eq!(status, Some(CliAgentStatus::WaitingPermission));
    }

    #[test]
    fn plain_title_with_no_signal_is_idle() {
        let status = detect(&CODEX, Some("codex"), "nothing interesting here");
        assert_eq!(status, Some(CliAgentStatus::Idle));
    }

    #[test]
    fn no_title_and_no_blocker_is_none() {
        let status = detect(&CODEX, None, "nothing interesting here");
        assert_eq!(status, None);
    }

    #[test]
    fn manifest_for_command_matches_by_basename() {
        assert!(manifest_for_command("codex").is_some());
        assert!(manifest_for_command("/usr/local/bin/codex").is_some());
        assert!(manifest_for_command("bash").is_none());
    }
}
