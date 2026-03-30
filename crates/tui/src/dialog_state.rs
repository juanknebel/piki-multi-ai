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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewTabMenu {
    Main,
    Agents,
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
        /// Index into AIProvider::dispatchable()
        provider_idx: usize,
        additional_prompt: String,
        additional_prompt_cursor: usize,
    },
}
