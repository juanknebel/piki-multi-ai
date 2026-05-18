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

/// Minimum bytes the PTY must produce *after* a previous idle signal before
/// the watcher is willing to re-arm. Filters cursor blinks (~6 B), status
/// redraws (~50 B), and spinner frames (~20 B) that would otherwise cause
/// the watcher to re-fire on every idle period without any meaningful
/// activity. A real agent turn produces hundreds of bytes minimum.
pub const DEFAULT_IDLE_REARM_BYTES: u64 = 256;

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
    /// Minimum cumulative byte delta past `bytes_at_last_fire` required to
    /// re-arm after a fire. See [`DEFAULT_IDLE_REARM_BYTES`].
    rearm_bytes: u64,
    /// `None` until first byte observed.
    last_output: Option<Instant>,
    last_bytes: u64,
    /// Byte counter at the moment we last emitted the signal. Re-arm gate
    /// compares against this — see `rearm_bytes`.
    bytes_at_last_fire: u64,
    /// Set after we emit the signal; cleared by the next *meaningful* byte
    /// delta (>= `rearm_bytes`).
    notified: bool,
}

impl IdleWatcher {
    /// `enabled = false` makes [`poll`](Self::poll) always return `None`.
    /// `rearm_bytes` is the cumulative byte delta required after a fire to
    /// re-arm — set to 0 to re-arm on any byte change (legacy behaviour).
    pub fn new(threshold: Duration, enabled: bool, rearm_bytes: u64) -> Self {
        Self {
            threshold,
            enabled,
            rearm_bytes,
            last_output: None,
            last_bytes: 0,
            bytes_at_last_fire: 0,
            notified: false,
        }
    }

    /// Build with the default threshold (3s), `enabled = true`, and the
    /// default re-arm delta ([`DEFAULT_IDLE_REARM_BYTES`]). Equivalent to
    /// [`from_provider_config(None)`](Self::from_provider_config).
    pub fn default_for_provider() -> Self {
        Self::from_provider_config(None)
    }

    /// Build from a provider's `providers.toml` entry, honouring its
    /// per-provider fallback knobs:
    /// - `idle_threshold_secs` → threshold (default
    ///   [`DEFAULT_IDLE_THRESHOLD_SECS`] when unset)
    /// - `idle_notify` → enabled (default `true` when no config)
    ///
    /// `None` yields the universal defaults. This is the single mapping point
    /// from provider config to watcher params — both the TUI and desktop
    /// frontends call it so per-provider config behaves identically.
    pub fn from_provider_config(cfg: Option<&crate::providers::ProviderConfig>) -> Self {
        let threshold = cfg
            .and_then(|c| c.idle_threshold_secs)
            .unwrap_or(DEFAULT_IDLE_THRESHOLD_SECS);
        let enabled = cfg.map(|c| c.idle_notify).unwrap_or(true);
        Self::new(
            Duration::from_secs(threshold),
            enabled,
            DEFAULT_IDLE_REARM_BYTES,
        )
    }

    /// Poll with the current `bytes_processed` from the PTY.
    /// Returns `Some(IdleSignal)` exactly once per idle period — once the
    /// counter has been still for `threshold` after producing some output
    /// and no signal has been emitted since the last *meaningful* byte
    /// delta. Micro-bursts below `rearm_bytes` advance the silent timer but
    /// do not re-arm the notification gate.
    pub fn poll(&mut self, current_bytes: u64) -> Option<IdleSignal> {
        if !self.enabled {
            return None;
        }
        let now = Instant::now();
        if current_bytes != self.last_bytes {
            self.last_bytes = current_bytes;
            self.last_output = Some(now);
            // Re-arm only when the cumulative delta since last fire is big
            // enough to count as real activity. Pre-fire (initial arm) the
            // gate is `notified=false` already, so this branch is a no-op.
            if self.notified
                && current_bytes.saturating_sub(self.bytes_at_last_fire) >= self.rearm_bytes
            {
                self.notified = false;
            }
            return None;
        }
        if self.notified {
            return None;
        }
        let last = self.last_output?;
        let silent = now.saturating_duration_since(last);
        if silent >= self.threshold {
            self.notified = true;
            self.bytes_at_last_fire = current_bytes;
            Some(IdleSignal { silent_for: silent })
        } else {
            None
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy-shape watcher used by tests that pre-date the rearm gate.
    /// `rearm_bytes = 0` makes the gate a no-op so the original assertions
    /// (any byte change → re-arm) still hold.
    fn watcher_with(threshold_ms: u64) -> IdleWatcher {
        IdleWatcher::new(Duration::from_millis(threshold_ms), true, 0)
    }

    fn watcher_with_delta(threshold_ms: u64, rearm_bytes: u64) -> IdleWatcher {
        IdleWatcher::new(Duration::from_millis(threshold_ms), true, rearm_bytes)
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
    fn disabled_never_fires() {
        let mut w = IdleWatcher::new(Duration::from_millis(10), false, 0);
        w.poll(100);
        std::thread::sleep(Duration::from_millis(40));
        assert!(w.poll(100).is_none());
    }

    #[test]
    fn micro_burst_below_rearm_delta_does_not_refire() {
        // Threshold low for test speed; rearm delta = 256 bytes.
        let mut w = watcher_with_delta(40, 256);
        // First arm: establish a baseline at 1000 bytes.
        assert!(w.poll(1000).is_none());
        std::thread::sleep(Duration::from_millis(60));
        // First fire after 40ms of stillness.
        assert!(w.poll(1000).is_some());
        // Simulate a cursor-blink / status-redraw style micro-burst: +20
        // bytes (well below the 256 rearm threshold).
        assert!(w.poll(1020).is_none());
        std::thread::sleep(Duration::from_millis(60));
        // Watcher should NOT re-fire — delta since last fire is only 20.
        assert!(w.poll(1020).is_none());
        // Even after many polls and more time, still suppressed.
        std::thread::sleep(Duration::from_millis(60));
        assert!(w.poll(1020).is_none());
    }

    #[test]
    fn from_provider_config_honors_threshold_and_notify() {
        use crate::providers::{ProviderConfig, PromptFormat};
        let base = ProviderConfig {
            name: "X".into(),
            description: String::new(),
            command: "x".into(),
            default_args: Vec::new(),
            prompt_format: PromptFormat::Positional,
            dispatchable: false,
            agent_dir: None,
            idle_threshold_secs: Some(12),
            idle_notify: true,
            icon: None,
        };
        let w = IdleWatcher::from_provider_config(Some(&base));
        assert_eq!(w.threshold, Duration::from_secs(12));
        assert!(w.enabled());

        // idle_notify = false → disabled watcher (never fires).
        let mut quiet = base.clone();
        quiet.idle_notify = false;
        quiet.idle_threshold_secs = None; // also exercise the default branch
        let w = IdleWatcher::from_provider_config(Some(&quiet));
        assert_eq!(
            w.threshold,
            Duration::from_secs(DEFAULT_IDLE_THRESHOLD_SECS)
        );
        assert!(!w.enabled());

        // None → universal defaults; default_for_provider delegates here.
        let w = IdleWatcher::from_provider_config(None);
        assert_eq!(
            w.threshold,
            Duration::from_secs(DEFAULT_IDLE_THRESHOLD_SECS)
        );
        assert!(w.enabled());
        assert_eq!(w.rearm_bytes, DEFAULT_IDLE_REARM_BYTES);
    }

    #[test]
    fn significant_burst_re_arms_after_fire() {
        let mut w = watcher_with_delta(40, 256);
        // First arm + fire.
        assert!(w.poll(1000).is_none());
        std::thread::sleep(Duration::from_millis(60));
        assert!(w.poll(1000).is_some());
        // A real agent turn: +1024 bytes (well past the 256 threshold).
        assert!(w.poll(2024).is_none());
        std::thread::sleep(Duration::from_millis(60));
        // Watcher re-armed and fires again.
        assert!(w.poll(2024).is_some());
    }
}
