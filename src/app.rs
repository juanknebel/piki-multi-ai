use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::text::Text;

use crate::pty::PtySession;
use crate::theme::Theme;
use crate::workspace::FileWatcher;

/// An AI assistant that can be run in a PTY
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Main application mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Viewing PTY output of the active workspace
    Normal,
    /// Viewing a side-by-side diff
    Diff,
    /// Input dialog for creating a new workspace
    NewWorkspace,
    /// Input dialog for editing a workspace
    EditWorkspace,
    /// Confirmation dialog for deleting a workspace
    ConfirmDelete,
    /// Help overlay
    Help,
    /// Fuzzy file search overlay
    FuzzySearch,
    /// Inline file editor
    InlineEdit,
    /// Commit message input dialog
    CommitMessage,
    /// Merge confirmation dialog
    ConfirmMerge,
    /// New tab provider selection dialog
    NewTab,
    /// About overlay
    About,
    /// Workspace info overlay
    WorkspaceInfo,
    /// Confirmation dialog for quitting the application
    ConfirmQuit,
}

/// Strategy for merging a workspace branch into main
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    Merge,
    Rebase,
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
    Prompt,
    KanbanPath,
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

/// A tab within a workspace, each with its own PTY session
pub struct Tab {
    #[allow(dead_code)]
    pub id: usize,
    pub provider: AIProvider,
    pub pty_session: Option<PtySession>,
    pub pty_parser: Option<Arc<Mutex<vt100::Parser>>>,
    /// Whether this tab can be closed (first shell tab cannot)
    pub closable: bool,
    /// Scrollback offset: 0 = live view, N = N lines back from bottom
    pub term_scroll: usize,
    /// Last byte count from PTY for auto-scroll detection
    pub last_bytes_processed: u64,
    /// Markdown content (when this tab displays a markdown file instead of a PTY)
    pub markdown_content: Option<String>,
    /// Label for markdown tabs (filename)
    pub markdown_label: Option<String>,
    /// Scroll offset for markdown view
    pub markdown_scroll: u16,
}

/// A single workspace backed by a git worktree
pub struct Workspace {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub kanban_path: Option<String>,
    pub branch: String,
    pub path: PathBuf,
    /// Git root of the source repository this workspace was created from
    pub source_repo: PathBuf,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    /// Dynamic tabs, each with its own PTY session
    pub tabs: Vec<Tab>,
    /// Index of the currently active tab
    pub active_tab: usize,
    /// Counter for generating unique tab IDs
    pub next_tab_id: usize,
    pub watcher: Option<FileWatcher>,
    /// Whether the file list needs a refresh from git
    pub dirty: bool,
    /// Last time the file list was refreshed (for debounce)
    pub last_refresh: Option<Instant>,
    /// Commits ahead/behind upstream (ahead, behind)
    pub ahead_behind: Option<(usize, usize)>,
    /// Kanban app state
    pub kanban_app: Option<flow::App>,
    /// Kanban provider
    pub kanban_provider: Option<Box<dyn flow::provider::Provider>>,
}

impl Workspace {
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
            status: WorkspaceStatus::Idle,
            changed_files: Vec::new(),
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            watcher: None,
            dirty: false,
            last_refresh: None,
            ahead_behind: None,
            kanban_app: None,
            kanban_provider: None,
        }
    }

    /// Get the currently active tab
    pub fn current_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    /// Get the currently active tab mutably
    pub fn current_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Add a new tab and return its index
    pub fn add_tab(&mut self, provider: AIProvider, closable: bool) -> usize {
        let tab = Tab {
            id: self.next_tab_id,
            provider,
            pty_session: None,
            pty_parser: None,
            closable,
            term_scroll: 0,
            last_bytes_processed: 0,
            markdown_content: None,
            markdown_label: None,
            markdown_scroll: 0,
        };
        self.next_tab_id += 1;
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    /// Close a tab by index, returns true if closed
    pub fn close_tab(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() || !self.tabs[idx].closable {
            return false;
        }
        if let Some(ref mut pty) = self.tabs[idx].pty_session {
            let _ = pty.kill();
        }
        self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    /// Add a markdown viewer tab and return its index
    pub fn add_markdown_tab(&mut self, label: String, content: String) -> usize {
        let tab = Tab {
            id: self.next_tab_id,
            provider: AIProvider::Shell, // placeholder, not used for markdown
            pty_session: None,
            pty_parser: None,
            closable: true,
            term_scroll: 0,
            last_bytes_processed: 0,
            markdown_content: Some(content),
            markdown_label: Some(label),
            markdown_scroll: 0,
        };
        self.next_tab_id += 1;
        self.tabs.push(tab);
        let idx = self.tabs.len() - 1;
        self.active_tab = idx;
        idx
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
        self.ahead_behind = get_ahead_behind(&self.path).await;
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

/// Get ahead/behind counts relative to upstream.
/// Returns None if there's no upstream configured.
async fn get_ahead_behind(worktree_path: &PathBuf) -> Option<(usize, usize)> {
    let output = tokio::process::Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(worktree_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\t').collect();
    if parts.len() == 2 {
        let ahead = parts[0].parse().unwrap_or(0);
        let behind = parts[1].parse().unwrap_or(0);
        Some((ahead, behind))
    } else {
        None
    }
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

/// A fuzzy search match result
pub struct FuzzyMatch {
    pub path: String,
    pub score: u32,
    pub match_indices: Vec<u32>,
}

/// State for the fuzzy file search overlay
pub struct FuzzyState {
    pub query: String,
    pub all_files: Vec<String>,
    pub results: Vec<FuzzyMatch>,
    pub selected: usize,
}

/// State for the inline file editor
pub struct EditorState {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
}

impl EditorState {
    pub fn new(content: &str) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        Self {
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
        }
    }

    pub fn contents(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }

    pub fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte_idx(line, self.cursor_col);
        line.insert(byte_idx, c);
        self.cursor_col += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let byte_idx = char_to_byte_idx(line, self.cursor_col - 1);
            line.remove(byte_idx);
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            let removed = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].chars().count();
            self.lines[self.cursor_row].push_str(&removed);
        }
    }

    pub fn enter(&mut self) {
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte_idx(line, self.cursor_col);
        let rest = line[byte_idx..].to_string();
        line.truncate(byte_idx);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, rest);
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_col();
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.clamp_col();
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        }
    }

    fn clamp_col(&mut self) {
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col > line_len {
            self.cursor_col = line_len;
        }
    }

    /// Adjust scroll_offset so cursor is visible within `visible_height` lines
    pub fn adjust_scroll(&mut self, visible_height: usize) {
        if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        } else if self.cursor_row >= self.scroll_offset + visible_height {
            self.scroll_offset = self.cursor_row - visible_height + 1;
        }
    }
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub anchor_row: u16,
    pub anchor_col: u16,
    pub end_row: u16,
    pub end_col: u16,
    pub active: bool,
}

impl Selection {
    pub fn new(row: u16, col: u16) -> Self {
        Self {
            anchor_row: row,
            anchor_col: col,
            end_row: row,
            end_col: col,
            active: true,
        }
    }

    /// Returns (start_row, start_col, end_row, end_col) ordered top-left to bottom-right
    pub fn normalized(&self) -> (u16, u16, u16, u16) {
        if self.anchor_row < self.end_row
            || (self.anchor_row == self.end_row && self.anchor_col <= self.end_col)
        {
            (self.anchor_row, self.anchor_col, self.end_row, self.end_col)
        } else {
            (self.end_row, self.end_col, self.anchor_row, self.anchor_col)
        }
    }

    pub fn contains(&self, row: u16, col: u16) -> bool {
        let (sr, sc, er, ec) = self.normalized();
        if row < sr || row > er {
            return false;
        }
        if sr == er {
            return col >= sc && col <= ec;
        }
        if row == sr {
            return col >= sc;
        }
        if row == er {
            return col <= ec;
        }
        true
    }
}

/// Central application state
/// Which border is being dragged for resize
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeDrag {
    /// Vertical border between left sidebar and main panel
    Sidebar,
    /// Horizontal border between workspace list and file list
    LeftSplit,
}

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
    pub prompt_input_buffer: String,
    pub kanban_input_buffer: String,
    /// Cursor positions (char index) for each dialog input field
    pub input_cursor: usize,
    pub dir_input_cursor: usize,
    pub desc_input_cursor: usize,
    pub prompt_input_cursor: usize,
    pub kanban_input_cursor: usize,
    /// Scroll offset for help overlay
    pub help_scroll: u16,
    /// Horizontal scroll offset for workspace info overlay
    pub info_hscroll: u16,
    pub active_dialog_field: DialogField,
    pub status_message: Option<String>,
    /// Index of workspace targeted for deletion (used by ConfirmDelete dialog)
    pub delete_target: Option<usize>,
    /// Index of workspace targeted for editing (used by EditWorkspace dialog)
    pub edit_target: Option<usize>,
    /// Fuzzy file search state
    pub fuzzy: Option<FuzzyState>,
    /// Inline editor state
    pub editor: Option<EditorState>,
    /// Path of the file being edited inline
    pub editing_file: Option<PathBuf>,
    /// Current PTY dimensions (rows, cols) — updated on terminal resize
    pub pty_rows: u16,
    pub pty_cols: u16,
    pub theme: Theme,
    pub selection: Option<Selection>,
    pub terminal_inner_area: Option<Rect>,
    /// Commit message buffer (for git commit dialog)
    pub commit_msg_buffer: String,
    /// System info (CPU, RAM, battery, time)
    pub sysinfo: std::sync::Arc<std::sync::Mutex<crate::sysinfo::SystemInfo>>,
    /// Sidebar width as percentage (10..=90)
    pub sidebar_pct: u16,
    /// Left panel vertical split: workspace list percentage (10..=90)
    pub left_split_pct: u16,
    /// Mouse drag-resize state
    pub resize_drag: Option<ResizeDrag>,
    /// X coordinate of the vertical border between sidebar and main panel
    pub sidebar_x: u16,
    /// Y coordinate of the horizontal border between workspace list and file list
    pub left_split_y: u16,
    /// Rect of the left sidebar area (for resize calculations)
    pub left_area_rect: Rect,
    pub config: crate::config::Config,
    /// Last left-click position and time (for double-click detection)
    pub last_click: Option<(Instant, u16, u16)>,
    /// Layout areas for mouse hit-testing
    pub ws_list_area: Rect,
    pub file_list_area: Rect,
    pub tabs_area: Rect,
    pub subtabs_area: Rect,
    pub main_content_area: Rect,
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
            prompt_input_buffer: String::new(),
            kanban_input_buffer: String::new(),
            input_cursor: 0,
            dir_input_cursor: 0,
            desc_input_cursor: 0,
            prompt_input_cursor: 0,
            kanban_input_cursor: 0,
            help_scroll: 0,
            info_hscroll: 0,
            active_dialog_field: DialogField::Name,
            status_message: None,
            delete_target: None,
            edit_target: None,
            fuzzy: None,
            editor: None,
            editing_file: None,
            pty_rows: 24,
            pty_cols: 80,
            theme: Theme::default(),
            selection: None,
            terminal_inner_area: None,
            commit_msg_buffer: String::new(),
            sysinfo: std::sync::Arc::new(std::sync::Mutex::new(crate::sysinfo::SystemInfo::default())),
            sidebar_pct: 20,
            left_split_pct: 50,
            resize_drag: None,
            sidebar_x: 0,
            left_split_y: 0,
            left_area_rect: Rect::default(),
            config: crate::config::Config::load(),
            last_click: None,
            ws_list_area: Rect::default(),
            file_list_area: Rect::default(),
            tabs_area: Rect::default(),
            subtabs_area: Rect::default(),
            main_content_area: Rect::default(),
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
            self.selection = None;
            if let Some(tab) = self.workspaces[index].current_tab_mut() {
                tab.term_scroll = 0;
            }
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

    /// Open the fuzzy file search overlay by scanning all files in the active worktree
    pub fn open_fuzzy_search(&mut self) {
        let worktree_path = match self.current_workspace() {
            Some(ws) => ws.path.clone(),
            None => {
                self.status_message = Some("No active workspace".into());
                return;
            }
        };

        let mut all_files = Vec::new();
        let walker = ignore::WalkBuilder::new(&worktree_path)
            .git_ignore(true)
            .build();
        for entry in walker.flatten() {
            if entry.file_type().is_some_and(|ft| ft.is_file())
                && let Ok(rel) = entry.path().strip_prefix(&worktree_path)
            {
                all_files.push(rel.to_string_lossy().to_string());
            }
        }
        all_files.sort();

        let results: Vec<FuzzyMatch> = all_files
            .iter()
            .map(|p| FuzzyMatch {
                path: p.clone(),
                score: 0,
                match_indices: Vec::new(),
            })
            .collect();

        self.fuzzy = Some(FuzzyState {
            query: String::new(),
            all_files,
            results,
            selected: 0,
        });
        self.mode = AppMode::FuzzySearch;
    }

    /// Re-filter fuzzy search results based on the current query
    pub fn update_fuzzy_filter(&mut self) {
        if let Some(ref mut state) = self.fuzzy {
            if state.query.is_empty() {
                state.results = state
                    .all_files
                    .iter()
                    .map(|p| FuzzyMatch {
                        path: p.clone(),
                        score: 0,
                        match_indices: Vec::new(),
                    })
                    .collect();
            } else {
                use nucleo::pattern::{CaseMatching, Pattern};
                let pattern = Pattern::parse(&state.query, CaseMatching::Smart);
                let mut matcher = nucleo::Matcher::default();
                let mut buf = Vec::new();
                let mut results: Vec<FuzzyMatch> = state
                    .all_files
                    .iter()
                    .filter_map(|path| {
                        let haystack = nucleo::Utf32Str::new(path, &mut buf);
                        let mut indices = Vec::new();
                        pattern
                            .indices(haystack, &mut matcher, &mut indices)
                            .map(|score| {
                                indices.sort_unstable();
                                indices.dedup();
                                FuzzyMatch {
                                    path: path.clone(),
                                    score,
                                    match_indices: indices,
                                }
                            })
                    })
                    .collect();
                results.sort_by(|a, b| b.score.cmp(&a.score));
                state.results = results;
            }
            // Clamp selection
            if state.results.is_empty() {
                state.selected = 0;
            } else if state.selected >= state.results.len() {
                state.selected = state.results.len() - 1;
            }
        }
    }

    /// Open the inline editor for a file
    pub fn open_inline_editor(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.editor = Some(EditorState::new(&content));
                self.editing_file = Some(path);
                self.mode = AppMode::InlineEdit;
            }
            Err(e) => {
                self.status_message = Some(format!("Cannot read file: {}", e));
            }
        }
    }
}
