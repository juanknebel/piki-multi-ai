use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::text::Text;

// Re-export domain types from core for convenience
pub use piki_core::git::{get_ahead_behind, get_changed_files};
pub use piki_core::pty::PtySession;
pub use piki_core::workspace::FileWatcher;
pub use piki_core::{AIProvider, ChangedFile, FileStatus, WorkspaceStatus};

use crate::theme::Theme;

/// Result of an async git refresh for a workspace
pub struct RefreshResult {
    pub workspace_idx: usize,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
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
    /// Confirmation dialog for closing a tab
    ConfirmCloseTab,
    /// Confirmation dialog for quitting the application
    ConfirmQuit,
}

/// Which pane is currently selected / focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    WorkspaceList,
    GitStatus,
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
    /// Cached parsed markdown (avoids re-parsing every frame)
    pub markdown_rendered: Option<Text<'static>>,
}

/// A single workspace backed by a git worktree
pub struct Workspace {
    /// Core workspace metadata (shared with other frontends)
    pub info: piki_core::WorkspaceInfo,
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

impl std::ops::Deref for Workspace {
    type Target = piki_core::WorkspaceInfo;
    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl std::ops::DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.info
    }
}

impl Workspace {
    /// Create from a WorkspaceInfo (e.g. returned by WorkspaceManager::create)
    pub fn from_info(info: piki_core::WorkspaceInfo) -> Self {
        Self {
            info,
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
            markdown_rendered: None,
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
        let rendered = crate::ui::markdown::parse_to_static(&content);
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
            markdown_rendered: Some(rendered),
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
        self.changed_files = get_changed_files(&self.info.path).await?;
        self.ahead_behind = get_ahead_behind(&self.info.path).await;
        self.dirty = false;
        Ok(())
    }
}

/// A fuzzy search match result
pub struct FuzzyMatch {
    /// Index into FuzzyState.all_files (avoids cloning path strings)
    pub path_idx: usize,
    pub score: u32,
    pub match_indices: Vec<u32>,
}

/// State for the fuzzy file search overlay
pub struct FuzzyState {
    pub query: String,
    pub all_files: Vec<String>,
    pub results: Vec<FuzzyMatch>,
    pub selected: usize,
    /// Whether the filter needs to be re-applied (set on keystroke, cleared on tick)
    pub filter_stale: bool,
}

impl FuzzyState {
    /// Get the path of the currently selected result
    pub fn selected_path(&self) -> Option<&str> {
        self.results
            .get(self.selected)
            .map(|m| self.all_files[m.path_idx].as_str())
    }

    /// Get the path for a given result
    pub fn result_path(&self, result: &FuzzyMatch) -> &str {
        &self.all_files[result.path_idx]
    }
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
    pub diff_content: Option<Arc<Text<'static>>>,
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
    /// Index of tab targeted for closing (used by ConfirmCloseTab dialog)
    pub close_tab_target: Option<usize>,
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
    /// Pre-formatted system info string (CPU, RAM, battery, time)
    pub sysinfo: std::sync::Arc<parking_lot::Mutex<String>>,
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
    /// Whether the UI needs to be redrawn
    pub needs_redraw: bool,
    pub config: crate::config::Config,
    /// Channel for receiving async git refresh results
    pub refresh_tx: tokio::sync::mpsc::UnboundedSender<RefreshResult>,
    pub refresh_rx: tokio::sync::mpsc::UnboundedReceiver<RefreshResult>,
    /// Channel for receiving status messages from background tasks
    pub status_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub status_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    /// Whether a background git refresh is in-flight
    pub refresh_pending: bool,
    /// Handle for async fuzzy search file scanning
    pub fuzzy_scan_handle: Option<tokio::task::JoinHandle<Vec<String>>>,
    /// Last left-click position and time (for double-click detection)
    pub last_click: Option<(Instant, u16, u16)>,
    /// Layout areas for mouse hit-testing
    pub ws_list_area: Rect,
    pub file_list_area: Rect,
    pub tabs_area: Rect,
    pub subtabs_area: Rect,
    pub main_content_area: Rect,
    /// Cache for rendered diff output, keyed by file path (LRU eviction)
    pub diff_cache: lru::LruCache<String, Arc<Text<'static>>>,
    /// Cached footer keys: (mode, interacting, active_pane, has_markdown) → keys
    pub footer_cache: Option<(AppMode, bool, ActivePane, bool, Vec<(String, &'static str)>)>,
    /// Last time inactive workspace PTYs were checked for exit
    pub last_inactive_pty_check: Instant,
}

impl App {
    pub fn new() -> Self {
        let (refresh_tx, refresh_rx) = tokio::sync::mpsc::unbounded_channel::<RefreshResult>();
        let (status_tx, status_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
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
            close_tab_target: None,
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
            sysinfo: std::sync::Arc::new(parking_lot::Mutex::new(String::new())),
            sidebar_pct: 20,
            left_split_pct: 50,
            resize_drag: None,
            sidebar_x: 0,
            left_split_y: 0,
            left_area_rect: Rect::default(),
            needs_redraw: true,
            config: crate::config::Config::load(),
            refresh_tx,
            refresh_rx,
            status_tx,
            status_rx,
            refresh_pending: false,
            fuzzy_scan_handle: None,
            last_click: None,
            ws_list_area: Rect::default(),
            file_list_area: Rect::default(),
            tabs_area: Rect::default(),
            subtabs_area: Rect::default(),
            main_content_area: Rect::default(),
            diff_cache: lru::LruCache::new(std::num::NonZeroUsize::new(32).unwrap()),
            footer_cache: None,
            last_inactive_pty_check: Instant::now(),
        }
    }

    /// Insert a diff into the cache (LRU eviction handles size limit automatically).
    pub fn insert_diff_cache(&mut self, key: String, value: Arc<Text<'static>>) {
        self.diff_cache.put(key, value);
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
            // Trigger immediate background refresh for the new workspace
            self.workspaces[index].dirty = true;
            self.workspaces[index].last_refresh = None;
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

    /// Open the fuzzy file search overlay by scanning all files in the active worktree.
    /// The file scan runs asynchronously — the overlay opens immediately with an empty list.
    pub fn open_fuzzy_search(&mut self) {
        let worktree_path = match self.current_workspace() {
            Some(ws) => ws.info.path.clone(),
            None => {
                self.status_message = Some("No active workspace".into());
                return;
            }
        };

        self.fuzzy = Some(FuzzyState {
            query: String::new(),
            all_files: Vec::new(),
            results: Vec::new(),
            selected: 0,
            filter_stale: false,
        });
        self.mode = AppMode::FuzzySearch;

        // Spawn async file scan
        self.fuzzy_scan_handle = Some(tokio::task::spawn_blocking(move || {
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
            all_files
        }));
    }

    /// Re-filter fuzzy search results based on the current query
    pub fn update_fuzzy_filter(&mut self) {
        if let Some(ref mut state) = self.fuzzy {
            if state.query.is_empty() {
                state.results = (0..state.all_files.len())
                    .map(|i| FuzzyMatch {
                        path_idx: i,
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
                    .enumerate()
                    .filter_map(|(idx, path)| {
                        let haystack = nucleo::Utf32Str::new(path, &mut buf);
                        let mut indices = Vec::new();
                        pattern
                            .indices(haystack, &mut matcher, &mut indices)
                            .map(|score| {
                                indices.sort_unstable();
                                indices.dedup();
                                FuzzyMatch {
                                    path_idx: idx,
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
