mod common;

use std::path::PathBuf;

use piki_core::WorkspaceInfo;
use piki_core::workspace::config;

#[test]
fn test_save_and_load_config() {
    let (_dir, repo_path) = common::setup_test_repo();

    let workspaces = vec![WorkspaceInfo::new(
        "ws-config-test".to_string(),
        "test workspace".to_string(),
        "".to_string(),
        None,
        "ws-config-test".to_string(),
        repo_path.join("ws-config-test"),
        repo_path.clone(),
    )];

    // Create the fake worktree dir so load doesn't filter it out
    std::fs::create_dir_all(&workspaces[0].path).unwrap();

    config::save(&repo_path, &workspaces).expect("save should succeed");
    let entries = config::load(&repo_path).expect("load should succeed");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "ws-config-test");
    assert_eq!(entries[0].description, "test workspace");

    // Cleanup
    std::fs::remove_dir_all(&workspaces[0].path).ok();
}

#[test]
fn test_load_nonexistent_config() {
    let path = PathBuf::from("/tmp/nonexistent-piki-test-repo-xyz");
    let entries = config::load(&path).expect("load of missing config should return empty");
    assert!(entries.is_empty());
}

#[test]
fn test_stale_entries_filtered() {
    let (_dir, repo_path) = common::setup_test_repo();

    let stale_path = repo_path.join("stale-ws-that-does-not-exist");

    let workspaces = vec![WorkspaceInfo::new(
        "stale-ws".to_string(),
        "stale".to_string(),
        "".to_string(),
        None,
        "stale-ws".to_string(),
        stale_path,
        repo_path.clone(),
    )];

    config::save(&repo_path, &workspaces).expect("save should succeed");
    let entries = config::load(&repo_path).expect("load should succeed");

    assert!(
        entries.is_empty(),
        "stale entries should be filtered out, got {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}
