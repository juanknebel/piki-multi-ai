use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::text::Text;

// Re-export domain types from core for convenience
pub use piki_core::git::{get_ahead_behind, get_changed_files, get_current_branch};
pub use piki_core::pty::PtySession;
pub use piki_core::workspace::FileWatcher;
pub use piki_core::{AIProvider, ChangedFile, WorkspaceStatus, WorkspaceType};

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

/// Result of an async git refresh for a workspace
pub struct RefreshResult {
    pub workspace_idx: usize,
    pub changed_files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
    pub branch: Option<String>,
}

/// Result of backgrounded `FileWatcher::new` setup for a restored workspace.
/// Watch registration walks the whole worktree tree synchronously, so it's
/// run off the startup critical path (see `event_loop.rs`).
pub struct WatcherResult {
    pub workspace_idx: usize,
    pub watcher: anyhow::Result<piki_core::workspace::FileWatcher>,
}

/// Main application mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Viewing PTY output of the active workspace
    Normal,
    /// Input dialog for creating a new workspace
    NewWorkspace,
    /// Input dialog for editing a workspace
    EditWorkspace,
    /// Input dialog for creating a git worktree from a GitHub-origin parent
    CreateWorktree,
    /// Confirmation dialog for deleting a workspace
    ConfirmDelete,
    /// Help overlay
    Help,
    /// Fuzzy file search overlay
    FuzzySearch,
    /// Inline file editor
    InlineEdit,
    /// New tab provider selection dialog
    NewTab,
    /// About overlay
    About,
    /// Warning overlay: a bridged agent opened without its hook prerequisites
    MissingPrereqs,
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
    /// Dispatch agent dialog
    DispatchAgent,
    /// Manage agent profiles overlay
    ManageAgents,
    /// Edit/create agent profile dialog (step 1: name + provider)
    EditAgent,
    /// Edit agent role (step 2: large floating text editor)
    EditAgentRole,
    /// Import agents from repo files overlay
    ImportAgents,
    /// Choose kanban column for dispatched card on workspace deletion
    DispatchCardMove,
    /// Manage custom providers overlay
    ManageProviders,
    /// Edit/create a custom provider
    EditProvider,
    /// Global AI chat overlay (persists state when hidden)
    ChatPanel,
}

/// Which pane is currently selected / focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    WorkspaceList,
    Agents,
    MainPanel,
}

/// Which field is active in the New Workspace dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogField {
    /// Source toggle: Local folder vs GitHub URL.
    Source,
    /// Folder path (when source = Local) or URL (when source = GitHub).
    Directory,
    /// Parent directory the GitHub clone will land into. Only used when
    /// source = GitHub; cycling skips this field for Local.
    Destination,
    Description,
    Prompt,
    KanbanPath,
}

/// Source mode for the New Workspace dialog. Drives whether the dialog asks
/// for a folder path or a GitHub URL, and which workspace-creation action is
/// dispatched on Enter.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NewWorkspaceSource {
    #[default]
    Local,
    GitHub,
}

/// An item in the sidebar workspace list. Every row is a real workspace now —
/// manual grouping (a synthetic header row) is gone. Grouping is derived at
/// render/navigation time from git worktree structure: workspaces that share
/// a `source_repo` are a "family"; the one whose `workspace_type != Worktree`
/// (if loaded) is the family's parent row and can collapse its worktree
/// children.
#[derive(Debug, Clone)]
pub enum SidebarItem {
    Workspace {
        index: usize,
        /// `Some(collapsed)` when this row is a worktree-family parent with
        /// children — i.e. other loaded workspaces share its `source_repo`
        /// and this one isn't itself a `Worktree`. `collapsed` says whether
        /// those children are currently hidden. `None` for every other row
        /// (standalone workspaces, family children, and orphaned worktree
        /// families with no parent loaded).
        collapsed: Option<bool>,
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
    /// Idle watcher for provider tabs. `Some` only for `AIProvider::Custom(_)`
    /// tabs (built-in tabs are interactive and never "idle"). The TUI ticks
    /// this every 50 ms and surfaces a notification + sidebar badge when it
    /// fires.
    pub idle_watcher: Option<piki_core::idle_watcher::IdleWatcher>,
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

impl Tab {
    /// Snapshot of the structured Claude agent state for this tab, if the
    /// cli-agent OSC 777 channel has produced at least one event. `None`
    /// for non-Claude tabs (or before the first event). Locks the shell
    /// mutex briefly — safe to call from pure render functions.
    /// (status, attention pending, last summary) for this tab's cli agent.
    /// `attention` is true while the agent has news the user hasn't looked
    /// at yet (cleared when the tab is viewed).
    pub fn cli_agent_snapshot(
        &self,
    ) -> Option<(piki_core::cli_agent::CliAgentStatus, bool, Option<String>)> {
        let shell = self.pty_session.as_ref()?.shell()?;
        let guard = shell.lock();
        let agent = guard.state.cli_agent.as_ref()?;
        Some((
            agent.status,
            agent.last_attention_at.is_some(),
            agent.last_summary.clone(),
        ))
    }
}

/// A single workspace backed by a git worktree
pub struct Workspace {
    /// Core workspace metadata (shared with other frontends)
    pub info: piki_core::WorkspaceInfo,
    pub status: WorkspaceStatus,
    pub changed_files: Vec<ChangedFile>,
    /// Sub-directories for Project workspaces
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
    /// Current git branch, refreshed in the background alongside `ahead_behind`.
    /// `None` until the first refresh completes, or if the workspace isn't a
    /// git repo / is in detached HEAD.
    pub branch: Option<String>,
    /// Kanban app state
    pub kanban_app: Option<flow_tui::App>,
    /// Kanban provider
    pub kanban_provider: Option<Box<dyn flow_core::provider::Provider>>,
    /// Code review state
    pub code_review: Option<crate::code_review::CodeReviewState>,
    /// True when at least one tab in this workspace has emitted an idle
    /// notification that the user has not yet acknowledged. Cleared when the
    /// user switches to this workspace. Drives the sidebar idle badge.
    pub has_idle_notification: bool,
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

/// Precedence for an agent (status, attention) pair, worst first: needs-
/// permission > unseen news > running > everything else. Shared by
/// `Workspace::agent_status_rollup()` (per-workspace) and the sidebar's
/// worktree-family aggregation (across a collapsed family's members).
pub(crate) fn agent_status_severity(status: piki_core::cli_agent::CliAgentStatus, attention: bool) -> u8 {
    use piki_core::cli_agent::CliAgentStatus as S;
    match (status, attention) {
        (S::WaitingPermission, _) => 4,
        (S::Idle | S::Done, true) => 3,
        (S::Running, _) => 2,
        _ => 0,
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
            branch: None,
            kanban_app: None,
            kanban_provider: None,
            code_review: None,
            has_idle_notification: false,
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

    /// Add a new tab and return its index.
    ///
    /// `provider_cfg` is the matching `providers.toml` entry for `Custom`
    /// providers (resolved by the caller); its per-provider idle knobs
    /// (`idle_threshold_secs` / `idle_notify`) drive the tab's `IdleWatcher`.
    /// Pass `None` for built-in providers (Shell/Kanban/Api/…).
    pub fn add_tab(
        &mut self,
        provider: AIProvider,
        closable: bool,
        provider_cfg: Option<&piki_core::providers::ProviderConfig>,
    ) -> usize {
        let idle_watcher = matches!(provider, AIProvider::Custom(_))
            .then(|| piki_core::idle_watcher::IdleWatcher::from_provider_config(provider_cfg));
        let tab = Tab {
            id: self.next_tab_id,
            provider,
            pty_session: None,
            pty_parser: None,
            closable,
            term_scroll: 0,
            last_bytes_processed: 0,
            idle_watcher,
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
            idle_watcher: None,
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
        self.changed_files.len()
    }

    /// Worst (status, attention) across this workspace's agent tabs, for the
    /// sidebar rollup. Priority: needs-permission > unseen news > running.
    pub fn agent_status_rollup(&self) -> Option<(piki_core::cli_agent::CliAgentStatus, bool)> {
        self.tabs
            .iter()
            .filter_map(|t| t.cli_agent_snapshot().map(|(status, att, _)| (status, att)))
            .max_by_key(|&(s, a)| agent_status_severity(s, a))
    }

    pub fn status_label(&self) -> &str {
        match &self.status {
            WorkspaceStatus::Idle => "idle",
            WorkspaceStatus::Busy => "busy",
            WorkspaceStatus::Done => "done",
            WorkspaceStatus::Error(_) => "error",
        }
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

    /// Insert a multi-line string at the cursor position (for paste).
    pub fn insert_text(&mut self, text: &str) {
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte_idx(line, self.cursor_col);
        let after = line[byte_idx..].to_string();
        line.truncate(byte_idx);

        let mut paste_lines: Vec<&str> = text.split('\n').collect();
        // Remove trailing empty element from a trailing newline
        if paste_lines.last() == Some(&"") {
            paste_lines.pop();
        }

        if paste_lines.is_empty() {
            line.push_str(&after);
            return;
        }

        // First paste line appends to current line
        self.lines[self.cursor_row].push_str(paste_lines[0]);

        // Middle lines are inserted as new lines
        for &pl in &paste_lines[1..] {
            self.cursor_row += 1;
            self.lines.insert(self.cursor_row, pl.to_string());
        }

        // Append the remainder after the cursor to the last inserted line
        self.cursor_col = self.lines[self.cursor_row].chars().count();
        self.lines[self.cursor_row].push_str(&after);
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

/// Stable identity for a worktree family, used as the key into
/// `App::collapsed_groups`. The family's `source_repo` path is unique per
/// git root and stable across reorders/reloads, unlike a workspace index.
fn family_key(info: &piki_core::WorkspaceInfo) -> String {
    info.source_repo.to_string_lossy().to_string()
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
    /// Vertical border between file list and diff in code review
    CodeReviewSplit,
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

/// Keyboard input state for the tmux-style prefix model. Keys always go to
/// the focused pane except while a prefix chord or the terminal scroll mode
/// is active. Only consulted in `AppMode::Normal` / `AppMode::Diff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputState {
    /// Passthrough: keys go to the focused pane (PTY, kanban, lists, ...).
    #[default]
    Normal,
    /// The prefix key was pressed; the next key is looked up in the app table.
    PrefixPending,
    /// Terminal scroll mode (`prefix [`): keys scroll the focused terminal.
    TermScroll,
    /// Resize repeat mode: entered by a sidebar/split resize action so the bare
    /// resize keys repeat without re-pressing the prefix each time (tmux
    /// `bind -r`). Any non-resize key or Esc exits.
    Resize,
}

/// Cached footer keys: (mode, input_state, active_pane, has_markdown, has_kanban, api_footer_state, new_tab_menu, keys)
/// api_footer_state: 0 = no API tab, 1 = API tab, 2 = API tab with search open
/// new_tab_menu: 0 = N/A, 1 = Main, 2 = Agents, 3 = Tools
pub type FooterCache = (
    AppMode,
    InputState,
    ActivePane,
    bool,
    bool,
    u8,
    u8,
    Vec<(String, &'static str)>,
);

pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub active_pane: ActivePane,
    /// Prefix/scroll input state (tmux-style; see [`InputState`])
    pub input_state: InputState,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub selected_workspace: usize,
    /// Selected row in the Agents pane (index into `agent_rows()`)
    pub selected_agent_row: usize,
    /// (workspace, tab id) the Agents highlight was last synced to, so the
    /// sync only fires when the user actually moves to another tab.
    agent_focus_key: Option<(usize, usize)>,
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
    /// Code review file-list width as percentage (10..=90)
    pub code_review_split_pct: u16,
    /// Mouse drag-resize state
    pub resize_drag: Option<ResizeDrag>,
    /// X coordinate of the vertical border between sidebar and main panel
    pub sidebar_x: u16,
    /// Y coordinate of the horizontal border between workspace list and file list
    pub left_split_y: u16,
    /// Rect of the left sidebar area (for resize calculations)
    pub left_area_rect: Rect,
    /// X coordinate of the vertical border in code review (file list | diff)
    pub code_review_divider_x: u16,
    /// Body area of the code review layout (for relative drag calculation)
    pub code_review_body_rect: Rect,
    /// Whether the UI needs to be redrawn
    pub needs_redraw: bool,
    /// Tick counter driving the running-agent spinner in the Agents pane
    /// (advanced by the event loop only while some agent is running)
    pub spinner_frame: usize,
    /// Last time the spinner advanced (and forced a redraw) — throttles the
    /// spinner to `SPINNER_INTERVAL` instead of the raw tick rate
    pub last_spinner_at: Instant,
    pub config: crate::config::Config,
    /// Channel for receiving async git refresh results
    pub refresh_tx: tokio::sync::mpsc::UnboundedSender<RefreshResult>,
    pub refresh_rx: tokio::sync::mpsc::UnboundedReceiver<RefreshResult>,
    /// Channel for receiving backgrounded FileWatcher setup results
    pub watcher_tx: tokio::sync::mpsc::UnboundedSender<WatcherResult>,
    pub watcher_rx: tokio::sync::mpsc::UnboundedReceiver<WatcherResult>,
    /// Channel for receiving status messages from background tasks
    pub status_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub status_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    /// Whether a background git refresh is in-flight
    pub refresh_pending: bool,
    /// Terminal search overlay (None = closed)
    pub term_search: Option<TermSearchState>,
    /// Layout areas for mouse hit-testing
    pub ws_list_area: Rect,
    pub agents_area: Rect,
    pub tabs_area: Rect,
    pub subtabs_area: Rect,
    pub main_content_area: Rect,
    /// Cached footer keys: (mode, input_state, active_pane, has_markdown) → keys
    pub footer_cache: Option<FooterCache>,
    /// Last time inactive workspace PTYs were checked for exit
    pub last_inactive_pty_check: Instant,
    /// Cached result of `gh` CLI availability check (None = not yet checked)
    pub gh_available: Option<bool>,
    /// Storage backend (SQLite)
    pub storage: std::sync::Arc<piki_core::storage::AppStorage>,
    /// Cached agent profiles for the current project
    pub agent_profiles: Vec<piki_core::storage::AgentProfile>,
    /// User-configurable providers loaded from providers.toml
    pub provider_manager: piki_core::providers::ProviderManager,
    /// Data paths for saving config files
    pub paths: piki_core::paths::DataPaths,
    /// Global AI chat panel state (persists when overlay is hidden)
    pub chat_panel: ChatPanelState,
    /// Channel for receiving streaming chat tokens from Ollama
    pub chat_token_tx: tokio::sync::mpsc::UnboundedSender<piki_api_client::ChatStreamEvent>,
    pub chat_token_rx: tokio::sync::mpsc::UnboundedReceiver<piki_api_client::ChatStreamEvent>,
    /// Channel for receiving agent loop events
    pub agent_event_tx: tokio::sync::mpsc::UnboundedSender<piki_agent::AgentEvent>,
    pub agent_event_rx: tokio::sync::mpsc::UnboundedReceiver<piki_agent::AgentEvent>,
}

/// Persistent state for the global AI chat overlay.
/// Lives as a top-level `App` field (not in `DialogState`) so state survives toggling.
/// Which sub-view the chat overlay is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSubMode {
    /// Normal chat (message list + input)
    #[default]
    Chat,
    /// Model selector list
    ModelSelect,
    /// Settings editor (base URL + system prompt)
    Settings,
}

/// Which field is active in the settings editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSettingsField {
    #[default]
    ServerType,
    BaseUrl,
    SystemPrompt,
}

#[derive(Default)]
pub struct ChatPanelState {
    pub messages: Vec<piki_core::chat::ChatMessage>,
    pub config: piki_core::chat::ChatConfig,
    pub input: String,
    pub input_cursor: usize,
    pub scroll: usize,
    pub streaming: bool,
    /// Accumulates tokens during a streaming response
    pub current_response: String,
    /// Cached model names from Ollama
    pub models: Vec<String>,
    pub model_selected: usize,
    /// Current sub-mode within the chat overlay
    pub sub_mode: ChatSubMode,
    /// Settings editor: editable base URL
    pub settings_url: String,
    /// Settings editor: editable system prompt
    pub settings_prompt: String,
    /// Settings editor: which field is focused
    pub settings_field: ChatSettingsField,
    /// Settings editor: cursor position in the active field
    pub settings_cursor: usize,
    /// Settings editor: editable server type
    pub settings_server_type: piki_core::chat::ChatServerType,
    /// Whether to use the agentic tool-use loop instead of plain chat
    pub agent_mode: bool,
    /// Currently executing tool name (shown during agent loop)
    pub agent_tool_status: Option<String>,
    /// Pending write-tool approval request from the agent loop
    pub pending_approval: Option<piki_agent::ApprovalRequest>,
}

impl App {
    pub fn new(
        storage: std::sync::Arc<piki_core::storage::AppStorage>,
        paths: &piki_core::paths::DataPaths,
    ) -> Self {
        let (refresh_tx, refresh_rx) = tokio::sync::mpsc::unbounded_channel::<RefreshResult>();
        let (watcher_tx, watcher_rx) = tokio::sync::mpsc::unbounded_channel::<WatcherResult>();
        let (status_tx, status_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (chat_token_tx, chat_token_rx) =
            tokio::sync::mpsc::unbounded_channel::<piki_api_client::ChatStreamEvent>();
        let (agent_event_tx, agent_event_rx) =
            tokio::sync::mpsc::unbounded_channel::<piki_agent::AgentEvent>();
        let config = crate::config::Config::load_from(paths);
        // Propagate notification prefs to the shared core layer (process
        // globals — the notify_* helpers read them on every event).
        piki_core::notifications::set_delivery(config.notifications.parsed_delivery());
        piki_core::sound::set_settings(config.notifications.sound_settings());
        let syntax = crate::syntax::SyntaxHighlighter::new(&config.syntax_theme);
        Self {
            should_quit: false,
            mode: AppMode::Normal,
            active_pane: ActivePane::WorkspaceList,
            input_state: InputState::default(),
            log_buffer: crate::log_buffer::new_buffer(),
            workspaces: Vec::new(),
            active_workspace: 0,
            selected_workspace: 0,
            selected_agent_row: 0,
            agent_focus_key: None,
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
            code_review_split_pct: 25,
            resize_drag: None,
            sidebar_x: 0,
            left_split_y: 0,
            left_area_rect: Rect::default(),
            code_review_divider_x: 0,
            code_review_body_rect: Rect::default(),
            needs_redraw: true,
            spinner_frame: 0,
            last_spinner_at: Instant::now(),
            config,
            refresh_tx,
            refresh_rx,
            watcher_tx,
            watcher_rx,
            status_tx,
            status_rx,
            refresh_pending: false,
            term_search: None,
            ws_list_area: Rect::default(),
            agents_area: Rect::default(),
            tabs_area: Rect::default(),
            subtabs_area: Rect::default(),
            main_content_area: Rect::default(),
            footer_cache: None,
            last_inactive_pty_check: Instant::now(),
            gh_available: None,
            storage,
            agent_profiles: Vec::new(),
            provider_manager: piki_core::providers::ProviderManager::load_or_init(
                &paths.providers_path(),
            ),
            paths: paths.clone(),
            chat_panel: ChatPanelState::default(),
            chat_token_tx,
            chat_token_rx,
            agent_event_tx,
            agent_event_rx,
        }
    }

    /// Persist layout preferences (sidebar_pct, left_split_pct) to storage if available.
    pub fn save_layout_prefs(&self) {
        if let Some(ref ui_prefs) = self.storage.ui_prefs {
            let _ = ui_prefs.set_preference("sidebar_pct", &self.sidebar_pct.to_string());
            let _ = ui_prefs.set_preference("left_split_pct", &self.left_split_pct.to_string());
            let _ = ui_prefs.set_preference(
                "code_review_split_pct",
                &self.code_review_split_pct.to_string(),
            );
        }
    }

    /// Build the list of AI providers from providers.toml.
    pub fn new_tab_agent_list(&self) -> Vec<AIProvider> {
        self.provider_manager
            .all()
            .iter()
            .map(|config| AIProvider::Custom(config.name.clone()))
            .collect()
    }

    /// Providers from providers.toml that are marked `dispatchable`.
    pub fn dispatchable_provider_list(&self) -> Vec<AIProvider> {
        self.provider_manager
            .dispatchable()
            .iter()
            .map(|config| AIProvider::Custom(config.name.clone()))
            .collect()
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
            .map(|item| match item {
                SidebarItem::Workspace { index, .. } => *index,
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
        self.switch_workspace_and_focus(next);
    }

    pub fn prev_workspace(&mut self) {
        let visible: Vec<usize> = self
            .sidebar_items()
            .iter()
            .map(|item| match item {
                SidebarItem::Workspace { index, .. } => *index,
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
        self.switch_workspace_and_focus(prev);
    }

    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.workspaces.len() {
            if index != self.active_workspace {
                self.previous_workspace = Some(self.active_workspace);
            }
            self.active_workspace = index;
            self.selected_workspace = index;
            self.sync_sidebar_row(index);
            self.mode = AppMode::Normal;
            self.selection = None;
            // Trigger immediate background refresh for the new workspace
            self.workspaces[index].dirty = true;
            self.workspaces[index].last_refresh = None;
            // User acknowledged any pending idle notification badge by
            // visiting; re-fires happen naturally on the next real burst of
            // agent output (gated by `IdleWatcher::rearm_bytes`).
            self.workspaces[index].has_idle_notification = false;
            if let Some(tab) = self.workspaces[index].current_tab_mut() {
                tab.term_scroll = 0;
            }
            // Persist the active workspace so the next startup focuses it.
            // Path is the canonical key (unique + stable across restarts).
            if let Some(prefs) = self.storage.ui_prefs.as_ref() {
                let path_str = self.workspaces[index]
                    .info
                    .path
                    .to_string_lossy()
                    .to_string();
                let _ = prefs.set_preference("last_focused_workspace", &path_str);
            }
        }
    }

    /// Same as `switch_workspace`, but also focuses the main panel — use this
    /// from any keyboard-driven action that should land the user on the
    /// workspace's content (as opposed to the startup restore path, which
    /// intentionally leaves `active_pane` untouched).
    pub fn switch_workspace_and_focus(&mut self, index: usize) {
        self.switch_workspace(index);
        self.active_pane = ActivePane::MainPanel;
    }

    /// Build the visual sidebar item list, grouping workspaces by their group field.
    /// Map an absolute screen `row` inside the Agents pane to an index into
    /// [`agent_rows`], accounting for the pane border and the derived scroll
    /// offset (mirrors `render_agents_pane`). Returns `None` when the row is
    /// on the border or past the last agent. Shared by mouse click + scroll
    /// hit-testing so the two can't drift.
    pub fn agent_row_at(&self, row: u16) -> Option<usize> {
        let rows = self.agent_rows();
        if rows.is_empty() {
            return None;
        }
        let inner_y = self.agents_area.y + 1;
        if row < inner_y {
            return None;
        }
        let visible = self.agents_area.height.saturating_sub(2) as usize;
        let selected = self.selected_agent_row.min(rows.len() - 1);
        let scroll_offset = if visible > 0 && selected >= visible {
            selected + 1 - visible
        } else {
            0
        };
        let idx = (row - inner_y) as usize + scroll_offset;
        (idx < rows.len()).then_some(idx)
    }

    /// Rows of the Agents pane: every (workspace, tab) pair running an AI
    /// agent, across ALL workspaces, in sidebar order. That's agent tabs
    /// (Custom provider) plus any other tab whose cli-agent channel has
    /// reported — e.g. a `claude` typed manually inside a shell tab.
    ///
    /// A shell entry disappears when its `claude` exits: the shell's OSC 133
    /// `CommandEnd` marker clears the tab's cli-agent state (see
    /// `ShellTabState::apply`), so its snapshot goes away and it drops off
    /// here. A dedicated Custom-provider tab always lists — that tab *is* the
    /// agent. Labels and status are derived live at render time.
    pub fn agent_rows(&self) -> Vec<(usize, usize)> {
        self.workspaces
            .iter()
            .enumerate()
            .flat_map(|(wi, ws)| {
                ws.tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        matches!(t.provider, AIProvider::Custom(_))
                            || t.cli_agent_snapshot().is_some()
                    })
                    .map(move |(ti, _)| (wi, ti))
            })
            .collect()
    }

    /// Move the Agents highlight onto the agent tab the user is standing on.
    ///
    /// The pane lists agents from every workspace, so a highlight left behind
    /// on another workspace's agent reads as "I'm on that one". Called once per
    /// event-loop iteration rather than at each tab/workspace switch site, and
    /// only re-selects when the active tab changed — so browsing the pane with
    /// j/k isn't yanked back. A non-agent tab (shell, lazygit) leaves the
    /// highlight where it was: there is no row to move it to.
    pub fn sync_agent_selection(&mut self) {
        let key = self
            .current_workspace()
            .and_then(|ws| ws.tabs.get(ws.active_tab).map(|tab| tab.id))
            .map(|tab_id| (self.active_workspace, tab_id));
        if key == self.agent_focus_key {
            return;
        }
        self.agent_focus_key = key;

        let Some(target) = self
            .current_workspace()
            .map(|ws| (self.active_workspace, ws.active_tab))
        else {
            return;
        };
        if let Some(idx) = self.agent_rows().iter().position(|&row| row == target) {
            self.selected_agent_row = idx;
        }
    }

    /// Build the visual sidebar item list. Workspaces are grouped by
    /// `source_repo`: a "family" is any set of 2+ workspaces sharing one.
    /// Within a family, the (at most one) workspace whose `workspace_type !=
    /// Worktree` is the parent and is emitted first with a collapse chevron;
    /// its `Worktree` siblings follow (in their existing relative order)
    /// unless collapsed, in which case they're omitted entirely. A family
    /// with no parent loaded (every member is a `Worktree`) has nothing to
    /// attach a chevron to, so its members are emitted flat and uncollapsible.
    /// Standalone workspaces (no siblings sharing `source_repo`) are emitted
    /// exactly as before.
    ///
    /// The family block is emitted at the position of its first member
    /// encountered in `self.workspaces` order.
    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let mut items = Vec::new();
        let mut consumed = vec![false; self.workspaces.len()];

        for i in 0..self.workspaces.len() {
            if consumed[i] {
                continue;
            }
            let source_repo = &self.workspaces[i].info.source_repo;
            let siblings: Vec<usize> = self
                .workspaces
                .iter()
                .enumerate()
                .filter(|(j, w)| !consumed[*j] && &w.info.source_repo == source_repo)
                .map(|(j, _)| j)
                .collect();

            if siblings.len() <= 1 {
                items.push(SidebarItem::Workspace {
                    index: i,
                    collapsed: None,
                });
                consumed[i] = true;
                continue;
            }

            let parent_pos = siblings
                .iter()
                .position(|&idx| self.workspaces[idx].info.workspace_type != WorkspaceType::Worktree);

            match parent_pos {
                Some(pp) => {
                    let parent_idx = siblings[pp];
                    let key = family_key(&self.workspaces[parent_idx].info);
                    let collapsed = self.collapsed_groups.contains(&key);
                    items.push(SidebarItem::Workspace {
                        index: parent_idx,
                        collapsed: Some(collapsed),
                    });
                    for &idx in &siblings {
                        consumed[idx] = true;
                        if idx != parent_idx && !collapsed {
                            items.push(SidebarItem::Workspace {
                                index: idx,
                                collapsed: None,
                            });
                        }
                    }
                }
                None => {
                    for &idx in &siblings {
                        consumed[idx] = true;
                        items.push(SidebarItem::Workspace {
                            index: idx,
                            collapsed: None,
                        });
                    }
                }
            }
        }

        items
    }

    /// Visual rows for the workspace sidebar, in render order: `Some(row)`
    /// indexes into `sidebar_items()`, `None` is a blank separator line
    /// inserted on BOTH sides of a worktree family block (before its parent
    /// row, after its last child) so the block reads as bounded against a
    /// flat neighbor — not just closed off on one side. Two adjacent
    /// families share a single separator between them, not one from each
    /// side. Rendering and mouse hit-testing must both walk this list (not
    /// `sidebar_items()` line-for-line) once separators exist, or their row
    /// math drifts apart.
    pub fn sidebar_visual_rows(&self) -> Vec<Option<usize>> {
        let items = self.sidebar_items();
        // A row's "block key" is its source_repo when it's part of a
        // worktree family (Some), or None for a standalone/flat row — two
        // flat rows never get a separator between them, only a transition
        // into or out of a family block does.
        let block_key = |i: usize| -> Option<&std::path::PathBuf> {
            let SidebarItem::Workspace { index, .. } = &items[i];
            let repo = &self.workspaces[*index].info.source_repo;
            let family_count = self
                .workspaces
                .iter()
                .filter(|w| &w.info.source_repo == repo)
                .count();
            (family_count > 1).then_some(repo)
        };

        let mut rows = Vec::with_capacity(items.len());
        for i in 0..items.len() {
            if i > 0 {
                let prev = block_key(i - 1);
                let cur = block_key(i);
                if prev != cur && (prev.is_some() || cur.is_some()) {
                    rows.push(None);
                }
            }
            rows.push(Some(i));
        }
        rows
    }

    /// Map a sidebar visual row to a workspace index. Every row is a real
    /// workspace, so this only returns `None` when `row` is out of range.
    pub fn sidebar_row_to_workspace(&self, row: usize) -> Option<usize> {
        self.sidebar_items()
            .get(row)
            .map(|SidebarItem::Workspace { index, .. }| *index)
    }

    pub fn select_next_sidebar_row(&mut self) {
        let count = self.sidebar_items().len();
        if count > 0 {
            self.follow_sidebar_row((self.selected_sidebar_row + 1) % count);
        }
    }

    pub fn select_prev_sidebar_row(&mut self) {
        let count = self.sidebar_items().len();
        if count > 0 {
            self.follow_sidebar_row((self.selected_sidebar_row + count - 1) % count);
        }
    }

    /// Land the sidebar cursor on `row` and switch to that row's workspace.
    ///
    /// Follow-focus: the cursor and the workspace every action targets are the
    /// same thing. Without it the two drift apart — the cursor sits on one
    /// workspace while `prefix c` opens its tab in whichever workspace the main
    /// panel still shows — and every workspace-scoped action becomes a coin
    /// flip. Every sidebar row is a workspace now, so this always switches.
    fn follow_sidebar_row(&mut self, row: usize) {
        self.selected_sidebar_row = row;
        if let Some(idx) = self.sidebar_row_to_workspace(row) {
            self.switch_workspace(idx);
        }
    }

    /// If the currently selected sidebar row is a worktree-family parent
    /// (has a collapse chevron), its family key and current collapsed state.
    fn selected_family_state(&self) -> Option<(String, bool)> {
        match self.sidebar_items().get(self.selected_sidebar_row)? {
            SidebarItem::Workspace {
                index,
                collapsed: Some(collapsed),
            } => {
                let ws = self.workspaces.get(*index)?;
                Some((family_key(&ws.info), *collapsed))
            }
            _ => None,
        }
    }

    /// Toggle collapse on the selected row if it's a worktree-family parent.
    /// No-op otherwise.
    pub fn toggle_selected_group(&mut self) {
        let Some((key, collapsed)) = self.selected_family_state() else {
            return;
        };
        if collapsed {
            self.collapsed_groups.remove(&key);
        } else {
            self.collapsed_groups.insert(key);
        }
        self.persist_collapsed_groups();
    }

    fn persist_collapsed_groups(&self) {
        if let Some(ref ui_prefs) = self.storage.ui_prefs {
            let _ = ui_prefs.set_collapsed_groups(&self.collapsed_groups);
        }
    }

    /// Collapse the selected row's worktree family. Tree-style `←` behaviour;
    /// a no-op unless the selection is on an (expanded) family parent row.
    pub fn collapse_selected_group(&mut self) {
        let Some((key, collapsed)) = self.selected_family_state() else {
            return;
        };
        if !collapsed {
            self.collapsed_groups.insert(key);
            self.persist_collapsed_groups();
        }
    }

    /// Expand the selected row's worktree family. Tree-style `→` behaviour;
    /// a no-op unless the selection is on a collapsed family parent row.
    pub fn expand_selected_group(&mut self) {
        let Some((key, collapsed)) = self.selected_family_state() else {
            return;
        };
        if collapsed {
            self.collapsed_groups.remove(&key);
            self.persist_collapsed_groups();
        }
    }

    /// Update selected_sidebar_row to point to the given workspace index.
    pub fn sync_sidebar_row(&mut self, ws_idx: usize) {
        let items = self.sidebar_items();
        for (i, SidebarItem::Workspace { index, .. }) in items.iter().enumerate() {
            if *index == ws_idx {
                self.selected_sidebar_row = i;
                return;
            }
        }
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
            self.switch_workspace_and_focus(prev);
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
            agent_profiles: None,
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
        let app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
        assert_eq!(app.input_state, InputState::Normal);
        assert!(!app.should_quit);
        assert!(app.workspaces.is_empty());
    }

    #[test]
    fn test_normal_to_help_and_back() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('a')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::About);

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_normal_to_confirm_quit() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('q')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::ConfirmQuit);
    }

    #[test]
    fn test_confirm_quit_cancel() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        app.mode = AppMode::ConfirmQuit;
        app.active_dialog = Some(DialogState::ConfirmQuit);
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('n')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_confirm_quit_accept() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        app.mode = AppMode::ConfirmQuit;
        app.active_dialog = Some(DialogState::ConfirmQuit);
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('y')));
        assert!(action.is_none());
        assert!(app.should_quit);
    }

    #[test]
    fn test_normal_to_new_workspace() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('s')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::NewWorkspace);
    }

    #[test]
    fn test_new_workspace_cancel() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // Opening new workspace sets both mode and dialog
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('s')));
        assert_eq!(app.mode, AppMode::NewWorkspace);

        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.active_dialog.is_none());
    }

    #[test]
    fn test_normal_to_new_tab_requires_workspace() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // No workspaces → prefix c should NOT enter NewTab mode
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('c')));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_new_workspace_tab_cycles_fields() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // Use the normal entry point to set up dialog state
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('s')));
        assert_eq!(app.mode, AppMode::NewWorkspace);

        let get_field = |app: &App| -> DialogField {
            match &app.active_dialog {
                Some(DialogState::NewWorkspace { active_field, .. }) => *active_field,
                _ => panic!("Expected NewWorkspace dialog"),
            }
        };

        // Dialog opens on Source field
        assert_eq!(get_field(&app), DialogField::Source);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Directory);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Description);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Prompt);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::KanbanPath);

        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(get_field(&app), DialogField::Source);
    }

    #[test]
    fn test_new_workspace_char_appends_to_active_buffer() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // Use normal entry point to create dialog
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('s')));
        assert_eq!(app.mode, AppMode::NewWorkspace);

        // Tab from Source → Directory → Description (Destination is skipped
        // for Local source) to reach the Description field
        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));
        crate::input::handle_key_event(&mut app, key(KeyCode::Tab));

        crate::input::handle_key_event(&mut app, key(KeyCode::Char('a')));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('b')));

        match &app.active_dialog {
            Some(DialogState::NewWorkspace {
                desc, desc_cursor, ..
            }) => {
                assert_eq!(desc, "ab");
                assert_eq!(*desc_cursor, 2);
            }
            _ => panic!("Expected NewWorkspace dialog"),
        }
    }

    #[test]
    fn test_guard_no_edit_without_workspaces() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // 'e' (edit workspace) should do nothing without workspaces
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('e')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_guard_no_delete_without_workspaces() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('d')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_guard_no_info_without_workspaces() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('i')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_prefix_state_machine() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        assert_eq!(app.input_state, InputState::Normal);

        // Prefix key arms the chord
        crate::input::handle_key_event(&mut app, ctrl('g'));
        assert_eq!(app.input_state, InputState::PrefixPending);

        // Esc cancels without any action
        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.input_state, InputState::Normal);
        assert_eq!(app.mode, AppMode::Normal);

        // Prefix + unknown key → back to Normal, toast, no action
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::F(5)));
        assert!(action.is_none());
        assert_eq!(app.input_state, InputState::Normal);
        assert!(app.toast.is_some());
        assert_eq!(app.mode, AppMode::Normal);

        // Prefix-prefix returns to Normal (literal send is a no-op without a PTY)
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, ctrl('g'));
        assert_eq!(app.input_state, InputState::Normal);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_prefix_is_one_shot() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // prefix ? opens Help and resets the state
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('?')));
        assert_eq!(app.mode, AppMode::Help);
        assert_eq!(app.input_state, InputState::Normal);
        // Close help; a bare 'q' must NOT open the quit dialog (no more nav mode)
        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('q')));
        assert_eq!(app.mode, AppMode::Normal);
        // prefix q opens the quit dialog
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('q')));
        assert_eq!(app.mode, AppMode::ConfirmQuit);
    }

    #[test]
    fn test_help_scroll() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // Open help through the prefix chord
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('?')));
        assert_eq!(app.mode, AppMode::Help);

        let get_scroll = |app: &App| -> u16 {
            match &app.active_dialog {
                Some(DialogState::Help { scroll, .. }) => *scroll,
                _ => panic!("Expected Help dialog"),
            }
        };

        // The help browser is a search box now: arrows scroll, letters filter.
        crate::input::handle_key_event(&mut app, key(KeyCode::Down));
        assert_eq!(get_scroll(&app), 1);

        crate::input::handle_key_event(&mut app, key(KeyCode::Up));
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);

        // prefix j → down → Agents pane
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.active_pane, ActivePane::Agents);

        // prefix l → right → MainPanel
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.active_pane, ActivePane::MainPanel);

        // prefix h → left → WorkspaceList
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);

        // prefix Left (arrow alternative) is a no-op from the sidebar
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Left));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    #[test]
    fn test_toast_set_and_expire() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
    fn test_commit_requires_workspace() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('C')));
        assert_eq!(app.mode, AppMode::Normal); // No workspace → no commit dialog
    }

    // ── Helper: add a minimal test workspace ──

    fn add_test_workspace(app: &mut App) -> usize {
        let idx = app.workspaces.len();
        // Each test workspace gets its own source_repo by default, so it's
        // standalone (no worktree family) unless a test deliberately shares
        // one across workspaces to exercise the family/collapse behavior.
        let info = piki_core::WorkspaceInfo {
            name: format!("test-ws-{}", idx),
            path: std::path::PathBuf::from("/tmp/test"),
            workspace_type: piki_core::WorkspaceType::Simple,
            description: String::new(),
            prompt: String::new(),
            kanban_path: None,
            order: idx as u32,
            source_repo: std::path::PathBuf::from(format!("/tmp/test-{idx}")),
            source_repo_display: String::new(),
            dispatch_card_id: None,
            dispatch_source_kanban: None,
            dispatch_agent_name: None,
            origin: piki_core::WorkspaceOrigin::default(),
            is_git_repo: true,
        };
        let ws = Workspace::from_info(info);
        app.workspaces.push(ws);
        app.workspaces.len() - 1
    }

    /// Give the active tab of `ws_idx` a live in-memory terminal (a bare vt100
    /// parser, no real PTY) so terminal-search gating sees a searchable pane.
    fn add_terminal_tab(app: &mut App, ws_idx: usize) {
        let ws = &mut app.workspaces[ws_idx];
        let idx = ws.add_tab(AIProvider::Shell, true, None);
        ws.tabs[idx].pty_parser = Some(Arc::new(Mutex::new(vt100::Parser::new(24, 80, 0))));
        ws.active_tab = idx;
    }

    // ── Fuzzy overlay tests ──

    #[test]
    fn test_command_palette_open_and_dismiss() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char(':')));
        assert_eq!(app.mode, AppMode::CommandPalette);
        assert!(app.command_palette.is_some());

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.command_palette.is_none());
    }

    #[test]
    fn test_workspace_switcher_open_and_dismiss() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);

        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('w')));
        assert_eq!(app.mode, AppMode::WorkspaceSwitcher);
        assert!(app.workspace_switcher.is_some());

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.workspace_switcher.is_none());
    }

    #[test]
    fn test_workspace_switcher_no_workspace() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // No workspaces → prefix w should not open switcher
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('w')));
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.workspace_switcher.is_none());
    }

    // ── Previous workspace toggle tests ──

    #[test]
    fn test_toggle_previous_workspace_round_trip() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app); // index 0
        add_test_workspace(&mut app); // index 1
        app.switch_workspace(1);
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.previous_workspace, Some(0));

        // prefix ` toggles back to 0
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('`')));
        assert_eq!(app.active_workspace, 0);
        assert_eq!(app.previous_workspace, Some(1));

        // prefix ` toggles back to 1
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('`')));
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.previous_workspace, Some(0));
    }

    #[test]
    fn test_toggle_previous_workspace_none() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        assert!(app.previous_workspace.is_none());

        // prefix ` with no previous → nothing changes
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('`')));
        assert_eq!(app.active_workspace, 0);
        assert!(app.previous_workspace.is_none());
    }

        #[test]
    fn test_scroll_mode_requires_pty() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app); // test workspace has no PTY tabs

        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('[')));
        assert_eq!(app.input_state, InputState::Normal);
        assert!(app.toast.is_some());
    }

    #[test]
    fn test_scroll_mode_exit_keys() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        // Force the state (entering requires a PTY, which tests can't spawn)
        app.input_state = InputState::TermScroll;

        crate::input::handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(app.input_state, InputState::Normal);

        app.input_state = InputState::TermScroll;
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('q')));
        assert_eq!(app.input_state, InputState::Normal);

        // The prefix key inside scroll mode exits and arms the prefix
        app.input_state = InputState::TermScroll;
        crate::input::handle_key_event(&mut app, ctrl('g'));
        assert_eq!(app.input_state, InputState::PrefixPending);
    }

    #[test]
    fn test_paste_cancels_pending_prefix() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        assert_eq!(app.input_state, InputState::PrefixPending);

        crate::input::handle_paste(&mut app, "hello");
        assert_eq!(app.input_state, InputState::Normal);
    }

    #[test]
    fn test_direct_app_binding_override() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        // Promote 'help' to a direct Alt+H chord via config override
        app.config.keybindings.app.insert(
            "help".to_string(),
            crate::config::BindingValue::one("alt-h"),
        );

        let alt_h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT);
        crate::input::handle_key_event(&mut app, alt_h);
        assert_eq!(app.mode, AppMode::Help);
        assert_eq!(app.input_state, InputState::Normal);
    }

    // ── prefix g: open-or-focus the lazygit tab ──

    #[test]
    fn test_prefix_g_without_workspace_toasts() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('g')));
        assert!(action.is_none());
        assert!(app.toast.is_some());
        assert_eq!(app.input_state, InputState::Normal);
    }

    #[test]
    fn test_prefix_g_spawns_git_tab() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('g')));
        assert!(matches!(
            action,
            Some(crate::action::Action::SpawnTab(piki_core::AIProvider::Git))
        ));
        assert_eq!(app.active_pane, ActivePane::MainPanel);
    }

    #[test]
    fn test_prefix_g_respawns_dead_git_tab() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let idx = add_test_workspace(&mut app);
        // A Git tab with no live PTY counts as dead → close + respawn
        app.workspaces[idx].add_tab(piki_core::AIProvider::Git, true, None);
        let tabs_before = app.workspaces[idx].tabs.len();

        crate::input::handle_key_event(&mut app, ctrl('g'));
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Char('g')));
        assert!(matches!(
            action,
            Some(crate::action::Action::SpawnTab(piki_core::AIProvider::Git))
        ));
        assert_eq!(app.workspaces[idx].tabs.len(), tabs_before - 1);
    }

    #[test]
    fn test_prefix_f_opens_terminal_search() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let ws_idx = add_test_workspace(&mut app);
        add_terminal_tab(&mut app, ws_idx);
        assert!(app.term_search.is_none());

        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('f')));
        assert!(
            app.term_search.is_some(),
            "Ctrl+G f should open the terminal search overlay over a terminal tab"
        );
    }

    #[test]
    fn test_prefix_f_noop_without_terminal() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        // A workspace with no tabs → nothing to search.
        add_test_workspace(&mut app);
        assert!(app.term_search.is_none());

        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('f')));
        assert!(
            app.term_search.is_none(),
            "Ctrl+G f should not open terminal search when no terminal is active"
        );
    }

    // ── Agents pane ──

    fn add_agent_tab(app: &mut App, ws_idx: usize, name: &str) {
        app.workspaces[ws_idx].add_tab(AIProvider::Custom(name.to_string()), true, None);
    }

    #[test]
    fn test_agent_rows_spans_all_workspaces() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Claude");
        add_agent_tab(&mut app, b, "Codex");
        add_agent_tab(&mut app, b, "Claude");
        // Shell/Git tabs only count as agents once their cli-agent channel
        // reports (manual `claude` inside them); without one they're excluded
        app.workspaces[a].add_tab(AIProvider::Shell, true, None);
        app.workspaces[b].add_tab(AIProvider::Git, true, None);

        let rows = app.agent_rows();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|&(wi, ti)| {
            matches!(
                app.workspaces[wi].tabs[ti].provider,
                AIProvider::Custom(_)
            )
        }));
    }

    #[test]
    fn test_agent_row_at_maps_click_to_index() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Claude");
        add_agent_tab(&mut app, a, "Codex");
        add_agent_tab(&mut app, a, "Gemini");
        // Pane at y=10, height 5 → border at row 10, content rows 11..=13
        app.agents_area = ratatui::layout::Rect::new(0, 10, 20, 5);

        assert_eq!(app.agent_row_at(10), None, "border row is not an agent");
        assert_eq!(app.agent_row_at(11), Some(0));
        assert_eq!(app.agent_row_at(12), Some(1));
        assert_eq!(app.agent_row_at(13), Some(2));
        assert_eq!(app.agent_row_at(9), None, "above the pane");
    }

    #[test]
    fn test_agent_row_at_accounts_for_scroll() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        for n in 0..6 {
            add_agent_tab(&mut app, a, &format!("Agent{n}"));
        }
        // visible height = 2 (height 4 minus borders); select the last row so
        // the render scrolls it into view (scroll_offset = 4, showing 4 & 5).
        app.agents_area = ratatui::layout::Rect::new(0, 0, 20, 4);
        app.selected_agent_row = 5;
        assert_eq!(app.agent_row_at(1), Some(4), "first visible row after scroll");
        assert_eq!(app.agent_row_at(2), Some(5), "selected row scrolled into view");
        assert_eq!(app.agent_row_at(3), None, "below the pane border");
    }

    #[test]
    fn test_agent_row_at_empty_is_none() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.agents_area = ratatui::layout::Rect::new(0, 0, 20, 5);
        assert_eq!(app.agent_row_at(1), None);
    }

    #[test]
    fn test_agents_pane_navigation_and_jump() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Claude");
        add_agent_tab(&mut app, b, "Codex");
        app.active_pane = ActivePane::Agents;
        app.active_workspace = a;

        // j moves the selection down
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected_agent_row, 1);
        // Arrow keys navigate too: Up back to row 0, Down to row 1
        crate::input::handle_key_event(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected_agent_row, 0);
        crate::input::handle_key_event(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected_agent_row, 1);
        // Enter jumps to workspace b's agent tab and focuses the main panel
        crate::input::handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(app.active_workspace, b);
        assert_eq!(app.active_pane, ActivePane::MainPanel);
        let ws = &app.workspaces[b];
        assert!(matches!(
            ws.tabs[ws.active_tab].provider,
            AIProvider::Custom(_)
        ));
    }

    #[test]
    fn test_sidebar_cursor_switches_the_active_workspace() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        app.switch_workspace(a);

        // Follow-focus: j/k don't just move a cursor, they move the workspace
        // every action targets — so `prefix c` right after lands its tab here.
        app.select_next_sidebar_row();
        assert_eq!(app.selected_workspace, b);
        assert_eq!(app.active_workspace, b);

        app.select_prev_sidebar_row();
        assert_eq!(app.active_workspace, a);
    }

    #[test]
    fn test_sidebar_cursor_on_a_worktree_family_parent_switches_to_it() {
        // Every row is a real workspace now (no synthetic header), so the
        // cursor always follows onto whatever it lands on — including a
        // worktree-family parent row.
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        let parent = add_test_workspace(&mut app);
        let child = add_test_workspace(&mut app);
        let shared_repo = app.workspaces[parent].info.source_repo.clone();
        app.workspaces[child].info.source_repo = shared_repo.clone();
        app.workspaces[child].info.workspace_type = piki_core::WorkspaceType::Worktree;
        app.switch_workspace(a);

        // Rows are [Workspace a, Workspace parent (collapsible), Workspace child].
        app.select_next_sidebar_row();
        assert!(matches!(
            app.sidebar_items()[app.selected_sidebar_row],
            SidebarItem::Workspace {
                collapsed: Some(false),
                ..
            }
        ));
        assert_eq!(app.active_workspace, parent);

        app.select_next_sidebar_row();
        assert_eq!(app.active_workspace, child);
    }

    #[test]
    fn test_agent_selection_follows_the_active_agent_tab() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Antigravity");
        add_agent_tab(&mut app, b, "Claude Code");

        app.active_workspace = a;
        app.sync_agent_selection();
        assert_eq!(app.selected_agent_row, 0);

        // Opening/switching to workspace b's agent moves the highlight with it
        app.switch_workspace(b);
        app.sync_agent_selection();
        assert_eq!(app.selected_agent_row, 1);
    }

    #[test]
    fn test_agent_selection_does_not_fight_pane_browsing() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Antigravity");
        add_agent_tab(&mut app, b, "Claude Code");
        app.active_workspace = b;
        app.sync_agent_selection();
        assert_eq!(app.selected_agent_row, 1);

        // The user browses the pane with j/k; the active tab didn't change, so
        // the next loop iteration must leave their cursor alone.
        app.selected_agent_row = 0;
        app.sync_agent_selection();
        assert_eq!(app.selected_agent_row, 0);
    }

    #[test]
    fn test_agent_selection_kept_on_a_non_agent_tab() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let a = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Antigravity");
        let b = add_test_workspace(&mut app);
        add_agent_tab(&mut app, b, "Claude Code");
        app.active_workspace = b;
        app.sync_agent_selection();
        assert_eq!(app.selected_agent_row, 1);

        // A shell tab has no row of its own — the highlight stays put rather
        // than snapping back to some other workspace's agent.
        app.workspaces[b].add_tab(AIProvider::Shell, true, None);
        app.sync_agent_selection();
        assert_eq!(app.selected_agent_row, 1);
    }

    #[test]
    fn test_agents_pane_empty_is_noop() {
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::Agents;

        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected_agent_row, 0);
        let action = crate::input::handle_key_event(&mut app, key(KeyCode::Enter));
        assert!(action.is_none());
        assert_eq!(app.mode, AppMode::Normal);
    }

    // ── Symmetric pane navigation tests ──

    #[test]
    fn test_pane_nav_main_panel_h_to_workspace_list() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        app.active_pane = ActivePane::MainPanel;
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    #[test]
    fn test_pane_nav_main_panel_j_to_git_status() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        app.active_pane = ActivePane::MainPanel;
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.active_pane, ActivePane::Agents);
    }

    #[test]
    fn test_pane_nav_main_panel_k_to_workspace_list() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        app.active_pane = ActivePane::MainPanel;
        crate::input::handle_key_event(&mut app, ctrl('g'));
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    // ── Workspace switch + focus tests ──

    #[test]
    fn test_workspace_list_keyboard_nav() {
        // The focused workspace list is keyboard-navigable: up/down (j/k or the
        // arrows) move the selection and Enter switches to the selected
        // workspace. Heavier actions (new/edit/delete) still go through the
        // prefix; bare letter keys are no-ops here.
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app); // index 0 → sidebar row 0
        add_test_workspace(&mut app); // index 1 → sidebar row 1
        app.active_pane = ActivePane::WorkspaceList;
        app.selected_sidebar_row = 0;

        // Down (j) moves the selection to row 1.
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected_sidebar_row, 1);
        // Arrow Up moves back to row 0.
        crate::input::handle_key_event(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected_sidebar_row, 0);
        // A bare letter key is a no-op (not a prefix chord).
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('e')));
        assert_eq!(app.selected_sidebar_row, 0);

        // Arrow Down then Enter switches to the selected workspace, and focus
        // stays on the list so navigation can continue.
        crate::input::handle_key_event(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected_sidebar_row, 1);
        crate::input::handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.active_dialog.is_none());
    }

    #[test]
    fn test_workspace_list_collapse_expand_group() {
        // Side arrows (or h/l) collapse/expand a worktree family's children
        // while the selection sits on the family's parent row.
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let parent = add_test_workspace(&mut app); // index 0
        let child = add_test_workspace(&mut app); // index 1
        let shared_repo = app.workspaces[parent].info.source_repo.clone();
        app.workspaces[child].info.source_repo = shared_repo;
        app.workspaces[child].info.workspace_type = piki_core::WorkspaceType::Worktree;
        app.active_pane = ActivePane::WorkspaceList;
        let fam_key = family_key(&app.workspaces[parent].info);
        // sidebar_items: [Workspace{parent, collapsed:Some(false)}, Workspace{child}]
        app.selected_sidebar_row = 0; // the family parent row

        // h collapses the family, hiding the child row.
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert!(app.collapsed_groups.contains(&fam_key));
        assert_eq!(app.sidebar_items().len(), 1);
        // l re-expands it.
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert!(!app.collapsed_groups.contains(&fam_key));
        assert_eq!(app.sidebar_items().len(), 2);
        // Arrow Left collapses again.
        crate::input::handle_key_event(&mut app, key(KeyCode::Left));
        assert!(app.collapsed_groups.contains(&fam_key));
        // Arrow Right expands.
        crate::input::handle_key_event(&mut app, key(KeyCode::Right));
        assert!(!app.collapsed_groups.contains(&fam_key));
    }

    // ── PTY idle notifications ──

    #[test]
    fn switch_workspace_clears_has_idle_notification() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        app.active_workspace = a;
        app.workspaces[b].has_idle_notification = true;

        app.switch_workspace(b);

        assert!(!app.workspaces[b].has_idle_notification);
        assert_eq!(app.active_workspace, b);
    }

    #[test]
    fn switch_workspace_to_same_index_still_clears_badge() {
        // Edge case: re-entering the active workspace acknowledges any
        // notifications that fired while it was visible (e.g. the active
        // tab went idle while the user was in another pane).
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        app.active_workspace = a;
        app.workspaces[a].has_idle_notification = true;

        app.switch_workspace(a);

        assert!(!app.workspaces[a].has_idle_notification);
    }

    // ── active_pane sync on workspace switch ──

    #[test]
    fn bare_switch_workspace_leaves_active_pane_untouched() {
        // Regression guard: the startup-restore path in event_loop.rs relies
        // on switch_workspace NOT touching active_pane.
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::WorkspaceList;

        app.switch_workspace(1);

        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
    }

    #[test]
    fn next_workspace_focuses_main_panel() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::WorkspaceList;

        app.next_workspace();

        assert_eq!(app.active_pane, ActivePane::MainPanel);
    }

    #[test]
    fn prev_workspace_focuses_main_panel() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::WorkspaceList;

        app.prev_workspace();

        assert_eq!(app.active_pane, ActivePane::MainPanel);
    }

    #[test]
    fn toggle_previous_workspace_focuses_main_panel() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        app.active_workspace = a;
        app.switch_workspace(b); // sets previous_workspace = Some(a), active_pane untouched
        app.active_pane = ActivePane::WorkspaceList;

        app.toggle_previous_workspace();

        assert_eq!(app.active_workspace, a);
        assert_eq!(app.active_pane, ActivePane::MainPanel);
    }
}
