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

/// Poll until an event mentioning `needle` (as a final path component)
/// arrives, or timeout. Ignores unrelated events (e.g. the mkdir itself).
fn wait_for_path_event(watcher: &mut FileWatcher, timeout: Duration, needle: &str) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(event) = watcher.try_recv() {
            if event.paths.iter().any(|p| p.ends_with(needle)) {
                return true;
            }
            continue;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[tokio::test]
async fn test_detect_file_in_directory_created_after_start() {
    let (_dir, repo_path) = common::setup_test_repo();
    let mut watcher = FileWatcher::new(repo_path.clone(), "test-watcher-newdir".to_string())
        .expect("watcher should start");

    // Small delay so watcher is ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create a directory AFTER the watcher started, then a file inside it.
    // In selective mode the subdirectory's watch is registered lazily from
    // the create event, so this exercises the dynamic registration path.
    let sub = repo_path.join("newdir");
    std::fs::create_dir(&sub).unwrap();
    assert!(
        wait_for_path_event(&mut watcher, Duration::from_secs(2), "newdir"),
        "should detect directory creation"
    );

    std::fs::write(sub.join("inner.txt"), "hi").unwrap();
    assert!(
        wait_for_path_event(&mut watcher, Duration::from_secs(2), "inner.txt"),
        "should detect a file created inside the new directory"
    );
}

#[tokio::test]
async fn test_ignore_node_modules_directory() {
    let (_dir, repo_path) = common::setup_test_repo();

    // node_modules exists before the watcher starts, so selective mode never
    // registers it and recursive mode relies on the event-side filter.
    let nm = repo_path.join("node_modules");
    std::fs::create_dir(&nm).unwrap();

    let mut watcher = FileWatcher::new(repo_path.clone(), "test-watcher-nm".to_string())
        .expect("watcher should start");

    // Small delay so watcher is ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    std::fs::write(nm.join("pkg.js"), "x").unwrap();

    let kind = wait_for_event(&mut watcher, Duration::from_secs(1));
    assert!(kind.is_none(), "events in node_modules/ should be ignored");
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
