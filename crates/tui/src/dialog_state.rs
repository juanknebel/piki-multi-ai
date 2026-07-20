use crate::app::{DialogField, NewWorkspaceSource};

/// Field-cycling behavior for tab-navigation in multi-field dialogs.
/// Implemented by per-dialog field enums (`EditWorkspaceField`,
/// `EditProviderField`, `EditAgentField`, `DialogField`) so handlers can
/// call `*active_field = active_field.next()` instead of hard-coding match
/// arms.
pub trait CycleField: Copy {
    fn next(self) -> Self;
    fn prev(self) -> Self;
}

impl CycleField for DialogField {
    fn next(self) -> Self {
        match self {
            Self::Source => Self::Directory,
            Self::Directory => Self::Destination,
            Self::Destination => Self::Description,
            Self::Description => Self::Prompt,
            Self::Prompt => Self::KanbanPath,
            Self::KanbanPath => Self::Source,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Source => Self::KanbanPath,
            Self::Directory => Self::Source,
            Self::Destination => Self::Directory,
            Self::Description => Self::Destination,
            Self::Prompt => Self::Description,
            Self::KanbanPath => Self::Prompt,
        }
    }
}

impl DialogField {
    /// Source-aware cycling: when `source = Local`, the `Destination`
    /// field is irrelevant (it's only used for GitHub clones) and Tab
    /// should jump over it.
    pub fn next_with(self, source: NewWorkspaceSource) -> Self {
        let n = self.next();
        if n == Self::Destination && source == NewWorkspaceSource::Local {
            n.next()
        } else {
            n
        }
    }

    pub fn prev_with(self, source: NewWorkspaceSource) -> Self {
        let p = self.prev();
        if p == Self::Destination && source == NewWorkspaceSource::Local {
            p.prev()
        } else {
            p
        }
    }
}

/// Which level of the new-tab menu is currently shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewTabMenu {
    Main,
    /// AI Agents submenu — selected index into the combined list of
    /// built-in dispatchable providers + custom providers from providers.toml.
    Agents { selected: usize },
    Tools,
}

/// Sub-state of the PR picker (`DialogState::PrPicker`) for browsing every
/// open PR of one specific repo instead of the default categorized list.
#[derive(Debug, Clone)]
pub enum RepoBrowse {
    /// Showing the default categorized "my PRs" list.
    Closed,
    /// Typing the `owner/repo` to browse (`o` to enter, `Enter` to load).
    Input { text: String, cursor: usize },
    /// Loaded: every open PR in `repo_nwo`, flat (no category grouping).
    Loaded {
        repo_nwo: String,
        items: Vec<piki_core::github::PrListItem>,
    },
}

/// Centralized dialog state — replaces 20+ scattered fields on App.
/// Dismissing any dialog is simply `app.active_dialog = None`.
#[derive(Debug, Clone)]
pub enum DialogState {
    NewWorkspace {
        /// Holds either the folder path (source=Local) or the GitHub URL
        /// (source=GitHub). The label rendered above this field switches
        /// between "Folder:" and "URL:" based on `source`.
        dir: String,
        dir_cursor: usize,
        /// Parent directory where the GitHub clone will land. Only used
        /// when source=GitHub; the dialog seeds it with `paths.repos_dir()`
        /// as a hint, but the user can change it before submit.
        destination: String,
        destination_cursor: usize,
        desc: String,
        desc_cursor: usize,
        prompt: String,
        prompt_cursor: usize,
        kanban: String,
        kanban_cursor: usize,
        source: NewWorkspaceSource,
        active_field: DialogField,
    },
    EditWorkspace {
        target: usize,
        kanban: String,
        kanban_cursor: usize,
        prompt: String,
        prompt_cursor: usize,
        active_field: EditWorkspaceField,
    },
    /// Create a git worktree from a GitHub-origin parent workspace, or load
    /// one that already exists on disk (via `git worktree list`). The parent
    /// is identified by index.
    /// `mode` selects between the two flows; `name`/`prompt`/`kanban`/
    /// `active_field` are only meaningful when `mode == CreateNew` (they
    /// capture the worktree branch name plus optional overrides, pre-filled
    /// from the parent). `existing`/`existing_selected`/`existing_loading`
    /// are only meaningful when `mode == LoadExisting`.
    CreateWorktree {
        parent_idx: usize,
        mode: CreateWorktreeMode,
        name: String,
        name_cursor: usize,
        prompt: String,
        prompt_cursor: usize,
        kanban: String,
        kanban_cursor: usize,
        active_field: CreateWorktreeField,
        existing: Vec<piki_core::workspace::ExistingWorktree>,
        existing_selected: usize,
        existing_loading: bool,
    },
    ConfirmDelete {
        target: usize,
    },
    ConfirmCloseTab {
        target: usize,
    },
    ConfirmQuit,
    NewTab {
        menu: NewTabMenu,
    },
    Help {
        scroll: u16,
        /// Incremental filter query; when non-empty the help shows only the
        /// keybinding lines that match it (case-insensitive substring).
        filter: String,
    },
    About,
    /// A bridged agent tab was opened while the tools its hooks need are
    /// missing from PATH. The tab spawned fine — this only warns that its
    /// status will be guessed from PTY silence instead of read from the agent.
    MissingPrereqs {
        /// The agent whose bridge is degraded (e.g. `Antigravity`).
        agent: String,
        /// Tools that weren't found (e.g. `jq`).
        missing: Vec<String>,
    },
    WorkspaceInfo {
        hscroll: u16,
    },
    /// PR picker — the Code Review entry point. Lists PRs relevant to the
    /// current `gh` user across all accessible repos, grouped by why they
    /// showed up (`PrInclusionReason`), independent of any open workspace.
    /// `repo_browse` swaps the list for "every open PR in one repo I typed
    /// in" (`o`) instead of the default categorized "my PRs" view.
    PrPicker {
        loading: bool,
        error: Option<String>,
        items: Vec<piki_core::github::PrListItem>,
        selected: usize,
        /// Index into the *currently visible* list (see
        /// `App::pr_picker_visible_items`) that's being checked out.
        checking_out: Option<usize>,
        repo_browse: RepoBrowse,
    },
    Dashboard {
        selected: usize,
        scroll_offset: usize,
    },
    Logs {
        scroll: u16,
        level_filter: u8,
        selected: usize,
        hscroll: u16,
        search_active: bool,
        search_buffer: String,
        search_cursor: usize,
        auto_refresh: bool,
    },
    DispatchAgent {
        source_ws: usize,
        card_id: String,
        card_title: String,
        card_description: String,
        card_priority: flow_core::Priority,
        card_project: String,
        /// When agents exist: index into agents vec. When empty: index into dispatchable provider list
        agent_idx: usize,
        /// Snapshot of configured agents (name, provider, role). Empty = fallback to raw providers
        agents: Vec<(String, String, String)>,
        additional_prompt: String,
        additional_prompt_cursor: usize,
        /// 0 = agent/provider select, 1 = workspace destination select
        step: u8,
        /// When step==1: false = new worktree, true = use current workspace
        use_current_ws: bool,
    },
    ManageAgents {
        selected: usize,
    },
    /// Step 1: name + provider selection
    EditAgent {
        /// None = creating new, Some(id) = editing existing
        editing_id: Option<i64>,
        name: String,
        name_cursor: usize,
        provider_idx: usize,
        /// Preserved role content (carried between steps)
        role: String,
        active_field: EditAgentField,
    },
    /// Step 2: large floating editor for the agent role
    EditAgentRole {
        editing_id: Option<i64>,
        name: String,
        provider_idx: usize,
        role: String,
        role_cursor: usize,
        /// Vertical scroll offset
        scroll: usize,
    },
    /// Import agent files from repo into app storage
    ImportAgents {
        /// Discovered: (name, provider_label, role_content, already_exists_in_db)
        discovered: Vec<(String, String, String, bool)>,
        /// Checkbox state per discovered agent
        selected: Vec<bool>,
        cursor: usize,
    },
    /// Choose which kanban column to move a dispatched card to when deleting a workspace
    DispatchCardMove {
        /// Workspace index being deleted
        target: usize,
        /// Available columns: (id, title)
        columns: Vec<(String, String)>,
        /// Currently selected column index
        selected: usize,
    },
    /// Manage custom providers overlay
    ManageProviders {
        selected: usize,
    },
    /// Edit/create a custom provider
    EditProvider {
        /// Original name when editing (for rename support), None when creating
        original_name: Option<String>,
        name: String,
        name_cursor: usize,
        description: String,
        desc_cursor: usize,
        command: String,
        command_cursor: usize,
        default_args: String,
        args_cursor: usize,
        prompt_format_idx: usize,
        prompt_flag: String,
        flag_cursor: usize,
        dispatchable: bool,
        agent_dir: String,
        agent_dir_cursor: usize,
        active_field: EditProviderField,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditProviderField {
    Name,
    Description,
    Command,
    DefaultArgs,
    PromptFormat,
    PromptFlag,
    Dispatchable,
    AgentDir,
}

impl CycleField for EditProviderField {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Description,
            Self::Description => Self::Command,
            Self::Command => Self::DefaultArgs,
            Self::DefaultArgs => Self::PromptFormat,
            Self::PromptFormat => Self::PromptFlag,
            Self::PromptFlag => Self::Dispatchable,
            Self::Dispatchable => Self::AgentDir,
            Self::AgentDir => Self::Name,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Name => Self::AgentDir,
            Self::Description => Self::Name,
            Self::Command => Self::Description,
            Self::DefaultArgs => Self::Command,
            Self::PromptFormat => Self::DefaultArgs,
            Self::PromptFlag => Self::PromptFormat,
            Self::Dispatchable => Self::PromptFlag,
            Self::AgentDir => Self::Dispatchable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAgentField {
    Name,
    Provider,
}

impl CycleField for EditAgentField {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Provider,
            Self::Provider => Self::Name,
        }
    }

    fn prev(self) -> Self {
        // Two-variant cycle: prev == next.
        self.next()
    }
}

/// Active field in the EditWorkspace dialog. Restricted to the three editable
/// fields (the workspace's name/dir/type are immutable at edit time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditWorkspaceField {
    KanbanPath,
    Prompt,
}

impl CycleField for EditWorkspaceField {
    fn next(self) -> Self {
        match self {
            Self::KanbanPath => Self::Prompt,
            Self::Prompt => Self::KanbanPath,
        }
    }

    fn prev(self) -> Self {
        // Two-variant cycle: prev == next.
        self.next()
    }
}

/// Which step of the CreateWorktree dialog is currently shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateWorktreeMode {
    /// Pick "Create new worktree" vs "Load existing worktree".
    ChooseSource,
    /// Today's text-field flow: branch name + optional prompt/kanban.
    CreateNew,
    /// Pick from worktrees discovered on disk via `git worktree list`.
    LoadExisting,
}

/// Active field in the CreateWorktree dialog (Layer 3 GitHub-only flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateWorktreeField {
    Name,
    Prompt,
    KanbanPath,
}

impl CycleField for CreateWorktreeField {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Prompt,
            Self::Prompt => Self::KanbanPath,
            Self::KanbanPath => Self::Name,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Name => Self::KanbanPath,
            Self::Prompt => Self::Name,
            Self::KanbanPath => Self::Prompt,
        }
    }
}
