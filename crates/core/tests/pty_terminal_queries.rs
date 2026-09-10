//! End-to-end check that `PtySession` answers terminal startup probes.
//!
//! Ignored by default: it drives the real `muse` binary. Run it by hand after
//! touching the vt100 answerback path:
//!
//! ```sh
//! cargo test -p piki-core --test pty_terminal_queries -- --ignored --nocapture
//! ```
//!
//! Muse probes the terminal at startup (DSR `CSI 6n`, OSC color queries, DA)
//! and exits silently with code 0 if nothing ever answers — which is exactly
//! what happened before the PTY reader drained the vt100 answerback buffer.

#![cfg(unix)]

use std::time::Duration;

use piki_core::pty::PtySession;

fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(bin))
            .find(|p| p.is_file())
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives the real `muse` binary"]
async fn muse_survives_startup_terminal_probe() {
    if which("muse").is_none() {
        eprintln!("skipping: `muse` not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();

    let mut session = PtySession::spawn(
        dir.path(),
        24,
        80,
        "muse",
        &[],
        &[],
        &[],
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Before the answerback fix muse gave up within ~2s of unanswered probes.
    tokio::time::sleep(Duration::from_secs(6)).await;
    let alive = session.is_alive();
    let _ = session.kill();
    assert!(
        alive,
        "muse exited: terminal startup probes went unanswered"
    );
}
