use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ratatui::text::Text;

use crate::pty::PtySession;
use crate::workspace::FileWatcher;

/// An AI assistant that can be run in a PTY
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AIProvider {
    Claude,
    Gemini,
    Codex,
    Shell,
}

impl AIProvider {
    /// CLI command to execute
    pub fn command(&self) -> &str {
        match self {
            AIProvider::Claude => "claude",
            AIProvider::Gemini => "gemini",
            AIProvider::Codex => "codex",
            AIProvider::Shell => "/bin/sh",
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
        }
    }

    /// All available providers in display order
    pub fn all() -> &'static [AIProvider] {
        &[AIProvider::Claude, AIProvider::Gemini, AIProvider::Codex, AIProvider::Shell]
    }
}

/// Main application mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Viewing PTY output of the active workspace
    Normal,
    /// Viewing a side-by-side diff
    Diff,
    /// Input dialog for creating a new workspace
    NewWorkspace,
    /// Confirmation dialog for deleting a workspace
    ConfirmDelete,
    /// Help overlay
    Help,
}

/// Which pane is currently selected / focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    WorkspaceList,
    FileList,
    MainPanel, // Terminal or Diff
}

/// Which field is active in the New Workspace dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogField {
    Name,
    Directory,
    Description,
}

/// Status of the Claude Code process in a workspace
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceStatus {
    Idle,
    Busy,
    Done,
    Error(String),
}

/// Git file change status
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
}

/// A single workspace backed by a git worktree
pub struct Workspace {
    pub name: String,
    pub description: String,
    pub branch: String,
    pub path: PathBuf,
    /// Git root of the source repository this workspace was created from
    pub source_repo: PathBuf,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    /// PTY sessions keyed by AI provider
    pub pty_sessions: HashMap<AIProvider, PtySession>,
    /// vt100 parsers keyed by AI provider
    pub pty_parsers: HashMap<AIProvider, Arc<Mutex<vt100::Parser>>>,
    /// Which AI provider sub-tab is currently active
    pub active_provider: AIProvider,
    pub watcher: Option<FileWatcher>,
    /// Whether the file list needs a refresh from git
    pub dirty: bool,
    /// Last time the file list was refreshed (for debounce)
    pub last_refresh: Option<Instant>,
    /// Scrollback offset: 0 = live view, N = N lines back from bottom
    pub term_scroll: usize,
    /// Last byte count from PTY for auto-scroll detection
    pub last_bytes_processed: u64,
}

impl Workspace {
    pub fn new(
        name: String,
        description: String,
        branch: String,
        path: PathBuf,
        source_repo: PathBuf,
    ) -> Self {
        Self {
            name,
            description,
            branch,
            path,
            source_repo,
            status: WorkspaceStatus::Idle,
            changed_files: Vec::new(),
            pty_sessions: HashMap::new(),
            pty_parsers: HashMap::new(),
            active_provider: AIProvider::Claude,
            watcher: None,
            dirty: false,
            last_refresh: None,
            term_scroll: 0,
            last_bytes_processed: 0,
        }
    }

    pub fn file_count(&self) -> usize {
        self.changed_files.len()
    }

    pub fn status_label(&self) -> &str {
        match &self.status {
            WorkspaceStatus::Idle => "idle",
            WorkspaceStatus::Busy => "busy",
            WorkspaceStatus::Done => "done",
            WorkspaceStatus::Error(_) => "error",
        }
    }

    /// Refresh the list of changed files by running `git diff --name-status HEAD`
    pub async fn refresh_changed_files(&mut self) -> anyhow::Result<()> {
        self.changed_files = get_changed_files(&self.path).await?;
        self.dirty = false;
        Ok(())
    }
}

/// Parse `git status --porcelain=v1` output into ChangedFile list.
///
/// Format: `XY path` where X = index status, Y = working tree status.
/// For renames: `XY old_path -> new_path`
pub fn parse_porcelain_status(output: &str) -> Vec<ChangedFile> {
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let x = line.as_bytes()[0];
            let y = line.as_bytes()[1];
            // Path starts at index 3 (after "XY ")
            let path_part = &line[3..];

            // Untracked
            if x == b'?' && y == b'?' {
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status: FileStatus::Untracked,
                });
            }

            // Conflicts: UU, AA, DD, AU, UA, DU, UD
            if x == b'U' || y == b'U' || (x == b'A' && y == b'A') || (x == b'D' && y == b'D') {
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status: FileStatus::Conflicted,
                });
            }

            // Renamed (in index)
            if x == b'R' {
                // Path format: "old_path -> new_path"
                let display_path = if let Some((_old, new)) = path_part.split_once(" -> ") {
                    new.to_string()
                } else {
                    path_part.to_string()
                };
                return Some(ChangedFile {
                    path: display_path,
                    status: FileStatus::Renamed,
                });
            }

            // Staged + modified in working tree
            if x != b' ' && x != b'?' && y == b'M' {
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status: FileStatus::StagedModified,
                });
            }

            // Staged only (index has changes, working tree clean)
            if y == b' ' {
                let status = match x {
                    b'M' => FileStatus::Staged,
                    b'A' => FileStatus::Added,
                    b'D' => FileStatus::Deleted,
                    _ => return None,
                };
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status,
                });
            }

            // Working tree changes only (unstaged)
            if x == b' ' {
                let status = match y {
                    b'M' => FileStatus::Modified,
                    b'D' => FileStatus::Deleted,
                    _ => return None,
                };
                return Some(ChangedFile {
                    path: path_part.to_string(),
                    status,
                });
            }

            None
        })
        .collect()
}

/// Run `git status --porcelain=v1` in a worktree and return changed files
pub async fn get_changed_files(worktree_path: &PathBuf) -> anyhow::Result<Vec<ChangedFile>> {
    let output = tokio::process::Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(worktree_path)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_porcelain_status(&stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_modified_unstaged() {
        let input = " M src/main.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::Modified);
    }

    #[test]
    fn test_parse_porcelain_staged() {
        let input = "M  src/main.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::Staged);
    }

    #[test]
    fn test_parse_porcelain_staged_modified() {
        let input = "MM src/main.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::StagedModified);
    }

    #[test]
    fn test_parse_porcelain_added() {
        let input = "A  src/new.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/new.rs");
        assert_eq!(files[0].status, FileStatus::Added);
    }

    #[test]
    fn test_parse_porcelain_deleted() {
        let input = " D old_file.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "old_file.rs");
        assert_eq!(files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn test_parse_porcelain_untracked() {
        let input = "?? new_file.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new_file.rs");
        assert_eq!(files[0].status, FileStatus::Untracked);
    }

    #[test]
    fn test_parse_porcelain_conflicts() {
        let input = "UU conflict.rs\nAA both_added.rs\nDD both_deleted.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, FileStatus::Conflicted);
        assert_eq!(files[1].status, FileStatus::Conflicted);
        assert_eq!(files[2].status, FileStatus::Conflicted);
    }

    #[test]
    fn test_parse_porcelain_renamed() {
        let input = "R  old.rs -> new.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].path, "new.rs");
    }

    #[test]
    fn test_parse_porcelain_empty() {
        let files = parse_porcelain_status("");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_porcelain_mixed() {
        let input = " M src/app.rs\nA  src/new.rs\n?? untracked.txt\nMM both.rs\nD  deleted.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[1].status, FileStatus::Added);
        assert_eq!(files[2].status, FileStatus::Untracked);
        assert_eq!(files[3].status, FileStatus::StagedModified);
        assert_eq!(files[4].status, FileStatus::Deleted);
    }

    #[test]
    fn test_parse_porcelain_malformed() {
        let input = "x\n\n M valid.rs\n";
        let files = parse_porcelain_status(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "valid.rs");
    }
}

/// Central application state
pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub active_pane: ActivePane,
    /// When true, keyboard input goes to the active pane; when false, hjkl navigates between panes
    pub interacting: bool,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub selected_workspace: usize,
    pub selected_file: usize,
    pub diff_scroll: u16,
    pub diff_content: Option<Text<'static>>,
    pub diff_file_path: Option<String>,
    pub input_buffer: String,
    pub dir_input_buffer: String,
    pub desc_input_buffer: String,
    pub active_dialog_field: DialogField,
    pub status_message: Option<String>,
    /// Index of workspace targeted for deletion (used by ConfirmDelete dialog)
    pub delete_target: Option<usize>,
    /// Current PTY dimensions (rows, cols) — updated on terminal resize
    pub pty_rows: u16,
    pub pty_cols: u16,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            mode: AppMode::Normal,
            active_pane: ActivePane::WorkspaceList,
            interacting: false,
            workspaces: Vec::new(),
            active_workspace: 0,
            selected_workspace: 0,
            selected_file: 0,
            diff_scroll: 0,
            diff_content: None,
            diff_file_path: None,
            input_buffer: String::new(),
            dir_input_buffer: String::new(),
            desc_input_buffer: String::new(),
            active_dialog_field: DialogField::Name,
            status_message: None,
            delete_target: None,
            pty_rows: 24,
            pty_cols: 80,
        }
    }

    pub fn current_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active_workspace)
    }

    pub fn current_workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active_workspace)
    }

    pub fn next_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.switch_workspace((self.active_workspace + 1) % self.workspaces.len());
        }
    }

    pub fn prev_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            let len = self.workspaces.len();
            self.switch_workspace((self.active_workspace + len - 1) % len);
        }
    }

    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.workspaces.len() {
            self.active_workspace = index;
            self.selected_workspace = index;
            self.selected_file = 0;
            self.mode = AppMode::Normal;
            self.diff_content = None;
            self.diff_file_path = None;
            self.workspaces[index].term_scroll = 0;
        }
    }

    pub fn next_file(&mut self) {
        if let Some(ws) = self.current_workspace() {
            if !ws.changed_files.is_empty() {
                self.selected_file = (self.selected_file + 1) % ws.changed_files.len();
            }
        }
    }

    pub fn prev_file(&mut self) {
        if let Some(ws) = self.current_workspace() {
            if !ws.changed_files.is_empty() {
                let len = ws.changed_files.len();
                self.selected_file = (self.selected_file + len - 1) % len;
            }
        }
    }

    pub fn select_next_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            self.selected_workspace = (self.selected_workspace + 1) % self.workspaces.len();
        }
    }

    pub fn select_prev_workspace(&mut self) {
        if !self.workspaces.is_empty() {
            let len = self.workspaces.len();
            self.selected_workspace = (self.selected_workspace + len - 1) % len;
        }
    }
}
