use std::sync::Arc;

use crate::app::Workspace;

/// State for the fuzzy workspace switcher overlay (backed by nucleo async matcher).
pub struct WorkspaceSwitcherState {
    pub query: String,
    pub nucleo: nucleo::Nucleo<WorkspaceSwitcherEntry>,
    pub selected: usize,
}

/// An entry in the workspace switcher results.
pub struct WorkspaceSwitcherEntry {
    /// Index into `App::workspaces`
    pub index: usize,
    /// Display name
    pub name: String,
    /// Optional group
    pub group: Option<String>,
    /// Git branch name
    pub branch: String,
}

impl WorkspaceSwitcherState {
    /// Get the workspace index of the currently selected item.
    pub fn selected_index(&self) -> Option<usize> {
        let snap = self.nucleo.snapshot();
        snap.get_matched_item(self.selected as u32)
            .map(|item| item.data.index)
    }
}

/// Create a new workspace switcher state populated with all workspaces.
pub fn create_state(workspaces: &[Workspace]) -> WorkspaceSwitcherState {
    let nucleo = nucleo::Nucleo::new(nucleo::Config::DEFAULT, Arc::new(|| {}), Some(1), 1);
    let injector = nucleo.injector();

    for (i, ws) in workspaces.iter().enumerate() {
        let group_str = ws
            .info
            .group
            .as_deref()
            .map(|g| format!(" ({})", g))
            .unwrap_or_default();
        let branch = ws.info.branch.clone();
        let search_text: nucleo::Utf32String =
            format!("{}{} [{}]", ws.name, group_str, branch).into();

        let entry = WorkspaceSwitcherEntry {
            index: i,
            name: ws.name.clone(),
            group: ws.info.group.clone(),
            branch,
        };
        injector.push(entry, |cols| {
            cols[0] = search_text;
        });
    }

    WorkspaceSwitcherState {
        query: String::new(),
        nucleo,
        selected: 0,
    }
}
