use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::app::Workspace;

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub name: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub source_repo: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectConfig {
    project_root: PathBuf,
    workspaces: Vec<WorkspaceEntry>,
}

/// Base directory for workspace config files.
fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/share/piki-multi/workspaces")
}

/// Config file path for a given git root.
fn config_path(git_root: &Path) -> PathBuf {
    let project_name = git_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    config_dir().join(format!("{}.json", project_name))
}

/// Save workspace list to disk.
pub fn save(git_root: &Path, workspaces: &[Workspace]) -> anyhow::Result<()> {
    let entries: Vec<WorkspaceEntry> = workspaces
        .iter()
        .map(|ws| WorkspaceEntry {
            name: ws.name.clone(),
            branch: ws.branch.clone(),
            worktree_path: ws.path.clone(),
            source_repo: ws.source_repo.clone(),
        })
        .collect();

    let config = ProjectConfig {
        project_root: git_root.to_path_buf(),
        workspaces: entries,
    };

    let path = config_path(git_root);
    std::fs::create_dir_all(path.parent().unwrap())
        .context("failed to create config directory")?;
    let json = serde_json::to_string_pretty(&config)
        .context("failed to serialize config")?;
    std::fs::write(&path, json).context("failed to write config file")?;
    Ok(())
}

/// Load workspace entries from disk, filtering out stale entries whose worktree dir no longer exists.
pub fn load(git_root: &Path) -> anyhow::Result<Vec<WorkspaceEntry>> {
    let path = config_path(git_root);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let data = std::fs::read_to_string(&path).context("failed to read config file")?;
    let config: ProjectConfig =
        serde_json::from_str(&data).context("failed to parse config file")?;

    // Filter out stale entries
    let entries = config
        .workspaces
        .into_iter()
        .filter(|e| e.worktree_path.exists())
        .collect();

    Ok(entries)
}

/// Load all workspace entries from every project config in the config directory.
/// Scans `~/.local/share/piki-multi/workspaces/*.json` and returns all valid entries,
/// filtering out stale worktrees that no longer exist on disk.
/// Skips corrupted or unreadable config files gracefully.
pub fn load_all() -> Vec<WorkspaceEntry> {
    let dir = config_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let read_dir = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut all_entries = Vec::new();

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let config: ProjectConfig = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Only keep entries whose worktree still exists on disk
        let valid = config
            .workspaces
            .into_iter()
            .filter(|e| e.worktree_path.exists());
        all_entries.extend(valid);
    }

    all_entries
}
