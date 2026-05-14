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
use std::time::Instant;

use parking_lot::Mutex;

/// Maximum entries kept in the in-process mailbox before old ones drop.
/// Matches Warp's mailbox cap (`app/src/ai/agent_management/notifications/item.rs`).
pub const MAILBOX_CAPACITY: usize = 100;

static APPNAME: OnceLock<&'static str> = OnceLock::new();
static MAILBOX: OnceLock<Mutex<NotificationMailbox>> = OnceLock::new();

/// Set the OS notification source name. Idempotent — only the first call
/// takes effect, so each binary should call this once during startup before
/// any notification is spawned.
pub fn set_appname(name: &'static str) {
    let _ = APPNAME.set(name);
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
pub fn notify_agent_idle(origin: &str, workspace_name: &str, agent_label: &str) {
    let item = NotificationItem {
        origin: origin.to_string(),
        workspace: workspace_name.to_string(),
        category: NotificationCategory::Complete,
        title: format!("Agent idle: {agent_label}"),
        body: format!("{workspace_name} — {agent_label} stopped producing output"),
        created_at: Instant::now(),
    };
    push_and_toast(item);
}

/// Notification for a shell tab whose OSC 133 `command-end` marker just
/// arrived. Non-zero exit codes are tagged [`NotificationCategory::Error`].
pub fn notify_command_end(origin: &str, workspace_name: &str, exit_code: Option<i32>) {
    let (category, title, body) = match exit_code {
        Some(0) => (
            NotificationCategory::Complete,
            "Command finished".to_string(),
            format!("{workspace_name} — exit 0"),
        ),
        Some(code) => (
            NotificationCategory::Error,
            format!("Command failed (exit {code})"),
            format!("{workspace_name} — exit {code}"),
        ),
        None => (
            NotificationCategory::Complete,
            "Command finished".to_string(),
            format!("{workspace_name} — exit unknown"),
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
    push_and_toast(item);
}

fn push_and_toast(item: NotificationItem) {
    {
        let mut mb = mailbox().lock();
        mb.push(item.clone());
    }
    spawn_toast(item.title, item.body);
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
}
