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
///
/// `command_palette` itself is skipped — offering "open the command palette"
/// from inside the open command palette is noise.
fn all_commands() -> Vec<PaletteCommand> {
    crate::action_catalog::global_actions()
        .filter(|a| a.id != "command_palette")
        .map(|a| PaletteCommand {
            id: a.id,
            label: Cow::Borrowed(a.label),
            category: a.category,
            keybinding_action: a.id,
            switch_workspace_idx: None,
        })
        .collect()
}

/// Cap on the persisted recently-used command list.
const MRU_CAP: usize = 20;

/// Recently-used command ids, most recent first (persisted via ui_prefs).
pub fn load_mru(storage: &piki_core::storage::AppStorage) -> Vec<String> {
    storage
        .ui_prefs
        .as_ref()
        .and_then(|p| p.get_preference("palette_mru").ok().flatten())
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default()
}

/// Record a command execution: move `id` to the front of the persisted list.
pub fn bump_mru(storage: &piki_core::storage::AppStorage, id: &str) {
    let Some(prefs) = storage.ui_prefs.as_ref() else {
        return;
    };
    let mut mru = load_mru(storage);
    mru.retain(|k| k != id);
    mru.insert(0, id.to_string());
    mru.truncate(MRU_CAP);
    if let Ok(json) = serde_json::to_string(&mru) {
        let _ = prefs.set_preference("palette_mru", &json);
    }
}

/// Create a new CommandPaletteState with all commands and workspace switch
/// entries. `mru` biases the empty-query order: recently used commands are
/// injected (and therefore listed) first.
pub fn create_state(workspaces: &[Workspace], mru: &[String]) -> CommandPaletteState {
    let nucleo = nucleo::Nucleo::new(nucleo::Config::DEFAULT, Arc::new(|| {}), Some(1), 1);
    let injector = nucleo.injector();

    let rank = |id: &str| -> usize { mru.iter().position(|k| k == id).unwrap_or(usize::MAX) };
    let mut commands = all_commands();
    commands.sort_by_key(|c| rank(c.id));

    for cmd in commands {
        let search_text: nucleo::Utf32String = format!("{}: {}", cmd.category, cmd.label).into();
        injector.push(cmd, |_cmd, cols| {
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
        injector.push(cmd, |_cmd, cols| {
            cols[0] = search_text;
        });
    }

    CommandPaletteState {
        query: String::new(),
        nucleo,
        selected: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::all_commands;

    /// Every global catalog action is offered by the palette (it derives from
    /// the shared catalog), except the palette's own opener. Anchors that the
    /// Sessions overlay is reachable from the palette in the TUI, mirroring the
    /// desktop's "Sessions (persistent)" entry.
    #[test]
    fn palette_lists_sessions_and_hides_its_own_opener() {
        let cmds = all_commands();
        assert!(
            cmds.iter().any(|c| c.id == "sessions"),
            "the palette must offer the Sessions overlay"
        );
        assert!(
            cmds.iter().all(|c| c.id != "command_palette"),
            "the palette must not offer opening itself"
        );
    }
}
