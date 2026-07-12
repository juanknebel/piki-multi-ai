//! Single source of truth for user-facing app actions.
//!
//! Every discoverability surface — the command palette, the which-key overlay,
//! the `prefix-?` help browser and the prefix footer hints — derives its list
//! from [`catalog`], so adding or renaming an action touches one place instead
//! of the five parallel hand-maintained lists it used to.
//!
//! The catalog holds only *metadata*: the keybinding itself stays in
//! `config::default_app()` and is resolved at render time via
//! `Config::get_binding("app", id)` / `Config::prefix_chord(id)`. The `id` is
//! the join key across both, and the parity test below fails the build if a
//! catalog entry has no binding.

/// Static metadata describing one user-facing app action.
pub struct ActionMeta {
    /// Stable id — matches the `[keybindings.app]` action name.
    pub id: &'static str,
    /// Group used for ordering and section headers in the palette / which-key.
    pub category: &'static str,
    /// Full human label (e.g. "New Workspace"), shown in the palette.
    pub label: &'static str,
    /// Terse label for the space-constrained which-key overlay (e.g. "new").
    pub short: &'static str,
}

/// All user-facing app actions, in display order. Order is preserved from the
/// former hand-written palette list so palette snapshots stay stable.
pub fn catalog() -> &'static [ActionMeta] {
    &[
        // ── Workspace ──
        ActionMeta { id: "new_workspace", category: "Workspace", label: "New Workspace", short: "new" },
        ActionMeta { id: "clone_workspace", category: "Workspace", label: "Create Worktree (GitHub-only)", short: "worktree" },
        ActionMeta { id: "edit_workspace", category: "Workspace", label: "Edit Workspace", short: "edit" },
        ActionMeta { id: "delete_workspace", category: "Workspace", label: "Delete Workspace", short: "delete" },
        ActionMeta { id: "dashboard", category: "Workspace", label: "Dashboard", short: "dashboard" },
        ActionMeta { id: "workspace_info", category: "Workspace", label: "Workspace Info", short: "info" },
        ActionMeta { id: "workspace_switcher", category: "Workspace", label: "Switch Workspace (fuzzy)", short: "switch" },
        ActionMeta { id: "next_workspace", category: "Workspace", label: "Next Workspace", short: "next" },
        ActionMeta { id: "prev_workspace", category: "Workspace", label: "Previous Workspace", short: "prev" },
        ActionMeta { id: "toggle_prev_workspace", category: "Workspace", label: "Toggle Last Workspace", short: "last" },
        // ── Git ──
        ActionMeta { id: "git", category: "Git", label: "Git (lazygit)", short: "lazygit" },
        // ── Tabs ──
        ActionMeta { id: "new_tab", category: "Tabs", label: "New Tab", short: "new" },
        ActionMeta { id: "close_tab", category: "Tabs", label: "Close Tab", short: "close" },
        ActionMeta { id: "next_tab", category: "Tabs", label: "Next Tab", short: "next" },
        ActionMeta { id: "prev_tab", category: "Tabs", label: "Previous Tab", short: "prev" },
        // ── Search ──
        ActionMeta { id: "fuzzy_search", category: "Search", label: "Fuzzy File Search", short: "find file" },
        ActionMeta { id: "terminal_search", category: "Search", label: "Search in Terminal", short: "search" },
        // ── View ──
        ActionMeta { id: "command_palette", category: "View", label: "Command Palette", short: "palette" },
        ActionMeta { id: "help", category: "View", label: "Help", short: "all keys" },
        ActionMeta { id: "about", category: "View", label: "About", short: "about" },
        ActionMeta { id: "logs", category: "View", label: "Logs", short: "logs" },
        ActionMeta { id: "scroll_mode", category: "View", label: "Terminal Scroll Mode", short: "scroll" },
        // ── Focus ──
        ActionMeta { id: "focus_left", category: "Focus", label: "Focus Pane Left", short: "left" },
        ActionMeta { id: "focus_down", category: "Focus", label: "Focus Pane Down", short: "down" },
        ActionMeta { id: "focus_up", category: "Focus", label: "Focus Pane Up", short: "up" },
        ActionMeta { id: "focus_right", category: "Focus", label: "Focus Pane Right", short: "right" },
        // ── Layout ──
        ActionMeta { id: "sidebar_shrink", category: "Layout", label: "Shrink Sidebar", short: "sidebar −" },
        ActionMeta { id: "sidebar_grow", category: "Layout", label: "Grow Sidebar", short: "sidebar +" },
        ActionMeta { id: "split_up", category: "Layout", label: "Grow Left Split", short: "split +" },
        ActionMeta { id: "split_down", category: "Layout", label: "Shrink Left Split", short: "split −" },
        // ── Clipboard ──
        ActionMeta { id: "copy", category: "Clipboard", label: "Copy Terminal", short: "copy" },
        // ── Agents / Providers ──
        ActionMeta { id: "manage_agents", category: "Agents", label: "Manage Agents", short: "agents" },
        ActionMeta { id: "manage_providers", category: "Providers", label: "Manage Providers", short: "providers" },
        // ── App ──
        ActionMeta { id: "chat_panel", category: "View", label: "AI Chat", short: "chat" },
        ActionMeta { id: "quit", category: "App", label: "Quit", short: "quit" },
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

/// Keeps the README's prefix table honest.
///
/// Nothing used to enforce this, and it drifted: the README advertised `N`, `D`,
/// `A`, `V` and `)`/`(` long after they were renamed to `s`, `b`, `m`, `v` and
/// `}`/`{`, so a new user following it could not drive the app.
///
/// It checks the *keys*, not the prose — the descriptions stay hand-written.
#[cfg(test)]
mod readme_parity {
    use super::*;
    use std::collections::HashSet;

    const BEGIN: &str = "<!-- BEGIN:prefix-keys -->";
    const END: &str = "<!-- END:prefix-keys -->";

    /// Keys the table lists that aren't bound to a catalog action: the
    /// hardcoded tab jumps and the two prefix-state escapes.
    const META_KEYS: &[&str] = &["1", "9", "C-g", "Ctrl+G", "Esc"];

    fn readme() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../README.md")
            .canonicalize()
            .expect("README.md not found");
        std::fs::read_to_string(path).expect("failed to read README.md")
    }

    /// The rows between the markers, as their first (key) column.
    fn key_column_rows(readme: &str) -> Vec<String> {
        let body = readme
            .split_once(BEGIN)
            .unwrap_or_else(|| panic!("README.md is missing the `{BEGIN}` marker"))
            .1
            .split_once(END)
            .unwrap_or_else(|| panic!("README.md is missing the `{END}` marker"))
            .0;

        body.lines()
            .filter(|l| l.starts_with('|'))
            // Skip the header row and its `|---|---|` separator.
            .filter(|l| !l.contains("---"))
            .filter(|l| !l.contains("| Action |"))
            .filter_map(|l| l.split('|').nth(1).map(str::to_string))
            .collect()
    }

    /// Every inline-code span in a markdown fragment. Handles the ``` `` ` `` ```
    /// form the backtick key needs, as well as plain `` `x` ``.
    fn code_spans(s: &str) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        let mut spans = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != '`' {
                i += 1;
                continue;
            }
            let fence = if chars.get(i + 1) == Some(&'`') { 2 } else { 1 };
            let open = i + fence;
            let mut j = open;
            let close = loop {
                if j >= chars.len() {
                    break None;
                }
                let run = chars[j..].iter().take_while(|c| **c == '`').count();
                if run == fence {
                    break Some(j);
                }
                j += if run > 0 { run } else { 1 };
            };
            match close {
                Some(end) => {
                    let inner: String = chars[open..end].iter().collect();
                    spans.push(inner.trim().to_string());
                    i = end + fence;
                }
                None => break,
            }
        }
        spans
    }

    #[test]
    fn readme_prefix_table_lists_every_action_key() {
        let cfg = crate::config::Config::default();
        let readme = readme();
        let documented: HashSet<String> = key_column_rows(&readme)
            .iter()
            .flat_map(|col| code_spans(col))
            .collect();

        for a in catalog() {
            let Some(chord) = cfg.prefix_chord(a.id) else {
                continue; // direct chords (copy) live in the focused-pane table
            };
            assert!(
                documented.contains(&chord),
                "README prefix table is missing `{chord}` ({}). Add a row for it \
                 between the {BEGIN} markers.",
                a.label,
            );
        }
    }

    #[test]
    fn readme_prefix_table_invents_no_keys() {
        let cfg = crate::config::Config::default();
        let readme = readme();

        let mut bound: HashSet<String> = cfg.all_prefix_chords().into_iter().collect();
        bound.extend(META_KEYS.iter().map(|s| s.to_string()));

        for col in key_column_rows(&readme) {
            for key in code_spans(&col) {
                assert!(
                    bound.contains(&key),
                    "README prefix table documents `{key}`, which no action binds. \
                     It was probably renamed — check `default_app()`.",
                );
            }
        }
    }
}
