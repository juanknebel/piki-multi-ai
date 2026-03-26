use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::text::Text;

// Re-export domain types from core for convenience
pub use piki_core::git::{get_ahead_behind, get_changed_files};
pub use piki_core::pty::PtySession;
pub use piki_core::workspace::FileWatcher;
pub use piki_core::{AIProvider, ChangedFile, FileStatus, WorkspaceStatus, WorkspaceType};

use crate::dialog_state::DialogState;
use crate::theme::Theme;

/// Toast notification level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Error,
}

/// A timed notification message
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    fn new(message: String, level: ToastLevel) -> Self {
        let duration = match level {
            ToastLevel::Error => Duration::from_secs(5),
            _ => Duration::from_secs(3),
        };
        Self {
            message,
            level,
            created_at: Instant::now(),
            duration,
        }
    }

    /// Whether this toast has expired
    pub fn expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }
}

/// An undo-able git stage/unstage action
#[derive(Debug, Clone)]
pub enum UndoAction {
    Stage,
    Unstage,
}

/// Entry in the undo stack
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub action: UndoAction,
    pub workspace_idx: usize,
    pub file_path: String,
}

/// Result of an async git refresh for a workspace
pub struct RefreshResult {
    pub workspace_idx: usize,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
    /// Sub-directories for Project workspaces (when set, replaces sub_directories instead of changed_files)
    pub sub_directories: Option<Vec<String>>,
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
    /// Workspace dashboard overview
    Dashboard,
    /// Internal log viewer
    Logs,
    /// Command palette overlay
    CommandPalette,
    /// Submit review overlay (code review)
    SubmitReview,
    /// Fuzzy workspace switcher overlay
    WorkspaceSwitcher,
    /// Git log overlay
    GitLog,
    /// Git stash overlay
    GitStash,
    /// Conflict resolution overlay
    ConflictResolution,
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
    Type,
    Name,
    Directory,
    Description,
    Prompt,
    KanbanPath,
    Group,
}

/// An item in the sidebar workspace list
#[derive(Debug, Clone)]
pub enum SidebarItem {
    GroupHeader {
        name: String,
        count: usize,
        collapsed: bool,
    },
    Workspace {
        index: usize,
    },
}

/// Response data for the API Explorer tab
#[allow(dead_code)]
pub struct ApiResponseDisplay {
    pub status: u16,
    pub elapsed_ms: u128,
    pub body: String,
    pub headers: String,
}

/// Search state for the API response panel
pub struct ApiSearchState {
    pub query: String,
    pub cursor: usize,
    /// Match positions as (line_index in rendered body, col)
    pub matches: Vec<(usize, usize)>,
    /// Index into `matches` for the currently highlighted match
    pub current_match: usize,
}

/// State for the API history overlay
pub struct ApiHistoryState {
    pub entries: Vec<piki_core::storage::ApiHistoryEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search_query: String,
    pub searching: bool,
}

/// State for the API Explorer tab
pub struct ApiTabState {
    pub editor: EditorState,
    pub responses: Vec<ApiResponseDisplay>,
    pub loading: bool,
    pub response_scroll: u16,
    pub pending_responses: Arc<Mutex<Option<Vec<ApiResponseDisplay>>>>,
    /// Search overlay for the response panel (None = closed)
    pub search: Option<ApiSearchState>,
    /// History overlay (None = closed)
    pub history: Option<ApiHistoryState>,
}

impl ApiTabState {
    pub fn new() -> Self {
        Self {
            editor: EditorState::new(""),
            responses: Vec::new(),
            loading: false,
            response_scroll: 0,
            pending_responses: Arc::new(Mutex::new(None)),
            search: None,
            history: None,
        }
    }
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
    /// API Explorer state (when this tab is an API Explorer)
    pub api_state: Option<ApiTabState>,
}

/// A single workspace backed by a git worktree
pub struct Workspace {
    /// Core workspace metadata (shared with other frontends)
    pub info: piki_core::WorkspaceInfo,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    /// Sub-directories for Project workspaces
    pub sub_directories: Vec<String>,
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
    /// Code review state
    pub code_review: Option<crate::code_review::CodeReviewState>,
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
            sub_directories: Vec::new(),
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            watcher: None,
            dirty: false,
            last_refresh: None,
            ahead_behind: None,
            kanban_app: None,
            kanban_provider: None,
            code_review: None,
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
            api_state: None,
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
    pub fn add_markdown_tab(
        &mut self,
        label: String,
        content: String,
        syntax_hl: Option<&crate::syntax::SyntaxHighlighter>,
    ) -> usize {
        let rendered = crate::ui::markdown::parse_to_static(&content, syntax_hl);
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
            api_state: None,
        };
        self.next_tab_id += 1;
        self.tabs.push(tab);
        let idx = self.tabs.len() - 1;
        self.active_tab = idx;
        idx
    }

    pub fn file_count(&self) -> usize {
        if self.info.workspace_type == WorkspaceType::Project {
            self.sub_directories.len()
        } else {
            self.changed_files.len()
        }
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

    /// Refresh the list of immediate sub-directories (for Project workspaces).
    pub async fn refresh_sub_directories(&mut self) {
        let path = self.info.path.clone();
        let mut dirs = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir(&path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(ft) = entry.file_type().await
                    && ft.is_dir()
                    && let Some(name) = entry.file_name().to_str()
                    && !name.starts_with('.')
                {
                    dirs.push(name.to_string());
                }
            }
        }
        dirs.sort();
        self.sub_directories = dirs;
        self.dirty = false;
    }
}

/// State for the fuzzy file search overlay (backed by nucleo async matcher)
pub struct FuzzyState {
    pub query: String,
    pub nucleo: nucleo::Nucleo<String>,
    pub selected: usize,
}

impl FuzzyState {
    /// Get the path of the currently selected result
    pub fn selected_path(&self) -> Option<&str> {
        self.nucleo
            .snapshot()
            .get_matched_item(self.selected as u32)
            .map(|item| item.data.as_str())
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

/// Terminal search overlay state
pub struct TermSearchState {
    pub query: String,
    pub cursor: usize,
    /// Match positions as (row, col) pairs in screen coordinates
    pub matches: Vec<(usize, usize)>,
    /// Index into `matches` for the currently highlighted match
    pub current_match: usize,
}

/// Cached footer keys: (mode, interacting, active_pane, has_markdown, api_footer_state, new_tab_menu, keys)
/// api_footer_state: 0 = no API tab, 1 = API tab, 2 = API tab with search open
/// new_tab_menu: 0 = N/A, 1 = Main, 2 = Agents, 3 = Tools
pub type FooterCache = (
    AppMode,
    bool,
    ActivePane,
    bool,
    u8,
    u8,
    Vec<(String, &'static str)>,
);

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
    /// Active dialog state — None means no dialog is open
    pub active_dialog: Option<DialogState>,
    pub collapsed_groups: std::collections::HashSet<String>,
    pub selected_sidebar_row: usize,
    pub status_message: Option<String>,
    /// Toast notification (replaces status_message for timed display)
    pub toast: Option<Toast>,
    /// Fuzzy file search state
    pub fuzzy: Option<FuzzyState>,
    /// Command palette state
    pub command_palette: Option<crate::command_palette::CommandPaletteState>,
    /// Workspace switcher state (fuzzy search over workspaces)
    pub workspace_switcher: Option<crate::workspace_switcher::WorkspaceSwitcherState>,
    /// Previous workspace index for quick toggle (backtick)
    pub previous_workspace: Option<usize>,
    /// Inline editor state
    pub editor: Option<EditorState>,
    /// Path of the file being edited inline
    pub editing_file: Option<PathBuf>,
    /// Current PTY dimensions (rows, cols) — updated on terminal resize
    pub pty_rows: u16,
    pub pty_cols: u16,
    pub theme: Theme,
    pub syntax: crate::syntax::SyntaxHighlighter,
    pub selection: Option<Selection>,
    pub terminal_inner_area: Option<Rect>,
    /// Inner area of the API response panel (for mouse hit-testing)
    pub api_response_inner_area: Option<Rect>,
    /// In-memory log ring buffer for the log viewer overlay
    pub log_buffer: crate::log_buffer::LogBuffer,
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
    /// Channel for receiving completed undo entries from background git tasks
    pub undo_tx: tokio::sync::mpsc::UnboundedSender<UndoEntry>,
    pub undo_rx: tokio::sync::mpsc::UnboundedReceiver<UndoEntry>,
    /// Undo stack (max 20 entries)
    pub undo_stack: std::collections::VecDeque<UndoEntry>,
    /// Terminal search overlay (None = closed)
    pub term_search: Option<TermSearchState>,
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
    pub footer_cache: Option<FooterCache>,
    /// Last time inactive workspace PTYs were checked for exit
    pub last_inactive_pty_check: Instant,
    /// Cached result of `gh` CLI availability check (None = not yet checked)
    pub gh_available: Option<bool>,
    /// Storage backend (SQLite)
    pub storage: std::sync::Arc<piki_core::storage::AppStorage>,
}

impl App {
    pub fn new(
        storage: std::sync::Arc<piki_core::storage::AppStorage>,
        paths: &piki_core::paths::DataPaths,
    ) -> Self {
        let (refresh_tx, refresh_rx) = tokio::sync::mpsc::unbounded_channel::<RefreshResult>();
        let (status_tx, status_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (undo_tx, undo_rx) = tokio::sync::mpsc::unbounded_channel::<UndoEntry>();
        let config = crate::config::Config::load_from(paths);
        let syntax = crate::syntax::SyntaxHighlighter::new(&config.syntax_theme);
        Self {
            should_quit: false,
            mode: AppMode::Normal,
            active_pane: ActivePane::WorkspaceList,
            interacting: false,
            log_buffer: crate::log_buffer::new_buffer(),
            workspaces: Vec::new(),
            active_workspace: 0,
            selected_workspace: 0,
            selected_file: 0,
            diff_scroll: 0,
            diff_content: None,
            diff_file_path: None,
            active_dialog: None,
            collapsed_groups: std::collections::HashSet::new(),
            selected_sidebar_row: 0,
            status_message: None,
            toast: None,
            fuzzy: None,
            command_palette: None,
            workspace_switcher: None,
            previous_workspace: None,
            editor: None,
            editing_file: None,
            pty_rows: 24,
            pty_cols: 80,
            theme: Theme::default(),
            syntax,
            selection: None,
            terminal_inner_area: None,
            api_response_inner_area: None,
            sysinfo: std::sync::Arc::new(parking_lot::Mutex::new(String::new())),
            sidebar_pct: 20,
            left_split_pct: 50,
            resize_drag: None,
            sidebar_x: 0,
            left_split_y: 0,
            left_area_rect: Rect::default(),
            needs_redraw: true,
            config,
            refresh_tx,
            refresh_rx,
            status_tx,
            status_rx,
            refresh_pending: false,
            undo_tx,
            undo_rx,
            undo_stack: std::collections::VecDeque::new(),
            term_search: None,
            last_click: None,
            ws_list_area: Rect::default(),
            file_list_area: Rect::default(),
            tabs_area: Rect::default(),
            subtabs_area: Rect::default(),
            main_content_area: Rect::default(),
            diff_cache: lru::LruCache::new(std::num::NonZeroUsize::new(32).unwrap()),
            footer_cache: None,
            last_inactive_pty_check: Instant::now(),
            gh_available: None,
            storage,
        }
    }

    /// Persist layout preferences (sidebar_pct, left_split_pct) to storage if available.
    pub fn save_layout_prefs(&self) {
        if let Some(ref ui_prefs) = self.storage.ui_prefs {
            let _ = ui_prefs.set_preference("sidebar_pct", &self.sidebar_pct.to_string());
            let _ = ui_prefs.set_preference("left_split_pct", &self.left_split_pct.to_string());
        }
    }

    /// Insert a diff into the cache (LRU eviction handles size limit automatically).
    pub fn insert_diff_cache(&mut self, key: String, value: Arc<Text<'static>>) {
        self.diff_cache.put(key, value);
    }

    /// Set a toast notification, replacing any existing one.
    pub fn set_toast(&mut self, message: impl Into<String>, level: ToastLevel) {
        self.toast = Some(Toast::new(message.into(), level));
        // Also keep status_message in sync for backward compatibility
        self.status_message = self.toast.as_ref().map(|t| t.message.clone());
    }

    /// Expire the toast if its duration has passed. Returns true if expired.
    pub fn expire_toast(&mut self) -> bool {
        if self.toast.as_ref().is_some_and(|t| t.expired()) {
            self.toast = None;
            self.status_message = None;
            return true;
        }
        false
    }

    pub fn current_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active_workspace)
    }

    pub fn current_workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active_workspace)
    }

    pub fn next_workspace(&mut self) {
        let visible: Vec<usize> = self
            .sidebar_items()
            .iter()
            .filter_map(|item| match item {
                SidebarItem::Workspace { index } => Some(*index),
                _ => None,
            })
            .collect();
        if visible.is_empty() {
            return;
        }
        let pos = visible
            .iter()
            .position(|&i| i == self.active_workspace)
            .unwrap_or(0);
        let next = visible[(pos + 1) % visible.len()];
        self.switch_workspace(next);
    }

    pub fn prev_workspace(&mut self) {
        let visible: Vec<usize> = self
            .sidebar_items()
            .iter()
            .filter_map(|item| match item {
                SidebarItem::Workspace { index } => Some(*index),
                _ => None,
            })
            .collect();
        if visible.is_empty() {
            return;
        }
        let pos = visible
            .iter()
            .position(|&i| i == self.active_workspace)
            .unwrap_or(0);
        let prev = visible[(pos + visible.len() - 1) % visible.len()];
        self.switch_workspace(prev);
    }

    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.workspaces.len() {
            if index != self.active_workspace {
                self.previous_workspace = Some(self.active_workspace);
            }
            self.active_workspace = index;
            self.selected_workspace = index;
            self.selected_file = 0;
            self.sync_sidebar_row(index);
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
            let count = ws.file_count();
            if count > 0 {
                self.selected_file = (self.selected_file + 1) % count;
            }
        }
    }

    pub fn prev_file(&mut self) {
        if let Some(ws) = self.current_workspace() {
            let count = ws.file_count();
            if count > 0 {
                self.selected_file = (self.selected_file + count - 1) % count;
            }
        }
    }

    /// Build the visual sidebar item list, grouping workspaces by their group field.
    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let mut items = Vec::new();
        let mut groups: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        let mut ungrouped = Vec::new();

        for (i, ws) in self.workspaces.iter().enumerate() {
            if let Some(ref group) = ws.info.group {
                groups.entry(group.clone()).or_default().push(i);
            } else {
                ungrouped.push(i);
            }
        }

        for idx in ungrouped {
            items.push(SidebarItem::Workspace { index: idx });
        }

        for (name, indices) in &groups {
            let collapsed = self.collapsed_groups.contains(name);
            items.push(SidebarItem::GroupHeader {
                name: name.clone(),
                count: indices.len(),
                collapsed,
            });
            if !collapsed {
                for &idx in indices {
                    items.push(SidebarItem::Workspace { index: idx });
                }
            }
        }

        items
    }

    /// Map a sidebar visual row to a workspace index.
    pub fn sidebar_row_to_workspace(&self, row: usize) -> Option<usize> {
        self.sidebar_items().get(row).and_then(|item| match item {
            SidebarItem::Workspace { index } => Some(*index),
            _ => None,
        })
    }

    pub fn select_next_sidebar_row(&mut self) {
        let count = self.sidebar_items().len();
        if count > 0 {
            self.selected_sidebar_row = (self.selected_sidebar_row + 1) % count;
            if let Some(idx) = self.sidebar_row_to_workspace(self.selected_sidebar_row) {
                self.selected_workspace = idx;
            }
        }
    }

    pub fn select_prev_sidebar_row(&mut self) {
        let count = self.sidebar_items().len();
        if count > 0 {
            self.selected_sidebar_row = (self.selected_sidebar_row + count - 1) % count;
            if let Some(idx) = self.sidebar_row_to_workspace(self.selected_sidebar_row) {
                self.selected_workspace = idx;
            }
        }
    }

    pub fn toggle_selected_group(&mut self) {
        let items = self.sidebar_items();
        let name = match items.get(self.selected_sidebar_row) {
            Some(SidebarItem::GroupHeader { name, .. }) => name.clone(),
            _ => return,
        };
        if !self.collapsed_groups.remove(&name) {
            self.collapsed_groups.insert(name);
        }
        // Persist to storage if available
        if let Some(ref ui_prefs) = self.storage.ui_prefs {
            let _ = ui_prefs.set_collapsed_groups(&self.collapsed_groups);
        }
    }

    /// Update selected_sidebar_row to point to the given workspace index.
    pub fn sync_sidebar_row(&mut self, ws_idx: usize) {
        let items = self.sidebar_items();
        for (i, item) in items.iter().enumerate() {
            if let SidebarItem::Workspace { index } = item
                && *index == ws_idx
            {
                self.selected_sidebar_row = i;
                return;
            }
        }
    }

    /// Open the conflict resolution overlay for the active workspace.
    /// Open conflict resolution overlay with detected conflicts.
    /// Called from Action::DetectConflicts after async git status scan.
    pub fn open_conflict_resolution_with(
        &mut self,
        conflicts: Vec<crate::dialog_state::ConflictFile>,
        repo_path: std::path::PathBuf,
    ) {
        if conflicts.is_empty() {
            self.set_toast("No conflicts detected", ToastLevel::Info);
            return;
        }
        self.active_dialog = Some(DialogState::ConflictResolution {
            files: conflicts,
            selected: 0,
            repo_path,
        });
        self.mode = AppMode::ConflictResolution;
    }

    /// Open the fuzzy file search overlay by scanning all files in the active worktree.
    /// Uses nucleo's async matcher — results appear incrementally as the walker discovers files.
    pub fn open_fuzzy_search(&mut self) {
        let worktree_path = match self.current_workspace() {
            Some(ws) => ws.info.path.clone(),
            None => {
                self.status_message = Some("No active workspace".into());
                return;
            }
        };

        let nucleo = nucleo::Nucleo::new(nucleo::Config::DEFAULT, Arc::new(|| {}), Some(1), 1);
        let injector = nucleo.injector();

        self.fuzzy = Some(FuzzyState {
            query: String::new(),
            nucleo,
            selected: 0,
        });
        self.mode = AppMode::FuzzySearch;

        // Spawn file walker — injects items incrementally as they're found
        tokio::task::spawn_blocking(move || {
            let walker = ignore::WalkBuilder::new(&worktree_path)
                .git_ignore(true)
                .build();
            for entry in walker.flatten() {
                if entry.file_type().is_some_and(|ft| ft.is_file())
                    && let Ok(rel) = entry.path().strip_prefix(&worktree_path)
                {
                    let path = rel.to_string_lossy().to_string();
                    let col: nucleo::Utf32String = path.as_str().into();
                    injector.push(path, |cols| {
                        cols[0] = col;
                    });
                }
            }
        });
    }

    /// Open the command palette overlay.
    pub fn open_command_palette(&mut self) {
        self.command_palette = Some(crate::command_palette::create_state(&self.workspaces));
        self.mode = AppMode::CommandPalette;
    }

    /// Toggle to the previously active workspace (Alt-Tab equivalent).
    pub fn toggle_previous_workspace(&mut self) {
        if let Some(prev) = self.previous_workspace
            && prev < self.workspaces.len()
        {
            self.switch_workspace(prev);
        }
    }

    /// Open the fuzzy workspace switcher overlay.
    pub fn open_workspace_switcher(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        self.workspace_switcher = Some(crate::workspace_switcher::create_state(&self.workspaces));
        self.mode = AppMode::WorkspaceSwitcher;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialog_state::DialogState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn test_storage() -> std::sync::Arc<piki_core::storage::AppStorage> {
        std::sync::Arc::new(piki_core::storage::AppStorage {
            workspaces: Box::new(piki_core::storage::json::JsonStorage),
            api_history: None,
            ui_prefs: None,
        })
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_initial_state() {
        let app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
        assert!(!app.interacting);
        assert!(!app.should_quit);
        assert!(app.workspaces.is_empty());
    }

    #[test]
    fn test_normal_to_help_and_back() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('?')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Help);

        // Esc returns to Normal
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_normal_to_about_and_back() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('a')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::About);

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_normal_to_confirm_quit() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('q')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::ConfirmQuit);
    }

    #[test]
    fn test_confirm_quit_cancel() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.mode = AppMode::ConfirmQuit;
        app.active_dialog = Some(DialogState::ConfirmQuit);
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('n')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_confirm_quit_accept() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.mode = AppMode::ConfirmQuit;
        app.active_dialog = Some(DialogState::ConfirmQuit);
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('y')));
        assert!(action.is_none());
        assert!(app.should_quit);
    }

    #[test]
    fn test_normal_to_new_workspace() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('n')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::NewWorkspace);
    }

    #[test]
    fn test_new_workspace_cancel() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Opening new workspace sets both mode and dialog
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, AppMode::NewWorkspace);

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.active_dialog.is_none());
    }

    #[test]
    fn test_normal_to_new_tab_requires_workspace() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // No workspaces → pressing 't' should NOT enter NewTab mode
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('t')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_new_workspace_tab_cycles_fields() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Use the normal entry point to set up dialog state
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, AppMode::NewWorkspace);

        let get_field = |app: &App| -> DialogField {
            match &app.active_dialog {
                Some(DialogState::NewWorkspace { active_field, .. }) => *active_field,
                _ => panic!("Expected NewWorkspace dialog"),
            }
        };

        // Dialog opens on Type field
        assert_eq!(get_field(&app), DialogField::Type);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Name);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Directory);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Description);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Prompt);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::KanbanPath);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Group);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Type);
    }

    #[test]
    fn test_new_workspace_char_appends_to_active_buffer() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Use normal entry point to create dialog
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, AppMode::NewWorkspace);

        // Tab from Type to Name field first
        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));

        crate::input::handle_key_event(&mut app, key(KeyCode::Char('a')));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('b')));

        match &app.active_dialog {
            Some(DialogState::NewWorkspace {
                name, name_cursor, ..
            }) => {
                assert_eq!(name, "ab");
                assert_eq!(*name_cursor, 2);
            }
            _ => panic!("Expected NewWorkspace dialog"),
        }
    }

    #[test]
    fn test_guard_no_edit_without_workspaces() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // 'e' (edit workspace) should do nothing without workspaces
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('e')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_guard_no_delete_without_workspaces() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('d')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_guard_no_info_without_workspaces() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('i')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_interacting_toggle() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        assert!(!app.interacting);

        // Enter → interact
        crate::input::handle_key_event(&mut app, key(KeyCode::Enter));
        assert!(app.interacting);

        // Ctrl-G → back to navigation
        crate::input::handle_key_event(&mut app, ctrl('g'));
        assert!(!app.interacting);
    }

    #[test]
    fn test_help_scroll() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Use the normal entry point to open help
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('?')));
        assert_eq!(app.mode, AppMode::Help);

        let get_scroll = |app: &App| -> u16 {
            match &app.active_dialog {
                Some(DialogState::Help { scroll }) => *scroll,
                _ => panic!("Expected Help dialog"),
            }
        };

        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(get_scroll(&app), 1);

        crate::input::handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(get_scroll(&app), 0);

        // Page down
        crate::input::handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        );
        assert_eq!(get_scroll(&app), 10);
    }

    #[test]
    fn test_pane_navigation() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);

        // j → down → GitStatus
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.active_pane, ActivePane::GitStatus);

        // l → right → MainPanel
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.active_pane, ActivePane::MainPanel);

        // h → left → WorkspaceList
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    #[test]
    fn test_toast_set_and_expire() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.set_toast("hello", ToastLevel::Info);
        assert!(app.toast.is_some());
        assert_eq!(app.toast.as_ref().unwrap().message, "hello");
        assert_eq!(app.toast.as_ref().unwrap().level, ToastLevel::Info);

        // Toast shouldn't be expired immediately
        assert!(!app.expire_toast());
    }

    #[test]
    fn test_toast_error_duration() {
        let app_toast = Toast::new("err".to_string(), ToastLevel::Error);
        assert_eq!(app_toast.duration, Duration::from_secs(5));

        let info_toast = Toast::new("info".to_string(), ToastLevel::Info);
        assert_eq!(info_toast.duration, Duration::from_secs(3));
    }

    #[test]
    fn test_workspace_number_keys_with_empty_list() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Pressing '1' with no workspaces should not panic
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('1')));
        assert_eq!(app.active_workspace, 0);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_commit_requires_workspace() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('c')));
        assert_eq!(app.mode, AppMode::Normal); // No workspace → no commit dialog
    }

    // ── Helper: add a minimal test workspace ──

    fn add_test_workspace(app: &mut App) -> usize {
        let info = piki_core::WorkspaceInfo {
            name: format!("test-ws-{}", app.workspaces.len()),
            path: std::path::PathBuf::from("/tmp/test"),
            branch: "main".to_string(),
            workspace_type: piki_core::WorkspaceType::Simple,
            description: String::new(),
            prompt: String::new(),
            kanban_path: None,
            group: None,
            order: app.workspaces.len() as u32,
            source_repo: std::path::PathBuf::from("/tmp/test"),
            source_repo_display: String::new(),
        };
        let ws = Workspace::from_info(info);
        app.workspaces.push(ws);
        app.workspaces.len() - 1
    }

    // ── Fuzzy overlay tests ──

    #[test]
    fn test_command_palette_open_and_dismiss() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        crate::input::handle_key_event(&mut app, ctrl('p'));
        assert_eq!(app.mode, AppMode::CommandPalette);
        assert!(app.command_palette.is_some());

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.command_palette.is_none());
    }

    #[test]
    fn test_workspace_switcher_open_and_dismiss() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app);

        crate::input::handle_key_event(&mut app, key(KeyCode::Char(' ')));
        assert_eq!(app.mode, AppMode::WorkspaceSwitcher);
        assert!(app.workspace_switcher.is_some());

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.workspace_switcher.is_none());
    }

    #[test]
    fn test_workspace_switcher_no_workspace() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // No workspaces → Space should not open switcher
        crate::input::handle_key_event(&mut app, key(KeyCode::Char(' ')));
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.workspace_switcher.is_none());
    }

    // ── Previous workspace toggle tests ──

    #[test]
    fn test_toggle_previous_workspace_round_trip() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app); // index 0
        add_test_workspace(&mut app); // index 1
        app.switch_workspace(1);
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.previous_workspace, Some(0));

        // Backtick toggles back to 0
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('`')));
        assert_eq!(app.active_workspace, 0);
        assert_eq!(app.previous_workspace, Some(1));

        // Backtick toggles back to 1
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('`')));
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.previous_workspace, Some(0));
    }

    #[test]
    fn test_toggle_previous_workspace_none() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app);
        assert!(app.previous_workspace.is_none());

        // Backtick with no previous → nothing changes
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('`')));
        assert_eq!(app.active_workspace, 0);
        assert!(app.previous_workspace.is_none());
    }

    // ── Esc exits non-terminal interaction tests ──

    #[test]
    fn test_esc_exits_workspace_list_interaction() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::WorkspaceList;
        app.interacting = true;

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(!app.interacting);
    }

    #[test]
    fn test_esc_exits_filelist_interaction() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::GitStatus;
        app.interacting = true;

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(!app.interacting);
    }

    #[test]
    fn test_ctrl_g_exits_interaction() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::WorkspaceList;
        app.interacting = true;

        crate::input::handle_key_event(&mut app, ctrl('g'));
        assert!(!app.interacting);
    }

    // ── Quick stage/unstage tests ──

    #[test]
    fn test_quick_stage_from_navigation() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let idx = add_test_workspace(&mut app);
        app.workspaces[idx].changed_files.push(ChangedFile {
            path: "foo.rs".to_string(),
            status: FileStatus::Modified,
        });
        app.active_pane = ActivePane::GitStatus;
        app.interacting = false;

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('s')));
        assert!(matches!(action, Some(crate::action::Action::GitStage(0))));
    }

    #[test]
    fn test_quick_unstage_from_navigation() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let idx = add_test_workspace(&mut app);
        app.workspaces[idx].changed_files.push(ChangedFile {
            path: "foo.rs".to_string(),
            status: FileStatus::Staged,
        });
        app.active_pane = ActivePane::GitStatus;
        app.interacting = false;

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('u')));
        assert!(matches!(action, Some(crate::action::Action::GitUnstage(0))));
    }

    #[test]
    fn test_quick_stage_wrong_pane_ignored() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let idx = add_test_workspace(&mut app);
        app.workspaces[idx].changed_files.push(ChangedFile {
            path: "foo.rs".to_string(),
            status: FileStatus::Modified,
        });
        app.active_pane = ActivePane::MainPanel;
        app.interacting = false;

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('s')));
        assert!(action.is_none());
    }

    // ── Symmetric pane navigation tests ──

    #[test]
    fn test_pane_nav_main_panel_h_to_workspace_list() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_pane = ActivePane::MainPanel;
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    #[test]
    fn test_pane_nav_main_panel_j_to_git_status() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_pane = ActivePane::MainPanel;
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.active_pane, ActivePane::GitStatus);
    }

    #[test]
    fn test_pane_nav_main_panel_k_to_workspace_list() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_pane = ActivePane::MainPanel;
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    // ── Workspace switch + focus tests ──

    #[test]
    fn test_workspace_enter_focuses_main_panel() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app); // index 0
        add_test_workspace(&mut app); // index 1
        app.active_pane = ActivePane::WorkspaceList;
        app.interacting = true;
        // Select the second workspace row in the sidebar
        app.selected_sidebar_row = 1;

        // Press Enter to select workspace 1
        crate::input::handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.active_pane, ActivePane::MainPanel);
        assert!(!app.interacting);
    }

    // ── Number key invalid workspace tests ──

    #[test]
    fn test_workspace_number_keys_toast_invalid() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app); // index 0
        add_test_workspace(&mut app); // index 1

        // Press '5' — no workspace at that position
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('5')));
        assert!(app.toast.is_some());
        assert!(app.toast.as_ref().unwrap().message.contains("No workspace"));
    }
}
