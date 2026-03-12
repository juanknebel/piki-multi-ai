mod common;

use piki_core::workspace::WorkspaceManager;

#[tokio::test]
async fn test_create_and_remove_workspace() {
    let (_dir, repo_path) = common::setup_test_repo();
    let manager = WorkspaceManager::new();

    let ws = manager
        .create("test-ws-create", "desc", "", None, &repo_path)
        .await
        .expect("create should succeed");

    assert_eq!(ws.name, "test-ws-create");
    assert_eq!(ws.branch, "test-ws-create");
    assert!(ws.path.exists(), "worktree directory should exist");

    manager
        .remove("test-ws-create", &repo_path)
        .await
        .expect("remove should succeed");

    assert!(!ws.path.exists(), "worktree directory should be removed");
}

#[tokio::test]
async fn test_create_duplicate_fails() {
    let (_dir, repo_path) = common::setup_test_repo();
    let manager = WorkspaceManager::new();

    manager
        .create("test-ws-dup", "desc", "", None, &repo_path)
        .await
        .expect("first create should succeed");

    let result = manager
        .create("test-ws-dup", "desc", "", None, &repo_path)
        .await;

    assert!(result.is_err(), "duplicate create should fail");

    // Cleanup
    manager.remove("test-ws-dup", &repo_path).await.ok();
}

#[tokio::test]
async fn test_detect_main_branch() {
    let (_dir, repo_path) = common::setup_test_repo();

    // Default branch in a fresh repo is usually "main" or "master"
    let branch = WorkspaceManager::detect_main_branch(&repo_path).await;
    assert!(
        branch == "main" || branch == "master",
        "expected 'main' or 'master', got '{}'",
        branch
    );
}
