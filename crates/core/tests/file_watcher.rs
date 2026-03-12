mod common;

use std::time::{Duration, Instant};

use piki_core::workspace::FileWatcher;
use piki_core::workspace::watcher::WatchEventKind;

/// Helper: poll watcher with retries until an event is received or timeout.
fn wait_for_event(watcher: &mut FileWatcher, timeout: Duration) -> Option<WatchEventKind> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(event) = watcher.try_recv() {
            return Some(event.kind);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[tokio::test]
async fn test_detect_file_creation() {
    let (_dir, repo_path) = common::setup_test_repo();
    let mut watcher = FileWatcher::new(repo_path.clone(), "test-watcher".to_string())
        .expect("watcher should start");

    // Create a new file
    std::fs::write(repo_path.join("created.txt"), "new").unwrap();

    let kind = wait_for_event(&mut watcher, Duration::from_secs(2));
    assert!(kind.is_some(), "should detect file creation within timeout");
}

#[tokio::test]
async fn test_detect_file_modification() {
    let (_dir, repo_path) = common::setup_test_repo();

    // Create file before watcher starts, so the watcher only sees modification
    let file_path = repo_path.join("modify_me.txt");
    std::fs::write(&file_path, "original").unwrap();

    let mut watcher = FileWatcher::new(repo_path.clone(), "test-watcher-mod".to_string())
        .expect("watcher should start");

    // Small delay so watcher is ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    std::fs::write(&file_path, "modified").unwrap();

    let kind = wait_for_event(&mut watcher, Duration::from_secs(2));
    assert!(
        kind.is_some(),
        "should detect file modification within timeout"
    );
}

#[tokio::test]
async fn test_detect_file_deletion() {
    let (_dir, repo_path) = common::setup_test_repo();

    let file_path = repo_path.join("delete_me.txt");
    std::fs::write(&file_path, "bye").unwrap();

    let mut watcher = FileWatcher::new(repo_path.clone(), "test-watcher-del".to_string())
        .expect("watcher should start");

    // Small delay so watcher is ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    std::fs::remove_file(&file_path).unwrap();

    let kind = wait_for_event(&mut watcher, Duration::from_secs(2));
    assert!(kind.is_some(), "should detect file deletion within timeout");
}

#[tokio::test]
async fn test_ignore_git_directory() {
    let (_dir, repo_path) = common::setup_test_repo();
    let mut watcher = FileWatcher::new(repo_path.clone(), "test-watcher-git".to_string())
        .expect("watcher should start");

    // Small delay so watcher is ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Write to .git directory — should be ignored
    let git_file = repo_path.join(".git/test_ignore");
    std::fs::write(&git_file, "ignored").unwrap();

    let kind = wait_for_event(&mut watcher, Duration::from_secs(1));
    assert!(kind.is_none(), "events in .git/ should be ignored");
}
