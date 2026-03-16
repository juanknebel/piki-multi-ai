use crate::app::DialogField;
use piki_core::WorkspaceType;

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
    NewTab,
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
}
