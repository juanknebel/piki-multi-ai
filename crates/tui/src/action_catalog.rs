//! Single source of truth for user-facing app actions.
//!
//! Every discoverability surface — the command palette, the which-key overlay,
//! the `prefix-?` help browser and the prefix footer hints — derives its list
//! from [`catalog`], so adding or renaming an action touches one place instead
//! of the five parallel hand-maintained lists it used to.
//!
//! The catalog holds only *metadata*: the keybinding itself stays in
//! `config::default_app()` and is resolved at render time via
//! `Config::get_binding("app", id)` / `Config::prefix_chord_display(id)`. The
//! `id` is the join key across both, and the parity test below fails the build
//! if a catalog entry has no binding.

/// Static metadata describing one user-facing app action.
pub struct ActionMeta {
    /// Stable id — matches the `[keybindings.app]` action name.
    pub id: &'static str,
    /// Group used for ordering and section headers in the palette / which-key.
    pub category: &'static str,
    /// Short human label (e.g. "New Workspace").
    pub label: &'static str,
}

/// All user-facing app actions, in display order. Order is preserved from the
/// former hand-written palette list so palette snapshots stay stable.
pub fn catalog() -> &'static [ActionMeta] {
    &[
        // ── Workspace ──
        ActionMeta { id: "new_workspace", category: "Workspace", label: "New Workspace" },
        ActionMeta { id: "clone_workspace", category: "Workspace", label: "Create Worktree (GitHub-only)" },
        ActionMeta { id: "edit_workspace", category: "Workspace", label: "Edit Workspace" },
        ActionMeta { id: "delete_workspace", category: "Workspace", label: "Delete Workspace" },
        ActionMeta { id: "dashboard", category: "Workspace", label: "Dashboard" },
        ActionMeta { id: "workspace_info", category: "Workspace", label: "Workspace Info" },
        ActionMeta { id: "workspace_switcher", category: "Workspace", label: "Switch Workspace (fuzzy)" },
        ActionMeta { id: "next_workspace", category: "Workspace", label: "Next Workspace" },
        ActionMeta { id: "prev_workspace", category: "Workspace", label: "Previous Workspace" },
        ActionMeta { id: "toggle_prev_workspace", category: "Workspace", label: "Toggle Last Workspace" },
        // ── Git ──
        ActionMeta { id: "git", category: "Git", label: "Git (lazygit)" },
        // ── Tabs ──
        ActionMeta { id: "new_tab", category: "Tabs", label: "New Tab" },
        ActionMeta { id: "close_tab", category: "Tabs", label: "Close Tab" },
        ActionMeta { id: "next_tab", category: "Tabs", label: "Next Tab" },
        ActionMeta { id: "prev_tab", category: "Tabs", label: "Previous Tab" },
        // ── Search ──
        ActionMeta { id: "fuzzy_search", category: "Search", label: "Fuzzy File Search" },
        ActionMeta { id: "terminal_search", category: "Search", label: "Search in Terminal" },
        // ── View ──
        ActionMeta { id: "help", category: "View", label: "Help" },
        ActionMeta { id: "about", category: "View", label: "About" },
        ActionMeta { id: "logs", category: "View", label: "Logs" },
        ActionMeta { id: "scroll_mode", category: "View", label: "Terminal Scroll Mode" },
        // ── Focus ──
        ActionMeta { id: "focus_left", category: "Focus", label: "Focus Pane Left" },
        ActionMeta { id: "focus_down", category: "Focus", label: "Focus Pane Down" },
        ActionMeta { id: "focus_up", category: "Focus", label: "Focus Pane Up" },
        ActionMeta { id: "focus_right", category: "Focus", label: "Focus Pane Right" },
        // ── Layout ──
        ActionMeta { id: "sidebar_shrink", category: "Layout", label: "Shrink Sidebar" },
        ActionMeta { id: "sidebar_grow", category: "Layout", label: "Grow Sidebar" },
        ActionMeta { id: "split_up", category: "Layout", label: "Grow Left Split" },
        ActionMeta { id: "split_down", category: "Layout", label: "Shrink Left Split" },
        // ── Clipboard ──
        ActionMeta { id: "copy", category: "Clipboard", label: "Copy Terminal" },
        // ── Agents / Providers ──
        ActionMeta { id: "manage_agents", category: "Agents", label: "Manage Agents" },
        ActionMeta { id: "manage_providers", category: "Providers", label: "Manage Providers" },
        // ── App ──
        ActionMeta { id: "chat_panel", category: "View", label: "AI Chat" },
        ActionMeta { id: "quit", category: "App", label: "Quit" },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every catalog action must resolve to a real binding — this is the parity
    /// guard that nothing enforced before. A typo'd id or a catalog entry with
    /// no `[keybindings.app]` default fails here instead of silently showing
    /// `???` in the palette.
    #[test]
    fn every_catalog_action_has_a_binding() {
        let cfg = crate::config::Config::default();
        for a in catalog() {
            let binding = cfg.get_binding("app", a.id);
            assert_ne!(
                binding, "???",
                "catalog action '{}' ({}) has no binding in default_app()",
                a.id, a.label
            );
        }
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = HashSet::new();
        for a in catalog() {
            assert!(seen.insert(a.id), "duplicate catalog id '{}'", a.id);
        }
    }
}
