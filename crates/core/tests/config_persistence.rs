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

#[test]
fn test_review_workspace_survives_missing_checkout() {
    let (_dir, repo_path) = common::setup_test_repo();

    // Simulates a restored PR-review workspace whose checkout directory was
    // pruned/deleted on disk since the last run: it must round-trip through
    // save/load (unlike a regular stale workspace) with `ephemeral` and its
    // PR identity intact, so the TUI can mark it `review_broken` and retry.
    let missing_checkout = repo_path.join("review-checkout-gone");
    let mut ws = WorkspaceInfo::new(
        "owner/repo#42".to_string(),
        "Some PR".to_string(),
        "".to_string(),
        None,
        missing_checkout,
        repo_path.clone(),
    );
    ws.ephemeral = true;
    ws.pr_repo_nwo = Some("owner/repo".to_string());
    ws.pr_number = Some(42);

    config::save(&repo_path, &[ws]).expect("save should succeed");
    let entries = config::load(&repo_path).expect("load should succeed");

    assert_eq!(entries.len(), 1);
    assert!(entries[0].ephemeral);
    assert_eq!(entries[0].pr_repo_nwo.as_deref(), Some("owner/repo"));
    assert_eq!(entries[0].pr_number, Some(42));

    let info = entries.into_iter().next().unwrap().into_info();
    assert!(info.ephemeral);
    assert_eq!(info.pr_repo_nwo.as_deref(), Some("owner/repo"));
    assert_eq!(info.pr_number, Some(42));
}
