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

#[tokio::test]
async fn test_create_simple_accepts_non_git_folder() {
    use piki_core::domain::WorkspaceOrigin;
    let dir = tempfile::TempDir::new().expect("tempdir");
    let manager = WorkspaceManager::new();

    let info = manager
        .create_simple("plain-folder-ws", "desc", "", None, &dir.path().to_path_buf())
        .await
        .expect("create_simple should accept a non-git folder");

    assert_eq!(info.origin, WorkspaceOrigin::Local);
    assert_eq!(info.branch, "main");
    assert_eq!(info.source_repo, dir.path());
}

#[tokio::test]
async fn test_create_simple_in_git_folder_no_remote_is_local() {
    use piki_core::domain::WorkspaceOrigin;
    let (_dir, repo_path) = common::setup_test_repo();
    let manager = WorkspaceManager::new();

    let info = manager
        .create_simple("git-no-remote-ws", "desc", "", None, &repo_path)
        .await
        .expect("create_simple should accept a git folder with no remote");

    assert_eq!(info.origin, WorkspaceOrigin::Local);
    assert_eq!(info.source_repo, repo_path);
}

#[tokio::test]
async fn test_create_from_github_clones_into_managed_destination() {
    use piki_core::domain::{WorkspaceOrigin, WorkspaceType};
    use piki_core::paths::DataPaths;

    // A local "remote" — git clone accepts any URL it can resolve. The
    // create_from_github call unconditionally tags origin as GitHub since the
    // intent of the API is GitHub-origin, so a local source still exercises
    // the full code path.
    let (_remote_dir, remote_path) = common::setup_test_repo();
    let url = remote_path.to_string_lossy().to_string();

    let data_dir = tempfile::TempDir::new().expect("tempdir");
    let paths = DataPaths::new(data_dir.path().to_path_buf());
    let manager = WorkspaceManager::with_paths(paths.clone());
    let expected_name = remote_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let info = manager
        .create_from_github("clone-ws", "desc", "", None, &url)
        .await
        .expect("create_from_github should clone successfully");

    let expected_path = paths.worktrees_dir(&expected_name);
    assert_eq!(info.path, expected_path);
    assert_eq!(info.source_repo, expected_path);
    assert_eq!(info.workspace_type, WorkspaceType::Simple);
    assert!(
        expected_path.join(".git").exists(),
        "cloned repo should have a .git directory"
    );
    match info.origin {
        WorkspaceOrigin::GitHub { url: u } => assert_eq!(u, url),
        WorkspaceOrigin::Local => panic!("expected GitHub origin"),
    }
}
