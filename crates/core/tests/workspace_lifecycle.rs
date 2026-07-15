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
    assert_eq!(info.branch, "");
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
    // macOS resolves `/tmp` to `/private/tmp`, so `git rev-parse
    // --show-toplevel` returns the canonical path while `TempDir` reports
    // the symlink path. Compare canonical forms to be cross-platform.
    let expected = repo_path.canonicalize().unwrap_or(repo_path.clone());
    let actual = info
        .source_repo
        .canonicalize()
        .unwrap_or(info.source_repo.clone());
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_create_from_github_clones_into_chosen_destination() {
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

    // User-chosen destination distinct from worktrees_dir so we prove the
    // clone really honors the argument.
    let chosen_dest = data_dir.path().join("my-projects");
    std::fs::create_dir_all(&chosen_dest).expect("create chosen_dest");

    let info = manager
        .create_from_github("clone-ws", "desc", "", None, &url, &chosen_dest)
        .await
        .expect("create_from_github should clone successfully");

    let expected_path = chosen_dest.join(&expected_name);
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

#[tokio::test]
async fn test_create_from_github_rejects_missing_destination() {
    use piki_core::paths::DataPaths;

    let (_remote_dir, remote_path) = common::setup_test_repo();
    let url = remote_path.to_string_lossy().to_string();

    let data_dir = tempfile::TempDir::new().expect("tempdir");
    let paths = DataPaths::new(data_dir.path().to_path_buf());
    let manager = WorkspaceManager::with_paths(paths);

    // A non-default, non-existent destination must error rather than
    // silently fall back to creating it. The repos_dir() default is the
    // only path the manager auto-creates.
    let bogus = data_dir.path().join("nope-not-there");
    let err = manager
        .create_from_github("clone-ws", "", "", None, &url, &bogus)
        .await
        .expect_err("should reject missing user-chosen destination");
    assert!(
        err.to_string().contains("does not exist"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_create_from_github_auto_creates_default_repos_dir() {
    use piki_core::paths::DataPaths;

    let (_remote_dir, remote_path) = common::setup_test_repo();
    let url = remote_path.to_string_lossy().to_string();

    let data_dir = tempfile::TempDir::new().expect("tempdir");
    let paths = DataPaths::new(data_dir.path().to_path_buf());
    let manager = WorkspaceManager::with_paths(paths.clone());

    // The default repos_dir does NOT exist on first run; the manager
    // should auto-create it as a convenience for the dialog default.
    let default_dest = paths.repos_dir();
    assert!(!default_dest.exists(), "test precondition");

    let info = manager
        .create_from_github("clone-ws", "", "", None, &url, &default_dest)
        .await
        .expect("default destination should auto-create");

    assert!(default_dest.is_dir(), "default repos_dir created");
    assert!(info.path.starts_with(&default_dest));
}

#[tokio::test]
async fn test_list_worktrees_excludes_main_checkout() {
    let (_dir, repo_path) = common::setup_test_repo();
    let manager = WorkspaceManager::new();

    let ws_a = manager
        .create("list-ws-a", "", "", None, &repo_path)
        .await
        .expect("create a should succeed");
    let ws_b = manager
        .create("list-ws-b", "", "", None, &repo_path)
        .await
        .expect("create b should succeed");

    let worktrees = manager
        .list_worktrees(&repo_path)
        .await
        .expect("list_worktrees should succeed");

    let paths: Vec<_> = worktrees.iter().map(|w| w.path.clone()).collect();
    assert!(paths.contains(&ws_a.path));
    assert!(paths.contains(&ws_b.path));
    assert!(
        !paths.contains(&repo_path),
        "main checkout should be excluded"
    );
    assert_eq!(worktrees.len(), 2);

    let a = worktrees
        .iter()
        .find(|w| w.path == ws_a.path)
        .expect("ws_a present");
    assert_eq!(a.branch, "list-ws-a");

    manager.remove("list-ws-a", &repo_path).await.ok();
    manager.remove("list-ws-b", &repo_path).await.ok();
}

#[tokio::test]
async fn test_import_existing_worktree_registers_without_git_worktree_add() {
    let (_dir, repo_path) = common::setup_test_repo();
    let manager = WorkspaceManager::new();

    let created = manager
        .create("import-ws", "", "", None, &repo_path)
        .await
        .expect("create should succeed");

    let worktrees_before = manager
        .list_worktrees(&repo_path)
        .await
        .expect("list should succeed");
    assert_eq!(worktrees_before.len(), 1);

    let info = manager
        .import_existing_worktree(
            "import-ws",
            created.branch.clone(),
            created.path.clone(),
            repo_path.clone(),
        )
        .await
        .expect("import should succeed");

    assert_eq!(info.path, created.path);
    assert_eq!(info.branch, created.branch);
    assert_eq!(info.workspace_type, piki_core::WorkspaceType::Worktree);

    // No new worktree should have been created on disk.
    let worktrees_after = manager
        .list_worktrees(&repo_path)
        .await
        .expect("list should succeed");
    assert_eq!(worktrees_after.len(), 1);

    manager.remove("import-ws", &repo_path).await.ok();
}

#[tokio::test]
async fn test_import_existing_worktree_rejects_missing_path() {
    let (_dir, repo_path) = common::setup_test_repo();
    let manager = WorkspaceManager::new();

    let missing = repo_path.join("does-not-exist-on-disk");
    let err = manager
        .import_existing_worktree(
            "ghost-ws",
            "some-branch".to_string(),
            missing,
            repo_path.clone(),
        )
        .await
        .expect_err("importing a missing path should fail");
    assert!(
        err.to_string().contains("no longer exists"),
        "unexpected error: {err}"
    );
}
