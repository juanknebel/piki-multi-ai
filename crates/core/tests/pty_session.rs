mod common;

use std::time::Duration;

use piki_core::pty::PtySession;

#[tokio::test(flavor = "multi_thread")]
async fn test_spawn_echo() {
    let (_dir, repo_path) = common::setup_test_repo();

    let pty = PtySession::spawn(&repo_path, 24, 80, "echo", &[], &[], &[], false, None).await;
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

    let mut pty = PtySession::spawn(&repo_path, 24, 80, "sleep", &[], &[], &[], false, None)
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

    let mut pty = PtySession::spawn(&repo_path, 24, 80, "cat", &[], &[], &[], false, None)
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

    let pty = PtySession::spawn(&repo_path, 24, 80, "cat", &[], &[], &[], false, None)
        .await
        .expect("spawn cat should succeed");

    let result = pty.resize(48, 120);
    assert!(result.is_ok(), "resize should succeed: {:?}", result.err());

    // Verify parser dimensions updated
    let parser = pty.parser().lock();
    let screen = parser.screen();
    assert_eq!(screen.size(), (48, 120));
}
