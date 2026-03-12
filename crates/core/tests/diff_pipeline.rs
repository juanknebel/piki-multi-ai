mod common;

use piki_core::diff::runner::run_diff;
use piki_core::domain::FileStatus;

#[tokio::test]
async fn test_diff_modified_file() {
    let (_dir, repo_path) = common::setup_test_repo();

    // Modify the README
    std::fs::write(repo_path.join("README.md"), "# Modified\nNew content\n").unwrap();

    let result = run_diff(&repo_path, "README.md", 120, &FileStatus::Modified).await;
    assert!(result.is_ok(), "diff should succeed: {:?}", result.err());

    let bytes = result.unwrap();
    assert!(!bytes.is_empty(), "diff output should not be empty");
}

#[tokio::test]
async fn test_diff_untracked_file() {
    let (_dir, repo_path) = common::setup_test_repo();

    std::fs::write(repo_path.join("new_file.txt"), "hello world\n").unwrap();

    let result = run_diff(&repo_path, "new_file.txt", 120, &FileStatus::Untracked).await;
    assert!(result.is_ok(), "diff should succeed: {:?}", result.err());

    let bytes = result.unwrap();
    assert!(
        !bytes.is_empty(),
        "diff output for untracked file should not be empty"
    );
}

#[tokio::test]
async fn test_diff_no_changes() {
    let (_dir, repo_path) = common::setup_test_repo();

    // README.md is committed and unchanged
    let result = run_diff(&repo_path, "README.md", 120, &FileStatus::Modified).await;
    assert!(result.is_ok());

    let bytes = result.unwrap();
    // No changes → diff output should be empty
    assert!(bytes.is_empty(), "diff of unchanged file should be empty");
}
