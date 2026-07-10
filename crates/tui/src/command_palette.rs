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

/// Returns the static list of all palette commands, derived from the shared
/// action catalog so it never drifts from the other discoverability surfaces.
fn all_commands() -> Vec<PaletteCommand> {
    crate::action_catalog::catalog()
        .iter()
        .map(|a| PaletteCommand {
            id: a.id,
            label: Cow::Borrowed(a.label),
            category: a.category,
            keybinding_action: a.id,
            switch_workspace_idx: None,
        })
        .collect()
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
