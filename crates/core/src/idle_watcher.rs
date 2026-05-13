//! Per-tab idle detector for provider tabs (Claude/Gemini/etc.).
//!
//! Provider tabs run their binary directly with no shell wrapper, so OSC 133
//! markers don't apply — we can't know precisely when an agent finished
//! responding. Instead we watch the tab's PTY byte counter: if no new bytes
//! appear for `threshold` seconds *after* the tab has produced at least some
//! output, we report the tab as idle.
//!
//! This replaces the inline heuristic that used to live in the TUI's
//! event-loop. Centralizing it here lets both TUI and desktop share the same
//! logic, and the threshold can be configured per-provider via
//! `providers.toml`.

use std::time::{Duration, Instant};

/// Default idle threshold when a provider config doesn't specify one.
pub const DEFAULT_IDLE_THRESHOLD_SECS: u64 = 3;

/// Single signal emitted exactly once per idle period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleSignal {
    /// How long the tab was silent before we considered it idle.
    pub silent_for: Duration,
}

#[derive(Debug)]
pub struct IdleWatcher {
    threshold: Duration,
    enabled: bool,
    /// `None` until first byte observed.
    last_output: Option<Instant>,
    last_bytes: u64,
    /// Set after we emit the signal, cleared by [`reset`](Self::reset).
    notified: bool,
}

impl IdleWatcher {
    /// `enabled = false` makes [`poll`](Self::poll) always return `None`.
    pub fn new(threshold: Duration, enabled: bool) -> Self {
        Self {
            threshold,
            enabled,
            last_output: None,
            last_bytes: 0,
            notified: false,
        }
    }

    /// Build with the default threshold (3s) and `enabled = true`.
    pub fn default_for_provider() -> Self {
        Self::new(
            Duration::from_secs(DEFAULT_IDLE_THRESHOLD_SECS),
            true,
        )
    }

    /// Poll with the current `bytes_processed` from the PTY.
    /// Returns `Some(IdleSignal)` exactly once per idle period — once the
    /// counter has been still for `threshold` after producing some output and
    /// no signal has been emitted since the last reset.
    pub fn poll(&mut self, current_bytes: u64) -> Option<IdleSignal> {
        if !self.enabled {
            return None;
        }
        let now = Instant::now();
        if current_bytes != self.last_bytes {
            self.last_bytes = current_bytes;
            self.last_output = Some(now);
            self.notified = false;
            return None;
        }
        if self.notified {
            return None;
        }
        let last = self.last_output?;
        let silent = now.saturating_duration_since(last);
        if silent >= self.threshold {
            self.notified = true;
            Some(IdleSignal { silent_for: silent })
        } else {
            None
        }
    }

    /// Drop the "already notified" gate so the next idle period can re-fire.
    /// Call this when the user acknowledges (e.g. focuses the workspace).
    pub fn reset(&mut self) {
        self.notified = false;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watcher_with(threshold_ms: u64) -> IdleWatcher {
        IdleWatcher::new(Duration::from_millis(threshold_ms), true)
    }

    #[test]
    fn no_signal_before_first_output() {
        let mut w = watcher_with(50);
        std::thread::sleep(Duration::from_millis(80));
        assert!(w.poll(0).is_none());
    }

    #[test]
    fn signal_fires_once_after_threshold() {
        let mut w = watcher_with(40);
        // First output establishes baseline.
        assert!(w.poll(100).is_none());
        std::thread::sleep(Duration::from_millis(60));
        let sig = w.poll(100).expect("idle signal expected");
        assert!(sig.silent_for >= Duration::from_millis(40));
        // Subsequent polls don't re-fire.
        assert!(w.poll(100).is_none());
    }

    #[test]
    fn new_output_resets_timer() {
        let mut w = watcher_with(40);
        w.poll(100);
        std::thread::sleep(Duration::from_millis(60));
        assert!(w.poll(100).is_some());
        // New output → timer resets, must wait again.
        assert!(w.poll(200).is_none());
        assert!(w.poll(200).is_none());
        std::thread::sleep(Duration::from_millis(60));
        assert!(w.poll(200).is_some());
    }

    #[test]
    fn reset_allows_signal_to_re_fire() {
        let mut w = watcher_with(40);
        w.poll(100);
        std::thread::sleep(Duration::from_millis(60));
        assert!(w.poll(100).is_some());
        assert!(w.poll(100).is_none()); // gated by `notified`
        w.reset();
        assert!(w.poll(100).is_some()); // reset → fires again
    }

    #[test]
    fn disabled_never_fires() {
        let mut w = IdleWatcher::new(Duration::from_millis(10), false);
        w.poll(100);
        std::thread::sleep(Duration::from_millis(40));
        assert!(w.poll(100).is_none());
    }
}
