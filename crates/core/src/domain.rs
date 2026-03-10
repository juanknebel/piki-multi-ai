use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// An AI assistant that can be run in a PTY
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AIProvider {
    Claude,
    Gemini,
    Codex,
    Shell,
    Kanban,
}

impl AIProvider {
    /// CLI command to execute
    pub fn command(&self) -> &str {
        match self {
            AIProvider::Claude => "claude",
            AIProvider::Gemini => "gemini",
            AIProvider::Codex => "codex",
            AIProvider::Shell => "/bin/sh",
            AIProvider::Kanban => "",
        }
    }

    /// Resolved command: for Shell, use $SHELL env var with fallback
    pub fn resolved_command(&self) -> String {
        match self {
            AIProvider::Shell => std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            other => other.command().to_string(),
        }
    }

    /// Label for the sub-tab
    pub fn label(&self) -> &str {
        match self {
            AIProvider::Claude => "Claude Code",
            AIProvider::Gemini => "Gemini",
            AIProvider::Codex => "Codex",
            AIProvider::Shell => "Shell",
            AIProvider::Kanban => "Kanban Board",
        }
    }

    /// All available providers in display order
    pub fn all() -> &'static [AIProvider] {
        &[
            AIProvider::Claude,
            AIProvider::Gemini,
            AIProvider::Codex,
            AIProvider::Shell,
            AIProvider::Kanban,
        ]
    }
}

/// Status of the process in a workspace
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceStatus {
    Idle,
    Busy,
    Done,
    Error(String),
}

/// Strategy for merging a workspace branch into main
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeStrategy {
    Merge,
    Rebase,
}

/// Git file change status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Modified,       // M in working tree (unstaged)
    Added,          // A in index (staged new file)
    Deleted,        // D
    Renamed,        // R
    Untracked,      // ?? (new files not staged)
    Conflicted,     // UU, AA, DD, etc. (merge conflicts)
    Staged,         // In index, no working tree changes
    StagedModified, // Staged + modified in working tree after staging
}

/// A file that has been changed in a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
}

/// Workspace metadata — the serializable, frontend-agnostic part of a workspace.
/// Each frontend (TUI, Tauri) wraps this with its own runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub kanban_path: Option<String>,
    pub branch: String,
    pub path: PathBuf,
    /// Git root of the source repository this workspace was created from
    pub source_repo: PathBuf,
}

impl WorkspaceInfo {
    pub fn new(
        name: String,
        description: String,
        prompt: String,
        kanban_path: Option<String>,
        branch: String,
        path: PathBuf,
        source_repo: PathBuf,
    ) -> Self {
        Self {
            name,
            description,
            prompt,
            kanban_path,
            branch,
            path,
            source_repo,
        }
    }
}
