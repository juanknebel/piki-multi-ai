//! OS-level notification helpers shared by the TUI and desktop binaries.
//!
//! Each helper spawns a detached thread that fires the notification via
//! `notify-rust` and logs (but never propagates) failures — a missing
//! notification daemon should never abort the calling code path.
//!
//! ## Appname
//!
//! Notifications carry an `appname` so the OS can group them per source.
//! Each binary calls [`set_appname`] once at startup with its own name
//! (`piki-multi-ai` for the TUI, `piki-desktop` for the desktop). Callers
//! that don't initialize fall back to a generic `"piki-multi"` — useful
//! for unit tests.

use std::sync::OnceLock;

static APPNAME: OnceLock<&'static str> = OnceLock::new();

/// Set the OS notification source name. Idempotent — only the first call
/// takes effect, so each binary should call this once during startup before
/// any notification is spawned.
pub fn set_appname(name: &'static str) {
    let _ = APPNAME.set(name);
}

fn appname() -> &'static str {
    APPNAME.get().copied().unwrap_or("piki-multi")
}

/// Fire-and-forget notification for a provider tab (`AIProvider::Custom(_)`)
/// whose `IdleWatcher` reports the PTY has been silent past its threshold.
pub fn notify_agent_idle(workspace_name: &str, agent_label: &str) {
    let summary = format!("Agent idle: {agent_label}");
    let body = format!("{workspace_name} — {agent_label} stopped producing output");
    spawn(summary, body);
}

/// Fire-and-forget notification for a shell tab whose OSC 133 `command-end`
/// marker just arrived. Body includes the exit code when known.
pub fn notify_command_end(workspace_name: &str, exit_code: Option<i32>) {
    let (summary, body) = match exit_code {
        Some(0) => (
            "Command finished".to_string(),
            format!("{workspace_name} — exit 0"),
        ),
        Some(code) => (
            format!("Command failed (exit {code})"),
            format!("{workspace_name} — exit {code}"),
        ),
        None => (
            "Command finished".to_string(),
            format!("{workspace_name} — exit unknown"),
        ),
    };
    spawn(summary, body);
}

fn spawn(summary: String, body: String) {
    let app = appname();
    std::thread::spawn(move || {
        if let Err(e) = notify_rust::Notification::new()
            .summary(&summary)
            .body(&body)
            .appname(app)
            .show()
        {
            tracing::warn!("OS notification failed: {e}");
        }
    });
}
