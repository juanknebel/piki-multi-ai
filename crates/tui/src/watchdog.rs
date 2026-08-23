//! Detects a frozen event loop and leaves a trace in the log.
//!
//! A stuck main thread is invisible from the outside: the screen stops
//! updating and, with the kitty keyboard protocol active, even Ctrl+C is an
//! ordinary key event the stuck loop never reads. Nothing gets logged, so
//! the report that eventually arrives is a screenshot of a trashed shell.
//! The watchdog thread watches a heartbeat the loop bumps every iteration
//! and logs the stall (and the recovery, if any) through the non-blocking
//! appender — which keeps flushing while the main thread is wedged — so the
//! next incident at least has a timestamp and a duration.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The loop ticks at least every 250 ms; anything past this is not load.
const STALL_THRESHOLD: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// While stalled, re-log this often so a long hang leaves a visible trail.
const REPEAT_EVERY: Duration = Duration::from_secs(30);

static EPOCH: OnceLock<Instant> = OnceLock::new();
static LAST_BEAT_MS: AtomicU64 = AtomicU64::new(0);
static SUSPENDED: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Called by the event loop once per iteration.
pub fn beat() {
    LAST_BEAT_MS.store(now_ms(), Ordering::Relaxed);
}

/// Pause stall detection while the loop is deliberately blocked — e.g.
/// suspended behind an external `$EDITOR`. Detection resumes on drop.
#[must_use = "detection resumes as soon as the guard is dropped"]
pub struct SuspendGuard(());

pub fn suspend() -> SuspendGuard {
    SUSPENDED.store(true, Ordering::Relaxed);
    SuspendGuard(())
}

impl Drop for SuspendGuard {
    fn drop(&mut self) {
        beat();
        SUSPENDED.store(false, Ordering::Relaxed);
    }
}

/// Start the watchdog thread. Idempotent enough for one call from `main`.
pub fn start() {
    beat();
    let spawned = std::thread::Builder::new()
        .name("event-loop-watchdog".to_string())
        .spawn(run);
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "event-loop watchdog not started");
    }
}

fn run() {
    let threshold_ms = STALL_THRESHOLD.as_millis() as u64;
    let repeat_ms = REPEAT_EVERY.as_millis() as u64;
    // Beat timestamp the current stall started from, if any.
    let mut stalled_since: Option<u64> = None;
    let mut last_report_ms = 0u64;
    loop {
        std::thread::sleep(CHECK_INTERVAL);
        if SUSPENDED.load(Ordering::Relaxed) {
            continue;
        }
        let now = now_ms();
        let last = LAST_BEAT_MS.load(Ordering::Relaxed);
        let gap = now.saturating_sub(last);
        if gap >= threshold_ms {
            if stalled_since.is_none() || now.saturating_sub(last_report_ms) >= repeat_ms {
                tracing::error!(
                    stalled_for_secs = gap / 1000,
                    "event loop stalled: main thread has not ticked"
                );
                last_report_ms = now;
            }
            stalled_since.get_or_insert(last);
        } else if let Some(since) = stalled_since.take() {
            tracing::warn!(
                stalled_for_secs = last.saturating_sub(since) / 1000,
                "event loop recovered"
            );
        }
    }
}
