use std::borrow::Cow;
use std::sync::Arc;

use crate::app::Workspace;

/// A command that can be executed from the command palette.
pub struct PaletteCommand {
    /// Unique identifier matching a navigation keybinding action name
    pub id: &'static str,
    /// Human-readable label shown in the palette
    pub label: Cow<'static, str>,
    /// Category for grouping (e.g. "Workspace", "Git", "Tabs")
    pub category: &'static str,
    /// Keybinding hint (e.g. "n", "ctrl-z") — looked up at render time from config
    pub keybinding_action: &'static str,
    /// Optional workspace index for dynamic "Switch to" commands
    pub switch_workspace_idx: Option<usize>,
}

/// State for the command palette overlay (backed by nucleo async matcher)
pub struct CommandPaletteState {
    pub query: String,
    pub nucleo: nucleo::Nucleo<PaletteCommand>,
    pub selected: usize,
}

impl CommandPaletteState {
    /// Get the currently selected command
    pub fn selected_command(&self) -> Option<&PaletteCommand> {
        let snap = self.nucleo.snapshot();
        snap.get_matched_item(self.selected as u32)
            .map(|item| item.data)
    }
}

macro_rules! cmd {
    ($id:expr, $label:expr, $cat:expr, $kb:expr) => {
        PaletteCommand {
            id: $id,
            label: Cow::Borrowed($label),
            category: $cat,
            keybinding_action: $kb,
            switch_workspace_idx: None,
        }
    };
}

/// Returns the static list of all palette commands.
fn all_commands() -> Vec<PaletteCommand> {
    vec![
        // Workspace
        cmd!(
            "new_workspace",
            "New Workspace",
            "Workspace",
            "new_workspace"
        ),
        cmd!(
            "clone_workspace",
            "Clone Workspace",
            "Workspace",
            "clone_workspace"
        ),
        cmd!(
            "edit_workspace",
            "Edit Workspace",
            "Workspace",
            "edit_workspace"
        ),
        cmd!(
            "delete_workspace",
            "Delete Workspace",
            "Workspace",
            "delete_workspace"
        ),
        cmd!("dashboard", "Dashboard", "Workspace", "dashboard"),
        cmd!(
            "workspace_info",
            "Workspace Info",
            "Workspace",
            "workspace_info"
        ),
        // Git
        cmd!("commit", "Commit", "Git", "commit"),
        cmd!("merge", "Merge", "Git", "merge"),
        cmd!("push", "Push", "Git", "push"),
        cmd!("stash", "Stash", "Git", "stash"),
        cmd!("git_log", "Git Log", "Git", "git_log"),
        cmd!("conflicts", "Conflict Resolution", "Git", "conflicts"),
        cmd!("undo", "Undo", "Git", "undo"),
        // Tabs
        cmd!("new_tab", "New Tab", "Tabs", "new_tab"),
        cmd!("close_tab", "Close Tab", "Tabs", "close_tab"),
        cmd!("next_tab", "Next Tab", "Tabs", "next_tab"),
        cmd!("prev_tab", "Previous Tab", "Tabs", "prev_tab"),
        // Workspace (extra)
        cmd!("kanban", "Open Kanban Board", "Workspace", "kanban"),
        // Search
        cmd!(
            "fuzzy_search",
            "Fuzzy File Search",
            "Search",
            "fuzzy_search"
        ),
        // View
        cmd!("help", "Help", "View", "help"),
        cmd!("about", "About", "View", "about"),
        cmd!("logs", "Logs", "View", "logs"),
        // Layout
        cmd!(
            "sidebar_shrink",
            "Shrink Sidebar",
            "Layout",
            "sidebar_shrink"
        ),
        cmd!("sidebar_grow", "Grow Sidebar", "Layout", "sidebar_grow"),
        cmd!("split_up", "Grow Left Split", "Layout", "split_up"),
        cmd!("split_down", "Shrink Left Split", "Layout", "split_down"),
        // Clipboard
        cmd!("copy", "Copy Terminal", "Clipboard", "copy"),
        // Agents / Providers
        cmd!(
            "manage_agents",
            "Manage Agents",
            "Agents",
            "manage_agents"
        ),
        cmd!(
            "manage_providers",
            "Manage Providers",
            "Providers",
            "manage_providers"
        ),
        // App
        cmd!("quit", "Quit", "App", "quit"),
    ]
}

/// Create a new CommandPaletteState with all commands and workspace switch entries.
pub fn create_state(workspaces: &[Workspace]) -> CommandPaletteState {
    let nucleo = nucleo::Nucleo::new(nucleo::Config::DEFAULT, Arc::new(|| {}), Some(1), 1);
    let injector = nucleo.injector();

    for cmd in all_commands() {
        let search_text: nucleo::Utf32String = format!("{}: {}", cmd.category, cmd.label).into();
        injector.push(cmd, |cols| {
            cols[0] = search_text;
        });
    }

    // Inject dynamic workspace switch commands
    for (i, ws) in workspaces.iter().enumerate() {
        let label = format!("Switch to {}", ws.name);
        let cmd = PaletteCommand {
            id: "switch_workspace",
            label: Cow::Owned(label),
            category: "Switch",
            keybinding_action: "",
            switch_workspace_idx: Some(i),
        };
        let search_text: nucleo::Utf32String = format!("Switch: {}", ws.name).into();
        injector.push(cmd, |cols| {
            cols[0] = search_text;
        });
    }

    CommandPaletteState {
        query: String::new(),
        nucleo,
        selected: 0,
    }
}
