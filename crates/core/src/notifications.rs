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
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// How notifications reach the user when they aren't looking at the event's
/// tab (herdr-style selector). The in-process mailbox always records
/// regardless; sound is a separate, independent layer (see [`crate::sound`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationDelivery {
    /// Mailbox only — no visible notification.
    Off,
    /// OS desktop notification via `notify-rust` (the historical behavior).
    #[default]
    System,
    /// OSC 9 escape written to `/dev/tty` so the *outer* terminal emulator
    /// (kitty, ghostty, …) shows its native notification. Useful inside
    /// tmux/ssh where a desktop toast can't reach the user; the sequence is
    /// tmux-passthrough-wrapped when `$TMUX` is set.
    Terminal,
}

static DELIVERY: AtomicU8 = AtomicU8::new(1); // System

/// Select the toast delivery mode. Frontends call this at startup from
/// their config; the default is [`NotificationDelivery::System`].
pub fn set_delivery(d: NotificationDelivery) {
    let v = match d {
        NotificationDelivery::Off => 0,
        NotificationDelivery::System => 1,
        NotificationDelivery::Terminal => 2,
    };
    DELIVERY.store(v, Ordering::Relaxed);
}

fn delivery() -> NotificationDelivery {
    match DELIVERY.load(Ordering::Relaxed) {
        0 => NotificationDelivery::Off,
        2 => NotificationDelivery::Terminal,
        _ => NotificationDelivery::System,
    }
}

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
    push_and_toast(item, from_active_view, Some(crate::sound::Sound::Done));
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
    // No sound for plain shell commands — chimes are agent events only.
    push_and_toast(item, from_active_view, None);
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
    let (category, title, body, sound) = match kind {
        "permission_request" => (
            NotificationCategory::Complete,
            format!("{icon_prefix}Permission needed"),
            format!("{workspace_name}{detail}"),
            crate::sound::Sound::Attention,
        ),
        "notification" => (
            NotificationCategory::Complete,
            format!("{icon_prefix}Agent waiting for input"),
            format!("{workspace_name} — Claude has been idle and needs you"),
            crate::sound::Sound::Attention,
        ),
        "stop" => (
            NotificationCategory::Complete,
            format!("{icon_prefix}Task complete"),
            format!("{workspace_name}{detail}"),
            crate::sound::Sound::Done,
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
    push_and_toast(item, from_active_view, Some(sound));
}

/// `visible` means the event's tab is the one the user is currently looking
/// at (active workspace + active tab). The mailbox always records; the OS
/// toast is suppressed **only** when the event is both `visible` *and* the
/// piki window has OS focus — i.e. the user would already see it. A
/// background-tab event still toasts even while piki is focused, since the
/// user can't see a tab they aren't on (this is the whole point of the
/// active-tab gate; window focus alone is too coarse).
fn push_and_toast(item: NotificationItem, visible: bool, sound: Option<crate::sound::Sound>) {
    {
        let mut mb = mailbox().lock();
        mb.push(item.clone());
    }
    let already_seen = visible && WINDOW_HAS_FOCUS.load(Ordering::Relaxed);
    if already_seen {
        return;
    }
    // Sound is independent from the toast delivery mode — it plays (when
    // enabled in config) even with delivery = off, mirroring herdr.
    if let Some(s) = sound {
        crate::sound::play(s);
    }
    match delivery() {
        NotificationDelivery::Off => {}
        NotificationDelivery::System => spawn_toast(item.title, item.body),
        NotificationDelivery::Terminal => emit_terminal_notification(&item.title, &item.body),
    }
}

/// Emit an OSC 9 notification escape to `/dev/tty` so the host terminal
/// emulator shows it natively. Called from the frontend's event-loop thread,
/// so the write is serialized with rendering (no tearing against ratatui's
/// output). Wrapped in a tmux passthrough when running inside tmux.
fn emit_terminal_notification(title: &str, body: &str) {
    #[cfg(unix)]
    {
        use std::io::Write;
        let msg = if body.is_empty() {
            sanitize_osc(title)
        } else {
            format!("{}: {}", sanitize_osc(title), sanitize_osc(body))
        };
        let seq = format!("\x1b]9;{msg}\x1b\\");
        let bytes = if std::env::var_os("TMUX").is_some() {
            wrap_tmux_passthrough(seq.as_bytes())
        } else {
            seq.into_bytes()
        };
        match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            Ok(mut tty) => {
                let _ = tty.write_all(&bytes);
            }
            Err(e) => tracing::warn!(error = %e, "terminal notification failed: /dev/tty not writable"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (title, body);
    }
}

/// Strip control characters so a title/body can't terminate or corrupt the
/// OSC sequence (ESC, BEL, newlines all collapse to spaces).
fn sanitize_osc(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Wrap an escape sequence in tmux's passthrough envelope
/// (`ESC P tmux; <seq with ESC doubled> ESC \`) so tmux forwards it to the
/// outer terminal instead of swallowing it.
fn wrap_tmux_passthrough(seq: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(seq.len() + 8);
    out.extend_from_slice(b"\x1bPtmux;");
    for &b in seq {
        if b == 0x1b {
            out.push(0x1b);
        }
        out.push(b);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

fn spawn_toast(summary: String, body: String) {
    let app = appname();
    std::thread::spawn(move || {
        // We only reach here when the user is NOT looking at the event's tab
        // (push_and_toast already gated on active-view + focus).
        let mut notification = notify_rust::Notification::new();
        notification.summary(&summary).body(&body).appname(app);
        // Mark it Critical so the notification daemon doesn't auto-suppress
        // it just because piki happens to be the focused window — a
        // documented freedesktop behaviour for normal-urgency notifications.
        // `urgency` is an XDG/freedesktop concept; the API doesn't exist on
        // macOS (NSUserNotification has no urgency), so gate it to Linux.
        #[cfg(target_os = "linux")]
        notification.urgency(notify_rust::Urgency::Critical);
        if let Err(e) = notification.show() {
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
        push_and_toast(fresh_item("tab-focus-on", "idle"), true, None);
        assert_eq!(mailbox_snapshot().len(), 1);

        // Unfocused: mailbox records and toast would fire (visibility moot).
        _reset_mailbox_for_test();
        set_window_focused(false);
        push_and_toast(fresh_item("tab-focus-off", "idle"), true, None);
        assert_eq!(mailbox_snapshot().len(), 1);

        _reset_mailbox_for_test();
    }

    #[test]
    fn tmux_passthrough_doubles_escapes_and_wraps() {
        let wrapped = wrap_tmux_passthrough(b"\x1b]9;hi\x1b\\");
        assert_eq!(wrapped, b"\x1bPtmux;\x1b\x1b]9;hi\x1b\x1b\\\x1b\\");
    }

    #[test]
    fn sanitize_osc_strips_control_chars() {
        assert_eq!(sanitize_osc("a\x1b]bad\x07\ntitle"), "a ]bad  title");
    }

    #[test]
    fn delivery_roundtrips_through_setter() {
        for d in [
            NotificationDelivery::Off,
            NotificationDelivery::Terminal,
            NotificationDelivery::System,
        ] {
            set_delivery(d);
            assert_eq!(delivery(), d);
        }
    }
}
