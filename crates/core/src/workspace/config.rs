use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::domain::{WorkspaceInfo, WorkspaceType};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub kanban_path: Option<String>,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub source_repo: PathBuf,
    #[serde(default)]
    pub workspace_type: WorkspaceType,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub dispatch_card_id: Option<String>,
    #[serde(default)]
    pub dispatch_source_kanban: Option<String>,
    #[serde(default)]
    pub dispatch_agent_name: Option<String>,
}

impl WorkspaceEntry {
    /// Convert this entry into a WorkspaceInfo
    pub fn into_info(self) -> WorkspaceInfo {
        let mut info = WorkspaceInfo::new(
            self.name,
            self.description,
            self.prompt,
            self.kanban_path,
            self.branch,
            self.worktree_path,
            self.source_repo,
        );
        info.workspace_type = self.workspace_type;
        info.group = self.group;
        info.order = self.order;
        info.dispatch_card_id = self.dispatch_card_id;
        info.dispatch_source_kanban = self.dispatch_source_kanban;
        info.dispatch_agent_name = self.dispatch_agent_name;
        info
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectConfig {
    project_root: PathBuf,
    workspaces: Vec<WorkspaceEntry>,
}

/// Base directory for workspace config files.
fn config_dir() -> PathBuf {
    crate::xdg::data_dir().join("workspaces")
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
/// Takes a slice of WorkspaceInfo and the git root to filter by project.
pub fn save(git_root: &Path, workspaces: &[WorkspaceInfo]) -> anyhow::Result<()> {
    let entries: Vec<WorkspaceEntry> = workspaces
        .iter()
        .filter(|ws| ws.source_repo == git_root)
        .map(|ws| WorkspaceEntry {
            name: ws.name.clone(),
            description: ws.description.clone(),
            prompt: ws.prompt.clone(),
            kanban_path: ws.kanban_path.clone(),
            branch: ws.branch.clone(),
            worktree_path: ws.path.clone(),
            source_repo: ws.source_repo.clone(),
            workspace_type: ws.workspace_type,
            group: ws.group.clone(),
            order: ws.order,
            dispatch_card_id: ws.dispatch_card_id.clone(),
            dispatch_source_kanban: ws.dispatch_source_kanban.clone(),
            dispatch_agent_name: ws.dispatch_agent_name.clone(),
        })
        .collect();

    let config = ProjectConfig {
        project_root: git_root.to_path_buf(),
        workspaces: entries,
    };

    let path = config_path(git_root);
    std::fs::create_dir_all(path.parent().unwrap()).context("failed to create config directory")?;
    let json = serde_json::to_string_pretty(&config).context("failed to serialize config")?;
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
    let mut entries: Vec<WorkspaceEntry> = config
        .workspaces
        .into_iter()
        .filter(|e| e.worktree_path.exists())
        .collect();

    // Sort by order field
    entries.sort_by_key(|e| e.order);

    // Legacy migration: if all entries have order == 0, assign sequential values
    if entries.len() > 1 && entries.iter().all(|e| e.order == 0) {
        for (i, e) in entries.iter_mut().enumerate() {
            e.order = i as u32;
        }
    }

    Ok(entries)
}

/// Load all workspace entries from every project config in the config directory.
pub fn load_all() -> Vec<WorkspaceEntry> {
    load_all_from_dir(&config_dir())
}

/// Load all workspace entries using a custom data directory.
pub fn load_all_with_paths(paths: &crate::paths::DataPaths) -> Vec<WorkspaceEntry> {
    load_all_from_dir(&paths.legacy_workspaces_dir())
}

fn load_all_from_dir(dir: &Path) -> Vec<WorkspaceEntry> {
    if !dir.exists() {
        return Vec::new();
    }

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut all_entries = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

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

        for e in config.workspaces {
            if e.worktree_path.exists() && !seen_paths.contains(&e.worktree_path) {
                seen_paths.insert(e.worktree_path.clone());
                all_entries.push(e);
            }
        }
    }

    // Sort by order field
    all_entries.sort_by_key(|e| e.order);

    // Legacy migration: if all entries have order == 0, assign sequential values
    if all_entries.len() > 1 && all_entries.iter().all(|e| e.order == 0) {
        for (i, e) in all_entries.iter_mut().enumerate() {
            e.order = i as u32;
        }
    }

    all_entries
}
