use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A session target that can be opened in a PTY or dedicated tab.
///
/// AI agent providers are **not** built-in here — they live in `providers.toml`
/// and are resolved via `ProviderManager`. Only non-AI utility targets remain
/// as built-in variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AIProvider {
    Shell,
    Kanban,
    CodeReview,
    Api,
    /// Git management via lazygit running in a PTY tab.
    Git,
    /// A user-defined provider loaded from `providers.toml`.
    /// The string is the provider name (matches `ProviderConfig::name`).
    Custom(String),
}

impl AIProvider {
    /// CLI command to execute.
    /// For `Custom` providers, returns an empty string — use `ProviderManager` instead.
    pub fn command(&self) -> &str {
        match self {
            AIProvider::Shell => "/bin/sh",
            AIProvider::Kanban => "",
            AIProvider::CodeReview => "gh",
            AIProvider::Api => "",
            AIProvider::Git => "lazygit",
            AIProvider::Custom(_) => "",
        }
    }

    /// Resolved command: for Shell, use $SHELL env var with fallback.
    /// For `Custom` providers, returns an empty string — use `ProviderManager` instead.
    pub fn resolved_command(&self) -> String {
        match self {
            AIProvider::Shell => std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            other => other.command().to_string(),
        }
    }

    /// Icon prefix for sub-tab display.
    pub fn icon(&self) -> &str {
        match self {
            AIProvider::Shell => "▸",
            AIProvider::Kanban => "▦",
            AIProvider::CodeReview => "⊙",
            AIProvider::Api => "⚡",
            AIProvider::Git => "⎇",
            AIProvider::Custom(_) => "▸",
        }
    }

    /// Label for the sub-tab.
    /// For `Custom` providers, returns the provider name.
    pub fn label(&self) -> &str {
        match self {
            AIProvider::Shell => "Shell",
            AIProvider::Kanban => "Kanban Board",
            AIProvider::CodeReview => "Code Review",
            AIProvider::Api => "API Explorer",
            AIProvider::Git => "Git",
            AIProvider::Custom(name) => name,
        }
    }

    /// Built-in providers in display order (does not include Custom).
    pub fn all() -> &'static [AIProvider] {
        &[
            AIProvider::Shell,
            AIProvider::Kanban,
            AIProvider::CodeReview,
            AIProvider::Api,
        ]
    }

    /// Parse a provider from its label string.
    /// Unrecognized labels become `Custom(label)`.
    pub fn from_label(label: &str) -> AIProvider {
        match label {
            "Shell" => AIProvider::Shell,
            "Kanban Board" => AIProvider::Kanban,
            "Code Review" => AIProvider::CodeReview,
            "API Explorer" => AIProvider::Api,
            "Git" => AIProvider::Git,
            other => AIProvider::Custom(other.to_string()),
        }
    }

    /// CLI arguments to pass a prompt/task to this provider.
    /// For `Custom` providers, returns empty — use `ProviderManager::prompt_args()` instead.
    pub fn prompt_args(&self, _prompt: &str) -> Vec<String> {
        Vec::new()
    }

    /// Whether this is a built-in provider (not Custom).
    pub fn is_builtin(&self) -> bool {
        !matches!(self, AIProvider::Custom(_))
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

/// Type of workspace: backed by a git worktree, pointing to a directory, or a project root
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkspaceType {
    #[default]
    Worktree,
    Simple,
    Project,
}

/// Where the workspace's files originated.
///
/// `Local` covers any folder the user picked (regardless of whether it happens
/// to be inside a git repository). `GitHub { url }` covers workspaces that
/// `piki` cloned itself from a GitHub URL — only these support the
/// "Create Worktree" action (both creating a new worktree and loading one
/// that already exists on disk via `WorkspaceManager::list_worktrees`) and
/// the source-control panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "url")]
pub enum WorkspaceOrigin {
    #[default]
    Local,
    GitHub {
        url: String,
    },
}

impl WorkspaceOrigin {
    /// SQL `origin` column value: `"Local"` | `"GitHub"`.
    pub fn tag(&self) -> &'static str {
        match self {
            WorkspaceOrigin::Local => "Local",
            WorkspaceOrigin::GitHub { .. } => "GitHub",
        }
    }

    /// SQL `origin_github_url` column value (`None` for Local).
    pub fn github_url(&self) -> Option<&str> {
        match self {
            WorkspaceOrigin::Local => None,
            WorkspaceOrigin::GitHub { url } => Some(url.as_str()),
        }
    }

    /// Rebuild from the two SQL columns. Unknown tags or `GitHub` with a
    /// missing url fall back to `Local`.
    pub fn from_sql(tag: &str, github_url: Option<String>) -> Self {
        match (tag, github_url) {
            ("GitHub", Some(url)) => WorkspaceOrigin::GitHub { url },
            _ => WorkspaceOrigin::Local,
        }
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
}

/// Kind of a directory entry returned by
/// [`crate::workspace::manager::read_dir_entries`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

/// A single entry inside a directory (one level, non-recursive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    /// Milliseconds since the Unix epoch; 0 if unavailable.
    pub mtime: u64,
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
    /// Pre-computed display name for the source repo (avoids per-frame file_name() + to_string_lossy())
    #[serde(default)]
    pub source_repo_display: String,
    #[serde(default)]
    pub workspace_type: WorkspaceType,
    #[serde(default)]
    pub group: Option<String>,
    /// Persistent display order (lower values appear first)
    #[serde(default)]
    pub order: u32,
    /// Card ID from the kanban board that triggered this workspace dispatch
    #[serde(default)]
    pub dispatch_card_id: Option<String>,
    /// Kanban board path of the source workspace (for card lifecycle management)
    #[serde(default)]
    pub dispatch_source_kanban: Option<String>,
    /// Name of the agent profile used for dispatch
    #[serde(default)]
    pub dispatch_agent_name: Option<String>,
    /// Where the workspace files originated (local folder vs github clone).
    /// Drives source-control panel visibility and worktree-action availability.
    #[serde(default)]
    pub origin: WorkspaceOrigin,
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
        let source_repo_display = source_repo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| source_repo.to_string_lossy().to_string());
        Self {
            name,
            description,
            prompt,
            kanban_path,
            branch,
            path,
            source_repo,
            source_repo_display,
            workspace_type: WorkspaceType::default(),
            group: None,
            order: 0,
            dispatch_card_id: None,
            dispatch_source_kanban: None,
            dispatch_agent_name: None,
            origin: WorkspaceOrigin::default(),
        }
    }
}
