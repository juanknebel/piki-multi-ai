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
    /// Project-wide content search overlay (ripgrep)
    ProjectSearch,
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
    /// Destination picker for moving a tab to another workspace
    MoveTab,
    /// Confirmation dialog for quitting the application
    ConfirmQuit,
    /// Workspace dashboard overview
    Dashboard,
    /// Session-daemon overlay (persistent sessions management)
    Sessions,
    /// Projects overlay (cross-repo groups of workspaces/directories) —
    /// covers both the list and its edit sub-dialog
    Projects,
    /// Internal log viewer
    Logs,
    /// Command palette overlay
    CommandPalette,
    /// Submit review overlay (code review)
    SubmitReview,
    /// PR picker overlay — lists PRs relevant to the current `gh` user,
    /// independent of any workspace, entry point for the Code Review flow
    PrPicker,
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
    /// Rename current tab
    RenameTab,
    /// Global "scratch" terminal overlay: a single shell rooted at `$HOME`,
    /// tied to no workspace, drawn centered on top of everything. Persists
    /// its PTY when hidden (see [`ScratchTerminal`]).
    ScratchTerminal,
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

/// The sidebar's row model: the three-level project tree, built in core by
/// [`piki_core::projects::tree::project_tree`] from the cached project list
/// (`App::sidebar_projects`) plus the live workspace list. There is no second
/// row model and no flat workspace view — see `App::sidebar_rows`.
pub use piki_core::projects::tree::{Bucket, ProjectTreeRow};

/// What the sidebar cursor stands on, flattened to a copyable tag. Drives the
/// footer hints (each row kind offers different actions) and is part of the
/// footer cache key — `ProjectTreeRow` itself carries paths and isn't `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarRowKind {
    /// Nothing selected (empty tree).
    #[default]
    None,
    /// A real project header.
    Project,
    /// A synthetic bucket header (PR review / no project): no project to edit.
    Bucket,
    /// A repo group — the row "new worktree" and "add repo" act on.
    Repo,
    /// A loaded workspace.
    Checkout,
    /// A plain-directory member.
    Dir,
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

/// Result of one jq run: a filtered body per response, or jq's message.
pub type JqResult = Result<Vec<String>, String>;

/// jq filter bar over the API response bodies. The raw responses are never
/// modified — the filtered text lives in `ApiTabState::jq_output` — so
/// dropping the filter is just forgetting that.
pub struct ApiJqState {
    pub query: String,
    pub cursor: usize,
    /// jq's own message from the last run (bad filter, missing binary),
    /// shown in the bar instead of a result.
    pub error: Option<String>,
    /// A run is in flight.
    pub running: bool,
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
    /// jq filter bar (None = closed). Mutually exclusive with `search`:
    /// both draw over the same row at the bottom of the response panel.
    pub jq: Option<ApiJqState>,
    /// Filtered body per response while a jq filter is applied; `None` shows
    /// the raw responses.
    pub jq_output: Option<Vec<String>>,
    /// Slot the jq task writes its result (or error) into, polled by the
    /// event loop — same shape as `pending_responses`.
    pub pending_jq: Arc<Mutex<Option<JqResult>>>,
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
            jq: None,
            jq_output: None,
            pending_jq: Arc::new(Mutex::new(None)),
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
    /// PTY byte count as of the last passive agent-state scrape — the
    /// event loop skips the (parser-lock + full-screen-sample) detection
    /// for this tab when no new output has arrived since
    pub last_detect_bytes: u64,
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
    /// Custom title set by the user via rename (takes precedence over
    /// `markdown_label` and `provider.label()`).
    pub custom_title: Option<String>,
    /// Daemon session id for a persistent (Remote) PTY tab; `None` for
    /// in-process (Local) tabs, non-PTY tabs, and markdown tabs. Used to
    /// re-attach on restart and to remove the session on close.
    pub session_id: Option<String>,
}

impl Tab {
    /// Display label: custom title > markdown label > provider label.
    pub fn display_label(&self) -> &str {
        if let Some(custom) = self.custom_title.as_deref()
            && !custom.trim().is_empty()
        {
            return custom;
        }
        if let Some(md) = self.markdown_label.as_deref() {
            return md;
        }
        self.provider.label()
    }

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
        let agent = piki_core::cli_agent::cli_agent_of(&guard.state)?;
        Some((
            agent.status,
            agent.last_attention_at.is_some(),
            agent.last_summary.clone(),
        ))
    }

    /// How long the tab's agent run has been going (`3m 12s` in the Agents
    /// pane), if one is in flight.
    pub fn cli_agent_elapsed(&self) -> Option<std::time::Duration> {
        let shell = self.pty_session.as_ref()?.shell()?;
        let guard = shell.lock();
        piki_core::cli_agent::cli_agent_of(&guard.state)?.elapsed()
    }
}

/// The global "scratch" terminal — a single shell rooted at `$HOME`, owned
/// by no workspace, shown as a centered overlay on top of everything and
/// toggled by `prefix C-t` from anywhere. Its PTY is in-process (dies with
/// the app) and keeps running while the overlay is hidden, so re-opening is
/// instant. Lives as a top-level `App` field, like [`ChatPanelState`].
#[derive(Default)]
pub struct ScratchTerminal {
    pub pty_session: Option<PtySession>,
    pub pty_parser: Option<Arc<Mutex<vt100::Parser>>>,
    /// Whether the overlay is currently shown (the PTY outlives this).
    pub visible: bool,
    /// A `prefix` chord was pressed while the overlay had focus — the next
    /// key is dispatched as a mini prefix (so `prefix C-t` hides it).
    pub prefix_pending: bool,
    /// PTY byte counter as of the last render, for the redraw check.
    pub last_bytes_processed: u64,
    /// Scrollback offset: 0 = live view, N = N lines back (mouse wheel).
    pub term_scroll: usize,
    /// Mouse text selection inside the overlay (own field — `App.selection`
    /// is consumed by `render_main_content` before this overlay renders).
    pub selection: Option<Selection>,
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
    /// Runtime-only: true for a restored `ephemeral` (PR review) workspace
    /// whose checkout directory was missing on load. Not persisted — it's
    /// recomputed at startup and cleared (or re-set) by
    /// `Action::RetryReviewCheckout` when the user opens it.
    pub review_broken: bool,
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

/// Precedence for an agent (status, attention) pair — lives in core
/// ([`piki_core::cli_agent::status_severity`]) so the desktop frontend ranks
/// agents identically. Used by `Workspace::agent_status_rollup()`
/// (per-workspace) and the sidebar's worktree-family aggregation.
pub(crate) use piki_core::cli_agent::status_severity as agent_status_severity;

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
            review_broken: false,
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
            last_detect_bytes: 0,
            idle_watcher,
            markdown_content: None,
            markdown_label: None,
            markdown_scroll: 0,
            markdown_rendered: None,
            api_state: None,
            custom_title: None,
            session_id: None,
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
            last_detect_bytes: 0,
            idle_watcher: None,
            markdown_content: Some(content),
            markdown_label: Some(label),
            markdown_scroll: 0,
            markdown_rendered: Some(rendered),
            api_state: None,
            custom_title: None,
            session_id: None,
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

/// State for the project-wide content search overlay (ripgrep via
/// `piki_core::search`, shared with the desktop's Search-in-Project).
///
/// Results arrive asynchronously: each query edit bumps `generation` and
/// spawns a debounced search task; the task publishes into `shared` only if
/// its generation is still current, so stale results can never clobber a
/// newer query. The render loop's tick picks the results up.
pub struct ProjectSearchState {
    pub query: String,
    pub selected: usize,
    /// Worktree root captured at open time.
    pub root: std::path::PathBuf,
    pub generation: Arc<std::sync::atomic::AtomicU64>,
    pub shared: Arc<parking_lot::Mutex<ProjectSearchShared>>,
}

#[derive(Default)]
pub struct ProjectSearchShared {
    pub hits: Vec<piki_core::search::SearchMatch>,
    /// Generation of the query that produced `hits`.
    pub done_generation: u64,
}

impl ProjectSearchState {
    /// Debounce before spawning rg — batches a fast typist's keystrokes.
    const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(180);
    const MAX_HITS: usize = 200;

    /// True while a newer query's results haven't landed yet.
    pub fn searching(&self) -> bool {
        !self.query.is_empty()
            && self.shared.lock().done_generation
                < self.generation.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The hit under the cursor (clamped), as (relative path, line).
    pub fn selected_hit(&self) -> Option<(String, u32)> {
        let shared = self.shared.lock();
        let idx = self.selected.min(shared.hits.len().saturating_sub(1));
        shared.hits.get(idx).map(|h| (h.path.clone(), h.line_num))
    }

    /// Re-run the search for the current query (debounced, generation-gated).
    pub fn query_changed(&mut self) {
        use std::sync::atomic::Ordering;
        self.selected = 0;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if self.query.is_empty() {
            let mut shared = self.shared.lock();
            shared.hits.clear();
            shared.done_generation = generation;
            return;
        }
        let query = self.query.clone();
        let root = self.root.clone();
        let gen_handle = Arc::clone(&self.generation);
        let shared = Arc::clone(&self.shared);
        tokio::spawn(async move {
            tokio::time::sleep(Self::DEBOUNCE).await;
            if gen_handle.load(Ordering::SeqCst) != generation {
                return; // superseded while debouncing
            }
            let hits = piki_core::search::project_search(&root, &query, Self::MAX_HITS)
                .await
                .unwrap_or_default();
            if gen_handle.load(Ordering::SeqCst) == generation {
                let mut s = shared.lock();
                s.hits = hits;
                s.done_generation = generation;
            }
        });
    }
}

/// State for the inline file editor
pub struct EditorState {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
    /// Buffer contents as of open/last save — `is_dirty` compares against it.
    baseline: Vec<String>,
    /// One-shot confirm: set when exit was pressed on a dirty buffer; the
    /// next exit discards, any other key disarms.
    pub pending_discard: bool,
}

impl EditorState {
    pub fn new(content: &str) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        Self {
            baseline: lines.clone(),
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            pending_discard: false,
        }
    }

    /// True when the buffer differs from the last opened/saved contents.
    pub fn is_dirty(&self) -> bool {
        self.lines != self.baseline
    }

    /// Re-baseline after a successful save.
    pub fn mark_saved(&mut self) {
        self.baseline = self.lines.clone();
        self.pending_discard = false;
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
    /// (workspace index, tab id) the selection was made on. Cell coordinates
    /// are meaningless on any other tab, so the selection is dropped as soon
    /// as the active tab stops matching (see `App::drop_stale_selection`).
    pub owner: (usize, usize),
}

impl Selection {
    pub fn new(row: u16, col: u16, owner: (usize, usize)) -> Self {
        Self {
            anchor_row: row,
            anchor_col: col,
            end_row: row,
            end_col: col,
            active: true,
            owner,
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

/// Cached footer keys: (mode, input_state, active_pane, has_markdown, has_kanban, api_footer_state, new_tab_menu, sidebar_row_kind, keys)
/// api_footer_state: 0 = no API tab, 1 = API tab, 2 = API tab with search open
/// new_tab_menu: 0 = N/A, 1 = Main, 2 = Agents, 3 = Tools
/// sidebar_row_kind: what the sidebar cursor stands on, since the project tree
/// offers different actions per row kind (see `App::sidebar_row_kind`)
pub type FooterCache = (
    AppMode,
    InputState,
    ActivePane,
    bool,
    bool,
    u8,
    u8,
    SidebarRowKind,
    Vec<(String, &'static str)>,
);

/// Background result slot for `Action::LoadPrList`.
pub type PendingPrList = Arc<Mutex<Option<Result<Vec<piki_core::github::PrListItem>, String>>>>;
/// Background result slot for `Action::LoadSessions` (sessions overlay).
pub type PendingSessionsList =
    Arc<Mutex<Option<Result<Vec<piki_core::session::protocol::SessionInfo>, String>>>>;
/// Background result slot for `Action::OpenPrReview`.
pub type PendingPrCheckout =
    Arc<Mutex<Option<Result<crate::code_review::ReviewSessionData, String>>>>;
/// Background result slot for `Action::LoadRepoPrs`. Carries the repo
/// alongside the result so a stale response (user re-typed a different repo
/// before this one landed) can be told apart from the current query.
pub type PendingRepoPrs =
    Arc<Mutex<Option<(String, Result<Vec<piki_core::github::PrListItem>, String>)>>>;
/// Background result slot for `Action::RetryReviewCheckout`. Carries the
/// target workspace index alongside the result.
pub type PendingReviewRetry =
    Arc<Mutex<Option<(usize, Result<piki_core::github::PrCheckout, String>)>>>;

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
    /// Collapsed keys of the sidebar tree: project/bucket headers and repo
    /// groups alike (`Project::collapse_key`, `tree::repo_collapse_key`,
    /// `PR_REVIEW_KEY`, `UNASSIGNED_KEY`). Persisted, so a collapsed project
    /// stays collapsed across restarts.
    pub collapsed_groups: std::collections::HashSet<String>,
    /// Cursor over `sidebar_rows()` — the project tree's rows.
    pub selected_sidebar_row: usize,
    /// Wheel-viewport offset for the sidebar (visual rows). The wheel
    /// scrolls this without touching the selection; selection moves pull it
    /// along via `reveal_sidebar_selection`.
    pub sidebar_scroll: usize,
    /// Same for the Agents pane (agent rows); see `reveal_agent_selection`.
    pub agents_scroll: usize,
    /// The project list the sidebar tree is built from. Loaded from storage at
    /// startup and reloaded after every project mutation — renders stay pure.
    /// Members resolve against `workspaces` at render time.
    pub sidebar_projects: Vec<piki_core::projects::Project>,
    /// Project id a workspace being created should join, set by the sidebar's
    /// `add_repo` key before it opens the New Workspace dialog and consumed by
    /// `finish_workspace_creation`. That is what makes "add a repository to
    /// this project" one gesture instead of create-then-edit-membership.
    pub pending_project_member: Option<i64>,
    pub status_message: Option<String>,
    /// Toast notification (replaces status_message for timed display)
    pub toast: Option<Toast>,
    /// Fuzzy file search state
    pub fuzzy: Option<FuzzyState>,

    /// Project-wide content search overlay state
    pub project_search: Option<ProjectSearchState>,
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
    /// Inner area of the scratch-terminal overlay (for mouse hit-testing),
    /// set by its render fn.
    pub scratch_inner_area: Option<Rect>,
    /// Inner area of the API response panel (for mouse hit-testing)
    pub api_response_inner_area: Option<Rect>,
    /// Inner area of the chat messages panel (for mouse hit-testing)
    pub chat_messages_inner_area: Option<Rect>,
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
    /// Last time passive agent-state detection ran — throttles the
    /// screen-scrape sweep to `PASSIVE_DETECT_INTERVAL`
    pub last_passive_detect: Instant,
    /// External claude agents discovered via /proc scan (TUI only, no desktop yet)
    pub external_agents: Vec<piki_core::external_agents::AgentTree>,
    /// Last time external agent scan ran (throttled to 1s)
    pub last_external_scan: Instant,
    /// App-wide coalesced "PTY produced output" signal. Cloned into every
    /// spawned session; the event loop sleeps on it instead of polling byte
    /// counters at the tick rate.
    pub pty_output: piki_core::pty::PtyOutputSignal,
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
    /// Background result slot for `Action::LoadPrList`, polled in
    /// `event_loop.rs`'s tick. `Some` once the spawned task finishes.
    pub pending_pr_list: PendingPrList,
    /// Background result slot for `Action::LoadSessions` (sessions overlay).
    pub pending_sessions_list: PendingSessionsList,
    /// Background result slot for `Action::OpenPrReview`.
    pub pending_pr_checkout: PendingPrCheckout,
    /// Background result slot for `Action::LoadRepoPrs`.
    pub pending_repo_prs: PendingRepoPrs,
    /// Background result slot for `Action::RetryReviewCheckout`.
    pub pending_review_retry: PendingReviewRetry,
    /// Storage backend (SQLite)
    pub storage: std::sync::Arc<piki_core::storage::AppStorage>,
    /// Cached agent profiles for the current project
    pub agent_profiles: Vec<piki_core::storage::AgentProfile>,
    /// User-configurable providers loaded from providers.toml
    pub provider_manager: piki_core::providers::ProviderManager,
    /// Chat LLM providers (Ollama / llama.cpp / OpenRouter) estilo providers.toml
    pub chat_provider_manager: piki_core::chat_providers::ChatProviderManager,
    /// Data paths for saving config files
    pub paths: piki_core::paths::DataPaths,
    /// Handle to the persistent-session daemon, when one is reachable. `None`
    /// means sessions are disabled or the daemon is unavailable — every tab
    /// then falls back to an in-process (Local) PTY.
    pub session_daemon: Option<piki_core::session::client::Daemon>,
    /// Global AI chat panel state (persists when overlay is hidden)
    pub chat_panel: ChatPanelState,
    /// Global scratch-terminal overlay state (persists when hidden)
    pub scratch: ScratchTerminal,
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
    /// Filter typed in ModelSelect (live search)
    pub model_filter: String,
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
    /// Abort handle for the in-flight stream/agent task (Ctrl+C stop,
    /// watchdog timeout).
    pub stream_abort: Option<tokio::task::AbortHandle>,
    /// Last stream event time — the tick watchdog unlocks the panel when a
    /// stream goes quiet without a terminal event.
    pub last_stream_activity: Option<std::time::Instant>,
    /// One-shot confirm: first Ctrl+L arms, the second clears. Any other key
    /// disarms.
    pub pending_clear: bool,
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
        let mut config = crate::config::Config::load_from(paths);
        // Fold in the choices made in the desktop's Settings ▸ General tab
        // (shared SQLite DB, `piki_core::app_settings`): DB override >
        // config.toml > default. After this line `config.sessions.enabled`
        // (read by `event_loop::run` before connecting the daemon) and
        // `config.notifications` are the effective values.
        let overrides = piki_core::app_settings::AppSettings::load(storage.ui_prefs.as_deref());
        config.sessions.enabled = overrides.sessions_enabled(config.sessions.enabled);
        config.notifications = overrides.notifications(config.notifications);
        // Propagate notification prefs to the shared core layer (process
        // globals — the notify_* helpers read them on every event).
        config.notifications.apply();
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
            sidebar_scroll: 0,
            agents_scroll: 0,
            sidebar_projects: Vec::new(),
            pending_project_member: None,
            status_message: None,
            toast: None,
            fuzzy: None,
            project_search: None,
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
            scratch_inner_area: None,
            api_response_inner_area: None,
            chat_messages_inner_area: None,
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
            last_passive_detect: Instant::now(),
            external_agents: Vec::new(),
            last_external_scan: Instant::now() - std::time::Duration::from_secs(2),
            pty_output: piki_core::pty::PtyOutputSignal::new(),
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
            pending_pr_list: Arc::new(Mutex::new(None)),
            pending_sessions_list: Arc::new(Mutex::new(None)),
            pending_pr_checkout: Arc::new(Mutex::new(None)),
            pending_repo_prs: Arc::new(Mutex::new(None)),
            pending_review_retry: Arc::new(Mutex::new(None)),
            storage,
            agent_profiles: Vec::new(),
            provider_manager: piki_core::providers::ProviderManager::load_or_init(
                &paths.providers_path(),
            ),
            chat_provider_manager: piki_core::chat_providers::ChatProviderManager::load_or_init(
                &paths.chat_providers_path(),
            ),
            paths: paths.clone(),
            session_daemon: None,
            chat_panel: ChatPanelState::default(),
            scratch: ScratchTerminal::default(),
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

    /// Stop the in-flight chat stream (if any): abort the task, keep the
    /// partial response as an assistant message, and unlock the input. Safe
    /// to call when nothing is streaming.
    pub fn chat_stop_stream(&mut self) {
        if let Some(handle) = self.chat_panel.stream_abort.take() {
            handle.abort();
        }
        if !self.chat_panel.current_response.is_empty() {
            let partial = std::mem::take(&mut self.chat_panel.current_response);
            self.chat_panel.messages.push(piki_core::chat::ChatMessage {
                role: piki_core::chat::ChatRole::Assistant,
                content: partial,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        self.chat_panel.streaming = false;
        self.chat_panel.agent_tool_status = None;
        self.chat_panel.last_stream_activity = None;
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

    /// Push an ephemeral PR-review `WorkspaceInfo` (see
    /// `WorkspaceManager::create_review_workspace`), switch focus to it, and
    /// start its file watcher. Unlike `finish_workspace_creation`
    /// (`action/workspace.rs`) this never persists — ephemeral workspaces
    /// are filtered out of `persistable_workspaces()` by design. Returns the
    /// new workspace's index.
    pub fn open_review_workspace(&mut self, mut info: piki_core::WorkspaceInfo) -> usize {
        info.order = self
            .workspaces
            .iter()
            .map(|w| w.info.order)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        self.workspaces.push(Workspace::from_info(info));
        let idx = self.workspaces.len() - 1;
        self.switch_workspace_and_focus(idx);
        let ws = &mut self.workspaces[idx];
        if let Ok(watcher) =
            piki_core::workspace::FileWatcher::new(ws.path.clone(), ws.name.clone())
        {
            ws.watcher = Some(watcher);
        }
        idx
    }

    /// `WorkspaceInfo`s to hand to `save_workspaces`. `ephemeral` (PR review)
    /// workspaces are included — they survive a restart, restored under the
    /// synthetic PR-review bucket (see `sidebar_rows`).
    pub fn persistable_workspaces(&self) -> Vec<piki_core::WorkspaceInfo> {
        self.workspaces.iter().map(|w| w.info.clone()).collect()
    }

    /// Workspace indices in sidebar order, each one once — what `}`/`{` cycle
    /// through. A workspace that belongs to two projects has two rows in the
    /// tree but must not be visited twice, so the first row wins.
    fn cycle_order(&self) -> Vec<usize> {
        let mut seen = std::collections::HashSet::new();
        self.sidebar_rows()
            .iter()
            .filter_map(|row| row.workspace_index())
            .filter(|idx| seen.insert(*idx))
            .collect()
    }

    pub fn next_workspace(&mut self) {
        let visible = self.cycle_order();
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
        let visible = self.cycle_order();
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
        let scroll_offset = self.agents_viewport();
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
    /// (workspace index, tab id) of the tab currently on screen — the identity
    /// a text selection is owned by. Tab *id* (not position) so closing or
    /// reordering sibling tabs can't make a stale selection look current.
    pub fn selection_owner_key(&self) -> Option<(usize, usize)> {
        self.current_workspace()
            .and_then(|ws| ws.tabs.get(ws.active_tab))
            .map(|tab| (self.active_workspace, tab.id))
    }

    /// Drop the text selection when it no longer belongs to the tab on screen.
    ///
    /// Selections are plain cell rectangles; after a tab or workspace switch
    /// the same rectangle silently addresses the *new* tab's content, so a
    /// leftover selection both renders a bogus highlight and — worse — gets
    /// re-copied from the wrong tab by the next mouse-up. Called once per
    /// event-loop iteration instead of at every tab-switch site (there are
    /// many; see `sync_agent_selection` for the same reasoning).
    pub fn drop_stale_selection(&mut self) {
        // Chat panel selection is global (overlay), not tied to a tab - don't drop it while chat is open
        if self.mode == AppMode::ChatPanel {
            return;
        }
        if let Some(ref sel) = self.selection
            && Some(sel.owner) != self.selection_owner_key()
        {
            self.selection = None;
        }
    }

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
            self.reveal_agent_selection();
        }
    }

    /// The sidebar's rows: the project tree, built in core from the cached
    /// project list plus the live workspace list (`projects::tree`). This is
    /// the ONE row model — there is no flat workspace view to fall back to,
    /// and every registered workspace is reachable through it (a project, or
    /// one of the two synthetic buckets).
    pub fn sidebar_rows(&self) -> Vec<ProjectTreeRow> {
        let infos: Vec<piki_core::WorkspaceInfo> =
            self.workspaces.iter().map(|w| w.info.clone()).collect();
        piki_core::projects::tree::project_tree(
            &self.sidebar_projects,
            &infos,
            &self.collapsed_groups,
        )
    }

    /// Indices of workspaces that have at least one open tab, in sidebar
    /// order. Built from the tree with nothing collapsed, so the dashboard
    /// always shows the full expanded view; a workspace in two projects is
    /// listed once (see `cycle_order`).
    pub fn dashboard_indices(&self) -> Vec<usize> {
        let infos: Vec<piki_core::WorkspaceInfo> =
            self.workspaces.iter().map(|w| w.info.clone()).collect();
        let mut seen = std::collections::HashSet::new();
        piki_core::projects::tree::project_tree(
            &self.sidebar_projects,
            &infos,
            &std::collections::HashSet::new(),
        )
        .iter()
        .filter_map(|row| row.workspace_index())
        .filter(|idx| seen.insert(*idx))
        .filter(|&idx| !self.workspaces[idx].tabs.is_empty())
        .collect()
    }

    /// Visual rows for the sidebar, in render order: `Some(row)` indexes into
    /// `sidebar_rows()`, `None` is a blank separator line inserted between two
    /// top-level groups so projects read as separate blocks. Rendering and
    /// mouse hit-testing must both walk this list (not `sidebar_rows()`
    /// line-for-line), or their row math drifts apart.
    pub fn sidebar_visual_rows(&self) -> Vec<Option<usize>> {
        let rows = self.sidebar_rows();
        let mut out = Vec::with_capacity(rows.len() + rows.len() / 4);
        for (i, row) in rows.iter().enumerate() {
            if i > 0 && matches!(row, ProjectTreeRow::Project { .. }) {
                out.push(None);
            }
            out.push(Some(i));
        }
        out
    }

    /// Map a sidebar row to a workspace index. Returns `None` when `row` is
    /// out of range or lands on a row with no workspace behind it (a project
    /// or bucket header, a repo group, a plain-directory member).
    pub fn sidebar_row_to_workspace(&self, row: usize) -> Option<usize> {
        self.sidebar_rows().get(row)?.workspace_index()
    }

    /// What the cursor stands on — see [`SidebarRowKind`].
    pub fn sidebar_row_kind(&self) -> SidebarRowKind {
        match self.sidebar_rows().get(self.selected_sidebar_row) {
            None => SidebarRowKind::None,
            Some(ProjectTreeRow::Project { bucket, .. }) => match bucket {
                Bucket::Project(_) => SidebarRowKind::Project,
                Bucket::PrReview | Bucket::Unassigned => SidebarRowKind::Bucket,
            },
            Some(ProjectTreeRow::Repo { .. }) => SidebarRowKind::Repo,
            Some(ProjectTreeRow::Checkout { .. }) => SidebarRowKind::Checkout,
            Some(ProjectTreeRow::Dir { .. }) => SidebarRowKind::Dir,
        }
    }

    /// The project the cursor acts on: its own row's project, or the project
    /// the row sits under. `None` on a synthetic bucket, which has no stored
    /// project to edit or delete.
    pub fn selected_project(&self) -> Option<&piki_core::projects::Project> {
        match self.sidebar_rows().get(self.selected_sidebar_row)?.bucket() {
            Bucket::Project(i) => self.sidebar_projects.get(i),
            Bucket::PrReview | Bucket::Unassigned => None,
        }
    }

    pub fn select_next_sidebar_row(&mut self) {
        let count = self.sidebar_rows().len();
        if count > 0 {
            self.follow_sidebar_row((self.selected_sidebar_row + 1) % count);
        }
    }

    pub fn select_prev_sidebar_row(&mut self) {
        let count = self.sidebar_rows().len();
        if count > 0 {
            self.follow_sidebar_row((self.selected_sidebar_row + count - 1) % count);
        }
    }

    /// Land the sidebar cursor on `row` and, when that row is a checkout,
    /// switch to its workspace.
    ///
    /// Follow-focus: the cursor and the workspace every action targets are the
    /// same thing. Without it the two drift apart — the cursor sits on one
    /// workspace while `prefix c` opens its tab in whichever workspace the main
    /// panel still shows — and every workspace-scoped action becomes a coin
    /// flip. Header rows (project, bucket, repo) carry no workspace, so they
    /// move the cursor and leave the active workspace where it was.
    fn follow_sidebar_row(&mut self, row: usize) {
        self.selected_sidebar_row = row;
        self.reveal_sidebar_selection();
        if let Some(idx) = self.sidebar_row_to_workspace(row) {
            self.switch_workspace(idx);
        }
    }

    /// Viewport scroll that keeps `selected` visible inside `total` rows of
    /// which `visible` fit, starting from the current `stored` wheel offset.
    fn reveal_scroll(total: usize, visible: usize, selected: usize, stored: usize) -> usize {
        if visible == 0 {
            return 0;
        }
        let max = total.saturating_sub(visible);
        let cur = stored.min(max);
        if selected < cur {
            selected
        } else if selected >= cur + visible {
            (selected + 1 - visible).min(max)
        } else {
            cur
        }
    }

    /// Viewport offset used by the workspace-list render and mouse
    /// hit-testing: the wheel position clamped to content.
    pub fn sidebar_viewport(&self) -> usize {
        let visible = self.ws_list_area.height.saturating_sub(2) as usize;
        let max = self.sidebar_visual_rows().len().saturating_sub(visible);
        self.sidebar_scroll.min(max)
    }

    /// Same for the Agents pane.
    pub fn agents_viewport(&self) -> usize {
        let visible = self.agents_area.height.saturating_sub(2) as usize;
        let max = self.agent_rows().len().saturating_sub(visible);
        self.agents_scroll.min(max)
    }

    /// Pull the workspace-list viewport so the selected row is visible.
    /// Called on selection moves (keyboard, workspace switches); the wheel
    /// deliberately does NOT call this — it scrolls freely.
    pub fn reveal_sidebar_selection(&mut self) {
        let rows = self.sidebar_visual_rows();
        let visible = self.ws_list_area.height.saturating_sub(2) as usize;
        let selected_visual = rows
            .iter()
            .position(|r| *r == Some(self.selected_sidebar_row))
            .unwrap_or(0);
        self.sidebar_scroll =
            Self::reveal_scroll(rows.len(), visible, selected_visual, self.sidebar_scroll);
    }

    /// Same for the Agents pane selection.
    pub fn reveal_agent_selection(&mut self) {
        let total = self.agent_rows().len();
        let visible = self.agents_area.height.saturating_sub(2) as usize;
        let selected = self.selected_agent_row.min(total.saturating_sub(1));
        self.agents_scroll = Self::reveal_scroll(total, visible, selected, self.agents_scroll);
    }

    /// (Re)load the project list the sidebar tree is built from, and clamp the
    /// cursor to the rows that survive. Called at startup and after every
    /// project save/delete — the tree is derived, so nothing else needs
    /// invalidating.
    pub fn reload_sidebar_projects(&mut self) {
        self.sidebar_projects = self
            .storage
            .projects
            .as_ref()
            .map(|s| s.list_projects())
            .unwrap_or_default();
        let rows = self.sidebar_rows().len();
        self.selected_sidebar_row = self.selected_sidebar_row.min(rows.saturating_sub(1));
        self.reveal_sidebar_selection();
    }

    /// If the selected row is collapsible (a project/bucket header or a repo
    /// group), its collapse key and current state.
    fn selected_collapsible(&self) -> Option<(String, bool)> {
        match self.sidebar_rows().get(self.selected_sidebar_row)? {
            ProjectTreeRow::Project { key, collapsed, .. }
            | ProjectTreeRow::Repo { key, collapsed, .. } => Some((key.clone(), *collapsed)),
            _ => None,
        }
    }

    /// Toggle collapse on the selected row if it's a header or a repo group.
    /// No-op otherwise.
    pub fn toggle_selected_group(&mut self) {
        let Some((key, collapsed)) = self.selected_collapsible() else {
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

    /// Collapse the selected row's group. Tree-style `←` behaviour; a no-op
    /// unless the selection is on an expanded header or repo row.
    pub fn collapse_selected_group(&mut self) {
        let Some((key, collapsed)) = self.selected_collapsible() else {
            return;
        };
        if !collapsed {
            self.collapsed_groups.insert(key);
            self.persist_collapsed_groups();
        }
    }

    /// Expand the selected row's group. Tree-style `→` behaviour; a no-op
    /// unless the selection is on a collapsed header or repo row.
    pub fn expand_selected_group(&mut self) {
        let Some((key, collapsed)) = self.selected_collapsible() else {
            return;
        };
        if collapsed {
            self.collapsed_groups.remove(&key);
            self.persist_collapsed_groups();
        }
    }

    /// Update `selected_sidebar_row` to point at the given workspace. A
    /// workspace that belongs to two projects has a row under each; the first
    /// one wins, so the cursor lands somewhere deterministic.
    pub fn sync_sidebar_row(&mut self, ws_idx: usize) {
        if let Some(i) = self
            .sidebar_rows()
            .iter()
            .position(|row| row.workspace_index() == Some(ws_idx))
        {
            self.selected_sidebar_row = i;
        }
        self.reveal_sidebar_selection();
    }

    /// Open the fuzzy file search overlay by scanning all files in the active worktree.
    /// Uses nucleo's async matcher — results appear incrementally as the walker discovers files.
    pub fn open_fuzzy_search(&mut self) {
        let worktree_path = match self.current_workspace() {
            Some(ws) => ws.info.path.clone(),
            None => {
                self.set_toast("No active workspace", ToastLevel::Info);
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
                    injector.push(path, |_path, cols| {
                        cols[0] = col;
                    });
                }
            }
        });
    }

    /// Open the project-wide content search overlay (ripgrep-backed).
    pub fn open_project_search(&mut self) {
        let root = match self.current_workspace() {
            Some(ws) => ws.info.path.clone(),
            None => {
                self.set_toast("No active workspace", ToastLevel::Info);
                return;
            }
        };
        self.project_search = Some(ProjectSearchState {
            query: String::new(),
            selected: 0,
            root,
            generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            shared: Arc::new(parking_lot::Mutex::new(ProjectSearchShared::default())),
        });
        self.mode = AppMode::ProjectSearch;
    }

    /// Open the command palette overlay.
    pub fn open_command_palette(&mut self) {
        let mru = crate::command_palette::load_mru(&self.storage);
        self.command_palette = Some(crate::command_palette::create_state(&self.workspaces, &mru));
        self.mode = AppMode::CommandPalette;
    }

    /// Re-parent a tab into another workspace, keeping its process running.
    ///
    /// The PTY is untouched — only which workspace owns the tab changes (the
    /// shell keeps the cwd it was started in). Tab ids are handed out per
    /// workspace (`Workspace::next_tab_id`), so the tab is re-stamped on
    /// arrival or it could collide with an id already in use there. Returns
    /// the tab's index in the destination, or `None` when the move is not
    /// possible (same workspace, or either index out of range).
    pub fn move_tab(&mut self, from: usize, tab_idx: usize, to: usize) -> Option<usize> {
        if from == to || from >= self.workspaces.len() || to >= self.workspaces.len() {
            return None;
        }
        let mut tab = {
            let src = &mut self.workspaces[from];
            if tab_idx >= src.tabs.len() {
                return None;
            }
            let tab = src.tabs.remove(tab_idx);
            if src.active_tab >= src.tabs.len() && !src.tabs.is_empty() {
                src.active_tab = src.tabs.len() - 1;
            }
            tab
        };
        let dst = &mut self.workspaces[to];
        tab.id = dst.next_tab_id;
        dst.next_tab_id += 1;
        // Landing scrolled back would show the tab mid-scrollback on arrival.
        tab.term_scroll = 0;
        dst.tabs.push(tab);
        let idx = dst.tabs.len() - 1;
        dst.active_tab = idx;
        Some(idx)
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
                self.set_toast(format!("Cannot read file: {}", e), ToastLevel::Error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialog_state::DialogState;
    use crate::test_support::{add_agent_tab, add_terminal_tab, add_test_workspace};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn test_storage() -> std::sync::Arc<piki_core::storage::AppStorage> {
        std::sync::Arc::new(piki_core::storage::AppStorage {
            workspaces: Box::new(piki_core::storage::json::JsonStorage),
            api_history: None,
            ui_prefs: None,
            agent_profiles: None,
            projects: None,
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

    // Workspace/tab fixtures live in `crate::test_support` so every test
    // module can build workspaces, not just this one.

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

    // ── Move a tab between workspaces ──

    #[test]
    fn test_move_tab_reparents_and_restamps_the_id() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app); // 0
        add_test_workspace(&mut app); // 1
        // Ids are handed out per workspace, so the source's first tab and the
        // destination's first tab are both id 0: moving one across without
        // re-stamping it would put two tabs with the same id in one workspace.
        let moved = crate::test_support::add_terminal_tab(&mut app, 0);
        crate::test_support::add_terminal_tab(&mut app, 0);
        crate::test_support::add_terminal_tab(&mut app, 1);
        assert_eq!(
            app.workspaces[0].tabs[moved].id, app.workspaces[1].tabs[0].id,
            "ids collide as set up"
        );

        let new_idx = app.move_tab(0, moved, 1).expect("move should succeed");

        assert_eq!(app.workspaces[0].tabs.len(), 1);
        assert_eq!(app.workspaces[1].tabs.len(), 2);
        assert_eq!(new_idx, 1);
        assert_eq!(app.workspaces[1].active_tab, 1, "the moved tab is in front");
        let ids: Vec<usize> = app.workspaces[1].tabs.iter().map(|t| t.id).collect();
        assert_ne!(ids[0], ids[1], "the moved tab got a fresh id");
    }

    #[test]
    fn test_move_tab_clamps_the_source_active_tab() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        add_test_workspace(&mut app);
        crate::test_support::add_terminal_tab(&mut app, 0);
        let last = crate::test_support::add_terminal_tab(&mut app, 0);
        app.workspaces[0].active_tab = last;

        app.move_tab(0, last, 1).expect("move should succeed");

        assert_eq!(
            app.workspaces[0].active_tab, 0,
            "the source must not point past its last tab"
        );
    }

    #[test]
    fn test_move_tab_rejects_impossible_moves() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        add_test_workspace(&mut app);
        crate::test_support::add_terminal_tab(&mut app, 0);

        assert!(app.move_tab(0, 0, 0).is_none(), "same workspace");
        assert!(app.move_tab(0, 9, 1).is_none(), "tab out of range");
        assert!(app.move_tab(0, 0, 9).is_none(), "workspace out of range");
        assert_eq!(app.workspaces[0].tabs.len(), 1, "nothing was taken");
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

    #[test]
    fn test_agent_rows_spans_all_workspaces() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
            matches!(app.workspaces[wi].tabs[ti].provider, AIProvider::Custom(_))
        }));
    }

    #[test]
    fn test_agent_row_at_maps_click_to_index() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        for n in 0..6 {
            add_agent_tab(&mut app, a, &format!("Agent{n}"));
        }
        // visible height = 2 (height 4 minus borders); select the last row and
        // reveal it, as the keyboard/sync paths do, so the viewport shows
        // rows 4 & 5.
        app.agents_area = ratatui::layout::Rect::new(0, 0, 20, 4);
        app.selected_agent_row = 5;
        app.reveal_agent_selection();
        assert_eq!(
            app.agent_row_at(1),
            Some(4),
            "first visible row after scroll"
        );
        assert_eq!(
            app.agent_row_at(2),
            Some(5),
            "selected row scrolled into view"
        );
        assert_eq!(app.agent_row_at(3), None, "below the pane border");
    }

    #[test]
    fn test_agent_row_at_empty_is_none() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        app.agents_area = ratatui::layout::Rect::new(0, 0, 20, 5);
        assert_eq!(app.agent_row_at(1), None);
    }

    #[test]
    fn test_agents_pane_navigation_and_jump() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        let b = add_test_workspace(&mut app);
        app.switch_workspace(a);

        // Follow-focus: j/k don't just move a cursor, they move the workspace
        // every action targets — so `prefix c` right after lands its tab here.
        // Rows: [bucket, repo-a, a, repo-b, b], so b is two rows past a (its
        // repo header sits between them and switches nothing).
        app.select_next_sidebar_row();
        app.select_next_sidebar_row();
        assert_eq!(app.selected_workspace, b);
        assert_eq!(app.active_workspace, b);

        app.select_prev_sidebar_row();
        app.select_prev_sidebar_row();
        assert_eq!(app.active_workspace, a);
    }

    #[test]
    fn sidebar_cursor_follows_onto_checkouts_and_rests_on_headers() {
        // The tree has header rows with no workspace behind them (the bucket,
        // each repo group). Walking the cursor over those leaves the active
        // workspace alone; landing on a checkout switches to it.
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        let parent = add_test_workspace(&mut app);
        let child = add_test_workspace(&mut app);
        let shared_repo = app.workspaces[parent].info.source_repo.clone();
        app.workspaces[child].info.source_repo = shared_repo.clone();
        app.workspaces[child].info.workspace_type = piki_core::WorkspaceType::Worktree;
        app.switch_workspace(a);

        // No projects configured, so everything sits in the no-project bucket:
        // [bucket, repo(a), a, repo(shared), parent, child].
        let rows = app.sidebar_rows();
        assert_eq!(rows.len(), 6, "{rows:#?}");
        assert!(matches!(rows[0], ProjectTreeRow::Project { .. }));
        assert_eq!(rows[2].workspace_index(), Some(a));
        assert_eq!(rows[4].workspace_index(), Some(parent));
        assert_eq!(rows[5].workspace_index(), Some(child));

        // The cursor sits on a's row after switch_workspace(a).
        assert_eq!(app.selected_sidebar_row, 2);
        // Next row is the shared repo's header: cursor moves, workspace doesn't.
        app.select_next_sidebar_row();
        assert_eq!(app.selected_sidebar_row, 3);
        assert_eq!(app.active_workspace, a);
        // Then the parent checkout, which does switch.
        app.select_next_sidebar_row();
        assert_eq!(app.active_workspace, parent);
        app.select_next_sidebar_row();
        assert_eq!(app.active_workspace, child);
    }

    #[test]
    fn test_selection_survives_on_its_own_tab_but_not_a_switch() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Claude");
        add_agent_tab(&mut app, a, "Codex");
        app.active_workspace = a;
        app.workspaces[a].active_tab = 0;

        let owner = app.selection_owner_key().unwrap();
        app.selection = Some(Selection::new(1, 2, owner));

        // Same tab on screen — the selection stays.
        app.drop_stale_selection();
        assert!(app.selection.is_some());

        // Tab switch — the cell rectangle now addresses the other tab's
        // content, so the selection must be dropped, not re-copied from it.
        app.workspaces[a].active_tab = 1;
        app.drop_stale_selection();
        assert!(app.selection.is_none());
    }

    #[test]
    fn test_selection_owner_is_the_tab_id_not_its_position() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let a = add_test_workspace(&mut app);
        add_agent_tab(&mut app, a, "Claude");
        add_agent_tab(&mut app, a, "Codex");
        app.active_workspace = a;
        app.workspaces[a].active_tab = 1;

        let owner = app.selection_owner_key().unwrap();
        app.selection = Some(Selection::new(0, 0, owner));

        // Closing the first tab shifts the owning tab to position 0; the id
        // still matches, so the selection survives.
        app.workspaces[a].tabs.remove(0);
        app.workspaces[a].active_tab = 0;
        app.drop_stale_selection();
        assert!(app.selection.is_some());
    }

    #[test]
    fn test_agent_selection_follows_the_active_agent_tab() {
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
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
        // The focused project tree is keyboard-navigable: up/down (j/k or the
        // arrows) move the selection and Enter opens the checkout under the
        // cursor. Heavier actions (new/edit/delete workspace) still go through
        // the prefix.
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        add_test_workspace(&mut app);
        add_test_workspace(&mut app);
        app.active_pane = ActivePane::WorkspaceList;
        // Rows: [bucket, repo-0, ws-0, repo-1, ws-1].
        app.selected_sidebar_row = 0;

        // Down (j) moves the selection to row 1.
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected_sidebar_row, 1);
        // Arrow Up moves back to row 0.
        crate::input::handle_key_event(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected_sidebar_row, 0);

        // Walk down to the second workspace's row and open it.
        for _ in 0..4 {
            crate::input::handle_key_event(&mut app, key(KeyCode::Down));
        }
        assert_eq!(app.selected_sidebar_row, 4);
        crate::input::handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(app.active_workspace, 1);
        assert_eq!(app.active_pane, ActivePane::WorkspaceList);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.active_dialog.is_none());
    }

    #[test]
    fn test_workspace_list_collapse_expand_group() {
        // Side arrows (or h/l) collapse/expand the repo group the cursor is on,
        // hiding its checkouts.
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
        // Rows: [bucket, repo, parent, child]. Stand on the repo group.
        app.selected_sidebar_row = 1;
        let repo_key = app.sidebar_rows()[1]
            .collapse_key()
            .expect("repo rows are collapsible")
            .to_string();

        // h collapses the repo, hiding both checkouts.
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert!(app.collapsed_groups.contains(&repo_key));
        assert_eq!(app.sidebar_rows().len(), 2);
        // l re-expands it.
        crate::input::handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert!(!app.collapsed_groups.contains(&repo_key));
        assert_eq!(app.sidebar_rows().len(), 4);
        // Arrow Left collapses again.
        crate::input::handle_key_event(&mut app, key(KeyCode::Left));
        assert!(app.collapsed_groups.contains(&repo_key));
        // Arrow Right expands.
        crate::input::handle_key_event(&mut app, key(KeyCode::Right));
        assert!(!app.collapsed_groups.contains(&repo_key));
    }

    #[test]
    fn ephemeral_workspaces_group_under_the_pr_review_bucket() {
        // Review workspaces each have their own source_repo (their own
        // checkout path), so grouping them by repo would emit one useless
        // header per PR — they land flat under one synthetic bucket instead.
        let mut app = App::new(
            test_storage(),
            &piki_core::paths::DataPaths::default_paths(),
        );
        let plain = add_test_workspace(&mut app); // index 0, non-ephemeral
        let review_a = add_test_workspace(&mut app); // index 1
        let review_b = add_test_workspace(&mut app); // index 2
        app.workspaces[review_a].info.ephemeral = true;
        app.workspaces[review_b].info.ephemeral = true;

        let rows = app.sidebar_rows();
        // The review bucket leads, then its two flat children, then the
        // no-project bucket with the plain workspace under its repo.
        assert_eq!(rows[0].bucket(), crate::app::Bucket::PrReview);
        assert_eq!(
            rows[1..3]
                .iter()
                .map(|r| r.workspace_index())
                .collect::<Vec<_>>(),
            vec![Some(review_a), Some(review_b)]
        );
        assert_eq!(rows[1].depth(), 1, "no repo group above a PR review");
        assert_eq!(rows[3].bucket(), crate::app::Bucket::Unassigned);
        assert_eq!(rows[5].workspace_index(), Some(plain));

        // Collapsing the bucket hides both review rows but keeps the plain
        // workspace visible.
        app.active_pane = ActivePane::WorkspaceList;
        app.selected_sidebar_row = 0;
        app.toggle_selected_group();
        assert!(
            app.collapsed_groups
                .contains(piki_core::projects::tree::PR_REVIEW_KEY)
        );
        let collapsed = app.sidebar_rows();
        assert_eq!(collapsed.len(), 4, "{collapsed:#?}");
        assert!(matches!(
            collapsed[0],
            ProjectTreeRow::Project {
                collapsed: true,
                ..
            }
        ));
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
