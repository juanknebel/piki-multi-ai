mod common;

use std::time::Duration;

use piki_core::pty::PtySession;

#[tokio::test(flavor = "multi_thread")]
async fn test_spawn_echo() {
    let (_dir, repo_path) = common::setup_test_repo();

    let pty = PtySession::spawn(&repo_path, 24, 80, "echo", &[], &[], &[], false, None, None).await;
    assert!(pty.is_ok(), "spawn echo should succeed: {:?}", pty.err());

    // Poll for echo to exit. The 200ms sleep that used to live here was flaky
    // on loaded CI runners (ubuntu in particular); a 2 s deadline gives
    // headroom without slowing the happy path.
    let mut pty = pty.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while pty.is_alive() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!pty.is_alive(), "echo should have exited within 2s");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_is_alive() {
    let (_dir, repo_path) = common::setup_test_repo();

    let mut pty = PtySession::spawn(&repo_path, 24, 80, "sleep", &[], &[], &[], false, None, None)
        .await
        .expect("spawn sleep should succeed");

    // sleep with no argument may exit immediately, so just check it doesn't panic
    // The important thing is that is_alive() works without crashing
    let _ = pty.is_alive();

    pty.kill().ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_write_and_read_cat() {
    let (_dir, repo_path) = common::setup_test_repo();

    let mut pty = PtySession::spawn(&repo_path, 24, 80, "cat", &[], &[], &[], false, None, None)
        .await
        .expect("spawn cat should succeed");

    assert!(pty.is_alive(), "cat should be running");

    // Write some data
    pty.write(b"hello\n").expect("write should succeed");

    // Give cat time to echo back
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Check parser has received data
    let bytes = pty.bytes_processed();
    assert!(
        bytes > 0,
        "cat should have echoed data, bytes_processed={}",
        bytes
    );

    // Cleanup
    pty.kill().ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_resize() {
    let (_dir, repo_path) = common::setup_test_repo();

    let pty = PtySession::spawn(&repo_path, 24, 80, "cat", &[], &[], &[], false, None, None)
        .await
        .expect("spawn cat should succeed");

    let result = pty.resize(48, 120);
    assert!(result.is_ok(), "resize should succeed: {:?}", result.err());

    // Verify parser dimensions updated
    let parser = pty.parser().lock();
    let screen = parser.screen();
    assert_eq!(screen.size(), (48, 120));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_output_signal_raised_on_pty_output() {
    let (_dir, repo_path) = common::setup_test_repo();

    let signal = piki_core::pty::PtyOutputSignal::new();
    let mut pty = PtySession::spawn(
        &repo_path,
        24,
        80,
        "cat",
        &[],
        &[],
        &[],
        false,
        None,
        Some(signal.clone()),
    )
    .await
    .expect("spawn cat should succeed");

    pty.write(b"hello\n").expect("write should succeed");

    // The reader thread raises the signal after flushing cat's echo into the
    // parser; `notified()` must resolve even if the raise landed before we
    // started waiting (Notify stores the permit).
    tokio::time::timeout(Duration::from_secs(2), signal.notified())
        .await
        .expect("output signal should fire within 2s");
    assert!(signal.take(), "dirty bit should be set after output");
    assert!(!signal.take(), "take() must clear the dirty bit");

    pty.kill().ok();
}
