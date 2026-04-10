use crate::app::DialogField;
use piki_core::WorkspaceType;

/// Strategy for resolving a merge conflict on a single file.
#[derive(Debug, Clone)]
pub enum ConflictStrategy {
    Ours,
    Theirs,
    MarkResolved,
}

/// A file with an unresolved merge conflict.
#[derive(Debug, Clone)]
pub struct ConflictFile {
    pub path: String,
    /// Human-readable status description (e.g. "Conflicted").
    #[allow(dead_code)]
    pub status: String,
}

/// A single line from `git log --oneline --graph`.
#[derive(Debug, Clone)]
pub struct GitLogEntry {
    pub raw_line: String,
    pub sha: Option<String>,
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

/// Centralized dialog state — replaces 20+ scattered fields on App.
/// Dismissing any dialog is simply `app.active_dialog = None`.
#[derive(Debug, Clone)]
pub enum DialogState {
    NewWorkspace {
        name: String,
        name_cursor: usize,
        dir: String,
        dir_cursor: usize,
        desc: String,
        desc_cursor: usize,
        prompt: String,
        prompt_cursor: usize,
        kanban: String,
        kanban_cursor: usize,
        group: String,
        group_cursor: usize,
        ws_type: WorkspaceType,
        active_field: DialogField,
    },
    EditWorkspace {
        target: usize,
        kanban: String,
        kanban_cursor: usize,
        prompt: String,
        prompt_cursor: usize,
        group: String,
        group_cursor: usize,
        active_field: DialogField,
    },
    CommitMessage {
        buffer: String,
    },
    ConfirmDelete {
        target: usize,
    },
    ConfirmCloseTab {
        target: usize,
    },
    ConfirmQuit,
    ConfirmMerge,
    NewTab {
        menu: NewTabMenu,
    },
    Help {
        scroll: u16,
    },
    About,
    WorkspaceInfo {
        hscroll: u16,
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
    },
    GitLog {
        lines: Vec<GitLogEntry>,
        selected: usize,
        scroll: usize,
    },
    GitStash {
        entries: Vec<(String, String)>,
        selected: usize,
        scroll: usize,
        input_mode: bool,
        input_buffer: String,
        input_cursor: usize,
    },
    ConflictResolution {
        files: Vec<ConflictFile>,
        selected: usize,
        /// The repo path where conflicts exist (source_repo for merge, ws.path for manual)
        repo_path: std::path::PathBuf,
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

impl EditProviderField {
    pub fn next(self) -> Self {
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

    pub fn prev(self) -> Self {
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
