//! OS-level notification helpers shared by the TUI and desktop binaries,
//! plus an in-process mailbox that de-duplicates events by origin so the
//! same logical session can't pile up stale entries.
//!
//! Design lifted from Warp (`app/src/ai/agent_management/notifications/`):
//! a new event for the same `origin` *replaces* the previous one in the
//! mailbox instead of stacking. Callers compose the `origin` however they
//! want — typically a per-tab identifier — and the latest event "wins".
//!
//! Each helper still fires a detached OS toast via `notify-rust`; failures
//! are logged (tracing) but never propagated — a missing notification
//! daemon should never abort the calling code path.
//!
//! ## Appname
//!
//! Notifications carry an `appname` so the OS can group them per source.
//! Each binary calls [`set_appname`] once at startup with its own name
//! (`piki-multi-ai` for the TUI, `piki-desktop` for the desktop). Callers
//! that don't initialize fall back to a generic `"piki-multi"`.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Maximum entries kept in the in-process mailbox before old ones drop.
/// Matches Warp's mailbox cap (`app/src/ai/agent_management/notifications/item.rs`).
pub const MAILBOX_CAPACITY: usize = 100;

static APPNAME: OnceLock<&'static str> = OnceLock::new();
static MAILBOX: OnceLock<Mutex<NotificationMailbox>> = OnceLock::new();

/// Whether the host window/terminal currently has OS focus. When `true`,
/// [`push_and_toast`] skips the call to `notify-rust` (the in-app toast
/// already covers the event). Default is `false` so frontends/terminals
/// that don't wire focus tracking — or terminals that don't emit focus
/// events (CSI ? 1004) — still notify like today: no regression.
static WINDOW_HAS_FOCUS: AtomicBool = AtomicBool::new(false);

/// Set the OS notification source name. Idempotent — only the first call
/// takes effect, so each binary should call this once during startup before
/// any notification is spawned.
pub fn set_appname(name: &'static str) {
    let _ = APPNAME.set(name);
}

/// Update the OS-focus state of the host window/terminal. Frontends call
/// this on focus-changed events (crossterm `FocusGained`/`FocusLost` in
/// the TUI, Tauri `WindowEvent::Focused` in the desktop). When `true`,
/// future OS notifications are suppressed (mailbox entries still record).
pub fn set_window_focused(focused: bool) {
    WINDOW_HAS_FOCUS.store(focused, Ordering::Relaxed);
}

fn appname() -> &'static str {
    APPNAME.get().copied().unwrap_or("piki-multi")
}

fn mailbox() -> &'static Mutex<NotificationMailbox> {
    MAILBOX.get_or_init(|| Mutex::new(NotificationMailbox::default()))
}

/// Coarse classification used for filtering and (eventually) styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationCategory {
    /// Long-running task finished cleanly — shell exit 0, or an agent
    /// went idle after a meaningful burst of output.
    Complete,
    /// A failure path — shell exited non-zero. (Reserved for future
    /// agent-crash detection.)
    Error,
}

/// A single notification entry. Cloneable so it can be both pushed to the
/// mailbox and handed to the OS-toast spawner without a borrow tangle.
#[derive(Debug, Clone)]
pub struct NotificationItem {
    /// De-duplication key — typically a per-tab identifier composed by the
    /// caller (e.g. a UUID string in the desktop, `format!("{ws}#{tab}")`
    /// in the TUI). A `push()` of a new item with the same `origin`
    /// removes any previous entry from the mailbox.
    pub origin: String,
    /// Workspace name for display in the OS toast body.
    pub workspace: String,
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    pub created_at: Instant,
}

/// Bounded, replace-by-origin mailbox of recent notifications.
///
/// Used today only as a server-side de-dup gate for the OS toast spawner —
/// a future in-app history panel can read `snapshot()` and render it.
#[derive(Debug, Default)]
pub struct NotificationMailbox {
    items: Vec<NotificationItem>,
}

impl NotificationMailbox {
    /// Replace any existing entry with the same `origin`, then insert at
    /// the front. Truncated to [`MAILBOX_CAPACITY`].
    pub fn push(&mut self, item: NotificationItem) {
        self.items.retain(|i| i.origin != item.origin);
        self.items.insert(0, item);
        self.items.truncate(MAILBOX_CAPACITY);
    }

    pub fn snapshot(&self) -> Vec<NotificationItem> {
        self.items.clone()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// Snapshot of the current mailbox. For future in-app history UIs.
pub fn mailbox_snapshot() -> Vec<NotificationItem> {
    mailbox().lock().snapshot()
}

/// Reset the mailbox to empty. Test-only — exposed because integration
/// tests in other crates may want a clean slate.
#[doc(hidden)]
pub fn _reset_mailbox_for_test() {
    mailbox().lock().clear();
}

/// Notification for a provider tab (`AIProvider::Custom(_)`) whose
/// `IdleWatcher` reports the PTY has been silent past its threshold.
/// Origin should uniquely identify the tab so repeated idle events from
/// the same agent replace each other instead of stacking.
///
/// `silent_for` is the duration the PTY was silent before the watcher
/// fired (lifted from `IdleSignal::silent_for`) — included in the body
/// as a hint of how long the agent has been quiet. `icon` (if any) is
/// prepended to the title; typically sourced from
/// `ProviderConfig.icon`.
pub fn notify_agent_idle(
    origin: &str,
    workspace_name: &str,
    agent_label: &str,
    silent_for: Duration,
    icon: Option<&str>,
    from_active_view: bool,
) {
    let icon_prefix = icon.map(|i| format!("{i} ")).unwrap_or_default();
    let idle_secs = silent_for.as_secs().max(1);
    let item = NotificationItem {
        origin: origin.to_string(),
        workspace: workspace_name.to_string(),
        category: NotificationCategory::Complete,
        title: format!("{icon_prefix}Agent idle: {agent_label}"),
        body: format!(
            "{workspace_name} — {agent_label} finished the task (idle {idle_secs}s)"
        ),
        created_at: Instant::now(),
    };
    push_and_toast(item, from_active_view);
}

/// Notification for a shell tab whose OSC 133 `command-end` marker just
/// arrived. Non-zero exit codes are tagged [`NotificationCategory::Error`].
/// `command` is the user-typed text captured by the OSC parser (may be
/// `None` if capture was empty / disabled); when present, it's quoted in
/// the body so the user can tell which command just finished.
pub fn notify_command_end(
    origin: &str,
    workspace_name: &str,
    exit_code: Option<i32>,
    command: Option<&str>,
    from_active_view: bool,
) {
    let cmd_suffix = command
        .map(|c| format!(" `{c}`"))
        .unwrap_or_default();
    let (category, title, body) = match exit_code {
        Some(0) => (
            NotificationCategory::Complete,
            "Command finished".to_string(),
            format!("{workspace_name} — exit 0{cmd_suffix}"),
        ),
        Some(code) => (
            NotificationCategory::Error,
            format!("Command failed (exit {code})"),
            format!("{workspace_name} — exit {code}{cmd_suffix}"),
        ),
        None => (
            NotificationCategory::Complete,
            "Command finished".to_string(),
            format!("{workspace_name} — exit unknown{cmd_suffix}"),
        ),
    };
    let item = NotificationItem {
        origin: origin.to_string(),
        workspace: workspace_name.to_string(),
        category,
        title,
        body,
        created_at: Instant::now(),
    };
    push_and_toast(item, from_active_view);
}

/// Notification for a structured Claude Code lifecycle event (Warp-style,
/// delivered in-band via OSC 777). `kind` is the cli-agent event name
/// (`permission_request`, `notification`, `stop`); other kinds are
/// informational-only and don't notify. `summary` is the hook-built
/// one-liner (permission preview, or the agent's final response preview).
/// `icon` (if any) is prepended to the title, mirroring `notify_agent_idle`.
pub fn notify_cli_agent(
    origin: &str,
    workspace_name: &str,
    kind: &str,
    summary: Option<&str>,
    icon: Option<&str>,
    from_active_view: bool,
) {
    let icon_prefix = icon.map(|i| format!("{i} ")).unwrap_or_default();
    let detail = summary
        .filter(|s| !s.is_empty())
        .map(|s| format!(" — {s}"))
        .unwrap_or_default();
    let (category, title, body) = match kind {
        "permission_request" => (
            NotificationCategory::Complete,
            format!("{icon_prefix}Permission needed"),
            format!("{workspace_name}{detail}"),
        ),
        "notification" => (
            NotificationCategory::Complete,
            format!("{icon_prefix}Agent waiting for input"),
            format!("{workspace_name} — Claude has been idle and needs you"),
        ),
        "stop" => (
            NotificationCategory::Complete,
            format!("{icon_prefix}Task complete"),
            format!("{workspace_name}{detail}"),
        ),
        _ => return,
    };
    let item = NotificationItem {
        origin: origin.to_string(),
        workspace: workspace_name.to_string(),
        category,
        title,
        body,
        created_at: Instant::now(),
    };
    push_and_toast(item, from_active_view);
}

/// `visible` means the event's tab is the one the user is currently looking
/// at (active workspace + active tab). The mailbox always records; the OS
/// toast is suppressed **only** when the event is both `visible` *and* the
/// piki window has OS focus — i.e. the user would already see it. A
/// background-tab event still toasts even while piki is focused, since the
/// user can't see a tab they aren't on (this is the whole point of the
/// active-tab gate; window focus alone is too coarse).
fn push_and_toast(item: NotificationItem, visible: bool) {
    {
        let mut mb = mailbox().lock();
        mb.push(item.clone());
    }
    let already_seen = visible && WINDOW_HAS_FOCUS.load(Ordering::Relaxed);
    if !already_seen {
        spawn_toast(item.title, item.body);
    }
}

fn spawn_toast(summary: String, body: String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_item(origin: &str, title: &str) -> NotificationItem {
        NotificationItem {
            origin: origin.to_string(),
            workspace: "ws".to_string(),
            category: NotificationCategory::Complete,
            title: title.to_string(),
            body: "body".to_string(),
            created_at: Instant::now(),
        }
    }

    #[test]
    fn push_inserts_at_front() {
        let mut mb = NotificationMailbox::default();
        mb.push(fresh_item("a", "first"));
        mb.push(fresh_item("b", "second"));
        let snap = mb.snapshot();
        assert_eq!(snap[0].title, "second");
        assert_eq!(snap[1].title, "first");
    }

    #[test]
    fn push_with_same_origin_replaces_previous() {
        let mut mb = NotificationMailbox::default();
        mb.push(fresh_item("tab-1", "old"));
        mb.push(fresh_item("tab-2", "other"));
        mb.push(fresh_item("tab-1", "new"));
        let snap = mb.snapshot();
        assert_eq!(snap.len(), 2);
        // New "tab-1" entry sits in front; the old one is gone.
        assert_eq!(snap[0].title, "new");
        assert_eq!(snap[1].title, "other");
        // No two items share the same origin.
        assert_eq!(
            snap.iter().filter(|i| i.origin == "tab-1").count(),
            1
        );
    }

    #[test]
    fn push_truncates_to_capacity() {
        let mut mb = NotificationMailbox::default();
        for i in 0..MAILBOX_CAPACITY + 10 {
            mb.push(fresh_item(&format!("origin-{i}"), &format!("item-{i}")));
        }
        assert_eq!(mb.len(), MAILBOX_CAPACITY);
        // Newest survives, oldest dropped.
        let snap = mb.snapshot();
        assert_eq!(snap[0].title, format!("item-{}", MAILBOX_CAPACITY + 9));
    }

    #[test]
    fn snapshot_returns_clone_not_alias() {
        let mut mb = NotificationMailbox::default();
        mb.push(fresh_item("a", "alpha"));
        let snap = mb.snapshot();
        mb.clear();
        // Snapshot survives clear because it's a deep copy.
        assert_eq!(snap.len(), 1);
        assert!(mb.is_empty());
    }

    // The two focus-gate cases share global statics (`WINDOW_HAS_FOCUS`,
    // `MAILBOX`), so they must run as a single test under cargo's default
    // test-thread parallelism. Splitting them caused a race where one
    // test's `_reset_mailbox_for_test()` would land between the other's
    // push and assert.
    #[test]
    fn push_and_toast_records_to_mailbox_in_both_focus_states() {
        // Focused: mailbox still records; OS toast is suppressed (side-effect
        // we cannot observe in-process — covered by the conditional in
        // push_and_toast).
        _reset_mailbox_for_test();
        set_window_focused(true);
        // visible=true + focused → OS toast suppressed; mailbox still records.
        push_and_toast(fresh_item("tab-focus-on", "idle"), true);
        assert_eq!(mailbox_snapshot().len(), 1);

        // Unfocused: mailbox records and toast would fire (visibility moot).
        _reset_mailbox_for_test();
        set_window_focused(false);
        push_and_toast(fresh_item("tab-focus-off", "idle"), true);
        assert_eq!(mailbox_snapshot().len(), 1);

        _reset_mailbox_for_test();
    }
}
