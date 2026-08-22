//! Passive agent-state detection for providers with no hook bridge (Codex,
//! Muse): read the provider's own OSC window-title convention and known
//! blocking-prompt text off the screen instead of an in-band protocol.
//! Mirrors `herdr`'s manifest approach, sized down to a static table.

use crate::cli_agent::CliAgentStatus;

pub struct StateManifest {
    pub working_title_chars: &'static [char],
    pub blocked_needles: &'static [&'static str],
}

const BRAILLE_SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const CODEX: StateManifest = StateManifest {
    working_title_chars: BRAILLE_SPINNER,
    blocked_needles: &[
        "press enter to confirm",
        "esc to cancel",
        "enter to submit answer",
        "allow command?",
        "[y/n]",
        "action required",
    ],
};

// Muse keeps the braille spinner in its window title even while an approval
// prompt is up ("Calling tools" spins behind the dialog), so classification
// relies on `detect()` checking blocked needles before the spinner. Needles
// verified against Muse Code 0.1.0: the tool-approval dialog and the
// workspace-trust dialog, both rendered in the bottom screen rows.
const MUSE: StateManifest = StateManifest {
    working_title_chars: BRAILLE_SPINNER,
    blocked_needles: &[
        "would you like to run the following command",
        "allow this stage once",
        "always allow in this workspace",
        "do you trust this workspace",
        "press enter to confirm",
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
        "muse" => Some(&MUSE),
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
    // Blocked outranks the spinner (same precedence as `status_severity`):
    // Muse keeps its title spinner running behind an approval dialog, so
    // checking the spinner first would report Running while the agent is
    // actually stuck waiting for the user.
    let title_lower = title.map(|t| t.to_lowercase());
    let tail_lower = screen_tail.to_lowercase();
    let blocked = manifest
        .blocked_needles
        .iter()
        .any(|n| tail_lower.contains(n) || title_lower.as_deref().is_some_and(|t| t.contains(n)));
    if blocked {
        return Some(CliAgentStatus::WaitingPermission);
    }

    if let Some(ref t) = title_lower
        && t.chars().any(|c| manifest.working_title_chars.contains(&c))
    {
        return Some(CliAgentStatus::Running);
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
        assert!(manifest_for_command("muse").is_some());
        assert!(manifest_for_command("/home/u/.local/bin/muse").is_some());
        assert!(manifest_for_command("bash").is_none());
    }

    // Muse-specific fixtures, captured from Muse Code 0.1.0.

    #[test]
    fn muse_spinner_title_is_running() {
        let status = detect(
            &MUSE,
            Some("⠹ zero"),
            "⟩ run ls\n◇ Working (1s · esc to interrupt)",
        );
        assert_eq!(status, Some(CliAgentStatus::Running));
    }

    #[test]
    fn muse_approval_dialog_wins_over_title_spinner() {
        // Muse keeps the title spinner going behind the approval dialog.
        let tail = "Would you like to run the following command?\n$ ls\n› 1. Allow this stage once (y)\nPress enter to confirm or esc to cancel";
        let status = detect(&MUSE, Some("⠼ zero"), tail);
        assert_eq!(status, Some(CliAgentStatus::WaitingPermission));
    }

    #[test]
    fn muse_workspace_trust_dialog_is_waiting_permission() {
        let tail = "Do you trust this workspace?\n> 1  Trust and continue\n  2  Quit";
        let status = detect(&MUSE, Some("zero"), tail);
        assert_eq!(status, Some(CliAgentStatus::WaitingPermission));
    }

    #[test]
    fn muse_plain_title_is_idle() {
        let status = detect(&MUSE, Some("zero"), "⟩ \nVoice input (Alt + v to start)");
        assert_eq!(status, Some(CliAgentStatus::Idle));
    }

    #[test]
    fn muse_working_footer_is_not_a_blocked_needle() {
        // "esc to interrupt" in the normal working footer must not read as
        // blocked ("esc to cancel" is a Codex needle, not a Muse one).
        let status = detect(
            &MUSE,
            Some("⠧ zero"),
            "◆ Calling tools (4s · esc to interrupt)",
        );
        assert_eq!(status, Some(CliAgentStatus::Running));
    }
}
