use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Create a temporary git repository suitable for integration tests.
/// Returns the TempDir (for lifetime) and the path to the repo root.
pub fn setup_test_repo() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().to_path_buf();

    Command::new("git")
        .args(["init"])
        .current_dir(&path)
        .output()
        .expect("git init failed");

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&path)
        .output()
        .expect("git config email failed");

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&path)
        .output()
        .expect("git config name failed");

    std::fs::write(path.join("README.md"), "# Test Repo\n").expect("failed to write README");

    Command::new("git")
        .args(["add", "."])
        .current_dir(&path)
        .output()
        .expect("git add failed");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&path)
        .output()
        .expect("git commit failed");

    (dir, path)
}
