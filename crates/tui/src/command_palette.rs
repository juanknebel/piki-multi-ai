use std::sync::Arc;

/// A command that can be executed from the command palette.
pub struct PaletteCommand {
    /// Unique identifier matching a navigation keybinding action name
    pub id: &'static str,
    /// Human-readable label shown in the palette
    pub label: &'static str,
    /// Category for grouping (e.g. "Workspace", "Git", "Tabs")
    pub category: &'static str,
    /// Keybinding hint (e.g. "n", "ctrl-z") — looked up at render time from config
    pub keybinding_action: &'static str,
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

/// Returns the static list of all palette commands.
pub fn all_commands() -> Vec<PaletteCommand> {
    vec![
        // Workspace
        PaletteCommand {
            id: "new_workspace",
            label: "New Workspace",
            category: "Workspace",
            keybinding_action: "new_workspace",
        },
        PaletteCommand {
            id: "clone_workspace",
            label: "Clone Workspace",
            category: "Workspace",
            keybinding_action: "clone_workspace",
        },
        PaletteCommand {
            id: "edit_workspace",
            label: "Edit Workspace",
            category: "Workspace",
            keybinding_action: "edit_workspace",
        },
        PaletteCommand {
            id: "delete_workspace",
            label: "Delete Workspace",
            category: "Workspace",
            keybinding_action: "delete_workspace",
        },
        PaletteCommand {
            id: "dashboard",
            label: "Dashboard",
            category: "Workspace",
            keybinding_action: "dashboard",
        },
        PaletteCommand {
            id: "workspace_info",
            label: "Workspace Info",
            category: "Workspace",
            keybinding_action: "workspace_info",
        },
        // Git
        PaletteCommand {
            id: "commit",
            label: "Commit",
            category: "Git",
            keybinding_action: "commit",
        },
        PaletteCommand {
            id: "merge",
            label: "Merge",
            category: "Git",
            keybinding_action: "merge",
        },
        PaletteCommand {
            id: "push",
            label: "Push",
            category: "Git",
            keybinding_action: "push",
        },
        PaletteCommand {
            id: "undo",
            label: "Undo",
            category: "Git",
            keybinding_action: "undo",
        },
        // Tabs
        PaletteCommand {
            id: "new_tab",
            label: "New Tab",
            category: "Tabs",
            keybinding_action: "new_tab",
        },
        PaletteCommand {
            id: "close_tab",
            label: "Close Tab",
            category: "Tabs",
            keybinding_action: "close_tab",
        },
        PaletteCommand {
            id: "next_tab",
            label: "Next Tab",
            category: "Tabs",
            keybinding_action: "next_tab",
        },
        PaletteCommand {
            id: "prev_tab",
            label: "Previous Tab",
            category: "Tabs",
            keybinding_action: "prev_tab",
        },
        // Search
        PaletteCommand {
            id: "fuzzy_search",
            label: "Fuzzy File Search",
            category: "Search",
            keybinding_action: "fuzzy_search",
        },
        // View
        PaletteCommand {
            id: "help",
            label: "Help",
            category: "View",
            keybinding_action: "help",
        },
        PaletteCommand {
            id: "about",
            label: "About",
            category: "View",
            keybinding_action: "about",
        },
        PaletteCommand {
            id: "logs",
            label: "Logs",
            category: "View",
            keybinding_action: "logs",
        },
        // Layout
        PaletteCommand {
            id: "sidebar_shrink",
            label: "Shrink Sidebar",
            category: "Layout",
            keybinding_action: "sidebar_shrink",
        },
        PaletteCommand {
            id: "sidebar_grow",
            label: "Grow Sidebar",
            category: "Layout",
            keybinding_action: "sidebar_grow",
        },
        PaletteCommand {
            id: "split_up",
            label: "Grow Left Split",
            category: "Layout",
            keybinding_action: "split_up",
        },
        PaletteCommand {
            id: "split_down",
            label: "Shrink Left Split",
            category: "Layout",
            keybinding_action: "split_down",
        },
        // Clipboard
        PaletteCommand {
            id: "copy",
            label: "Copy Terminal",
            category: "Clipboard",
            keybinding_action: "copy",
        },
        // App
        PaletteCommand {
            id: "quit",
            label: "Quit",
            category: "App",
            keybinding_action: "quit",
        },
    ]
}

/// Create a new CommandPaletteState with all commands injected into nucleo.
pub fn create_state() -> CommandPaletteState {
    let nucleo = nucleo::Nucleo::new(nucleo::Config::DEFAULT, Arc::new(|| {}), Some(1), 1);
    let injector = nucleo.injector();

    for cmd in all_commands() {
        let search_text: nucleo::Utf32String =
            format!("{}: {}", cmd.category, cmd.label).into();
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
