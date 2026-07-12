//! Single source of truth for every user-facing key in the TUI.
//!
//! All four discoverability surfaces — the command palette, the which-key
//! overlay, the `prefix-?` help browser and the README's prefix table — derive
//! from [`catalog`], so adding or renaming a key touches one place instead of
//! the parallel hand-maintained lists it used to.
//!
//! Two axes describe an entry:
//!
//! - [`Context`] — *where* the key is live. [`Context::Global`] means a prefix
//!   chord (or a direct app chord); everything else is a pane or overlay whose
//!   keys only fire while it is focused. The palette and which-key show
//!   `Global` only; the help browser shows all of them, one section per context.
//! - [`Keys`] — *where the key comes from*. [`Keys::Bind`] resolves through a
//!   `[keybindings.*]` config table and so follows the user's rebinds;
//!   [`Keys::Raw`] is hardcoded in a handler but still platform-formatted (macOS
//!   Cmd); [`Keys::Fixed`] is a literal that isn't a single keystroke at all
//!   ("Type", "Mouse drag", "0-5").
//!
//! The parity tests below fail the build if a `Bind` points at a binding that
//! doesn't exist, or if the README's table drifts from the catalog.

/// Where a key is live.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    /// App action behind the prefix (or a direct app chord). The only context
    /// the palette and which-key display.
    Global,
    Scroll,
    Terminal,
    Chat,
    AgentsPane,
    WorkspacePane,
    Fuzzy,
    Palette,
    WorkspaceSwitcher,
    Kanban,
    Dispatch,
    DispatchCardMove,
    ManageProviders,
    ManageAgents,
    ImportAgents,
    AgentRole,
    InlineEditor,
    CodeReview,
    Dashboard,
    Logs,
}

impl Context {
    /// Section heading in the help browser.
    pub fn title(self) -> &'static str {
        match self {
            Context::Global => "Prefix actions",
            Context::Scroll => "Terminal scroll mode",
            Context::Terminal => "Terminal pane",
            Context::Chat => "AI Chat overlay",
            Context::AgentsPane => "Agents pane",
            Context::WorkspacePane => "Workspace list pane",
            Context::Fuzzy => "Fuzzy file search",
            Context::Palette => "Command palette",
            Context::WorkspaceSwitcher => "Workspace switcher",
            Context::Kanban => "Kanban board",
            Context::Dispatch => "Dispatch agent dialog",
            Context::DispatchCardMove => "Move dispatched card (after confirming delete)",
            Context::ManageProviders => "Manage providers",
            Context::ManageAgents => "Manage agents",
            Context::ImportAgents => "Import agents overlay",
            Context::AgentRole => "Agent role editor (step 2)",
            Context::InlineEditor => "Inline editor",
            Context::CodeReview => "Code Review (requires gh CLI, locked mode)",
            Context::Dashboard => "Dashboard",
            Context::Logs => "Logs",
        }
    }

    /// The app action that opens this context, if any — rendered next to the
    /// title so the help reads "Logs (C-g o)".
    pub fn opened_by(self) -> Option<&'static str> {
        match self {
            Context::Scroll => Some("scroll_mode"),
            Context::Chat => Some("chat_panel"),
            Context::Fuzzy => Some("fuzzy_search"),
            Context::Palette => Some("command_palette"),
            Context::WorkspaceSwitcher => Some("workspace_switcher"),
            Context::ManageProviders => Some("manage_providers"),
            Context::ManageAgents => Some("manage_agents"),
            Context::Dashboard => Some("dashboard"),
            Context::Logs => Some("logs"),
            _ => None,
        }
    }
}

/// The order the help browser renders its sections in.
pub const HELP_ORDER: &[Context] = &[
    Context::Global,
    Context::Scroll,
    Context::Terminal,
    Context::WorkspacePane,
    Context::AgentsPane,
    Context::Fuzzy,
    Context::Palette,
    Context::WorkspaceSwitcher,
    Context::Dashboard,
    Context::Logs,
    Context::Chat,
    Context::Kanban,
    Context::Dispatch,
    Context::DispatchCardMove,
    Context::ManageAgents,
    Context::AgentRole,
    Context::ImportAgents,
    Context::ManageProviders,
    Context::InlineEditor,
    Context::CodeReview,
];

/// Where an entry's key text comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Keys {
    /// Resolved from a `[keybindings.<table>]` config table — follows rebinds.
    Bind(&'static str, &'static str),
    /// Hardcoded in a handler, but still platform-formatted (macOS Cmd).
    /// Prefer `Bind`; reach for this only when the handler really does hardcode
    /// the key.
    Raw(&'static str),
    /// A literal that isn't one keystroke — "Type", "Mouse drag", "0-5",
    /// "h/l/j/k". May contain `{prefix}`, substituted at render time.
    Fixed(&'static str),
}

impl Keys {
    /// The key text to show the user, in this config's key grammar (so macOS
    /// sees `Cmd-` and a rebound key shows the user's choice, not the default).
    pub fn display(self, cfg: &crate::config::Config) -> String {
        match self {
            Keys::Bind(table, action) => cfg.get_binding(table, action),
            Keys::Raw(binding) => cfg.format_binding(binding),
            Keys::Fixed(text) => text.replace("{prefix}", &cfg.prefix_display()),
        }
    }
}

/// One user-facing key and what it does.
pub struct ActionMeta {
    /// Stable id — matches the `[keybindings.app]` action name. `Global` only;
    /// empty elsewhere, where [`Keys`] carries the join instead.
    pub id: &'static str,
    pub context: Context,
    /// Grouping within a section. `Global` only (the which-key columns).
    pub category: &'static str,
    /// Full human label, shown in the palette and the help browser.
    pub label: &'static str,
    /// Terse label for the space-constrained which-key overlay. `Global` only.
    pub short: &'static str,
    pub keys: Keys,
}

/// A prefix-reachable app action. Its key always lives in `[keybindings.app]`
/// under the same id.
const fn app(
    id: &'static str,
    category: &'static str,
    label: &'static str,
    short: &'static str,
) -> ActionMeta {
    ActionMeta {
        id,
        context: Context::Global,
        category,
        label,
        short,
        keys: Keys::Bind("app", id),
    }
}

/// A key that is only live inside one pane or overlay.
const fn local(context: Context, keys: Keys, label: &'static str) -> ActionMeta {
    ActionMeta {
        id: "",
        context,
        category: "",
        label,
        short: "",
        keys,
    }
}

/// A `Global` key the config doesn't own — the tab jumps, the prefix-state
/// escapes, the mouse. It has no id, so the palette and which-key skip it; only
/// the help browser lists it.
const fn global_fixed(category: &'static str, keys: Keys, label: &'static str) -> ActionMeta {
    ActionMeta {
        id: "",
        context: Context::Global,
        category,
        label,
        short: "",
        keys,
    }
}

/// Every user-facing key, in display order.
pub fn catalog() -> &'static [ActionMeta] {
    CATALOG
}

use Context as C;
use Keys::{Bind, Fixed, Raw};

static CATALOG: &[ActionMeta] = {
    &[
        // ── Global: prefix actions ────────────────────────────────────────
        app("new_workspace", "Workspace", "New Workspace", "new"),
        app("clone_workspace", "Workspace", "Create Worktree (GitHub-only)", "worktree"),
        app("edit_workspace", "Workspace", "Edit Workspace", "edit"),
        app("delete_workspace", "Workspace", "Delete Workspace", "delete"),
        app("dashboard", "Workspace", "Dashboard", "dashboard"),
        app("workspace_info", "Workspace", "Workspace Info", "info"),
        app("workspace_switcher", "Workspace", "Switch Workspace (fuzzy)", "switch"),
        app("next_workspace", "Workspace", "Next Workspace", "next"),
        app("prev_workspace", "Workspace", "Previous Workspace", "prev"),
        app("toggle_prev_workspace", "Workspace", "Toggle Last Workspace", "last"),
        app("git", "Git", "Git (lazygit)", "lazygit"),
        app("new_tab", "Tabs", "New Tab", "new"),
        app("close_tab", "Tabs", "Close Tab", "close"),
        app("next_tab", "Tabs", "Next Tab", "next"),
        app("prev_tab", "Tabs", "Previous Tab", "prev"),
        app("fuzzy_search", "Search", "Fuzzy File Search", "find file"),
        app("terminal_search", "Search", "Search in Terminal", "search"),
        app("command_palette", "View", "Command Palette", "palette"),
        app("help", "View", "Help", "all keys"),
        app("about", "View", "About", "about"),
        app("logs", "View", "Logs", "logs"),
        app("scroll_mode", "View", "Terminal Scroll Mode", "scroll"),
        app("chat_panel", "View", "AI Chat", "chat"),
        app("focus_left", "Focus", "Focus Pane Left", "left"),
        app("focus_down", "Focus", "Focus Pane Down", "down"),
        app("focus_up", "Focus", "Focus Pane Up", "up"),
        app("focus_right", "Focus", "Focus Pane Right", "right"),
        app("sidebar_shrink", "Layout", "Shrink Sidebar", "sidebar −"),
        app("sidebar_grow", "Layout", "Grow Sidebar", "sidebar +"),
        app("split_up", "Layout", "Grow Left Split", "split +"),
        app("split_down", "Layout", "Shrink Left Split", "split −"),
        global_fixed("Layout", Fixed("Mouse drag"), "Drag a pane border to resize it"),
        app("copy", "Clipboard", "Copy Terminal", "copy"),
        app("manage_agents", "Agents", "Manage Agents", "agents"),
        app("manage_providers", "Providers", "Manage Providers", "providers"),
        app("quit", "App", "Quit", "quit"),
        // Prefix-state keys the config doesn't own.
        global_fixed("Prefix", Fixed("{prefix} 1..9"), "Jump to tab N"),
        global_fixed("Prefix", Fixed("{prefix} {prefix}"), "Send the prefix key to the terminal"),
        global_fixed("Prefix", Fixed("Esc"), "Cancel a pending prefix"),
        // ── Terminal scroll mode ──────────────────────────────────────────
        local(C::Scroll, Bind("scroll", "down"), "Scroll down a line"),
        local(C::Scroll, Bind("scroll", "up"), "Scroll up a line"),
        local(C::Scroll, Bind("scroll", "page_down"), "Page down"),
        local(C::Scroll, Bind("scroll", "page_up"), "Page up"),
        local(C::Scroll, Bind("scroll", "top"), "Jump to the top"),
        local(C::Scroll, Bind("scroll", "bottom"), "Jump to the bottom"),
        local(C::Scroll, Bind("scroll", "search"), "Search the scrollback"),
        local(C::Scroll, Bind("scroll", "exit"), "Exit scroll mode"),
        local(C::Scroll, Bind("scroll", "exit_alt"), "Exit scroll mode (alt)"),
        // ── Terminal pane ─────────────────────────────────────────────────
        local(C::Terminal, Fixed("(any)"), "Every key is forwarded to the active tab"),
        local(C::Terminal, Bind("app", "paste"), "Paste from the clipboard"),
        local(C::Terminal, Fixed("Mouse scroll"), "Scroll the terminal"),
        local(C::Terminal, Fixed("Mouse drag"), "Select text (copies on release)"),
        // ── Workspace list pane ───────────────────────────────────────────
        local(C::WorkspacePane, Bind("workspaces", "down"), "Select the next workspace"),
        local(C::WorkspacePane, Bind("workspaces", "up"), "Select the previous workspace"),
        local(C::WorkspacePane, Bind("workspaces", "collapse"), "Collapse the group"),
        local(C::WorkspacePane, Bind("workspaces", "expand"), "Expand the group"),
        local(C::WorkspacePane, Bind("workspaces", "select"), "Switch to it / toggle the group"),
        // ── Agents pane ───────────────────────────────────────────────────
        local(C::AgentsPane, Bind("agents", "down"), "Select the next agent"),
        local(C::AgentsPane, Bind("agents", "up"), "Select the previous agent"),
        local(C::AgentsPane, Bind("agents", "select"), "Jump to that workspace and tab"),
        local(C::AgentsPane, Fixed("Click"), "Jump to that workspace and tab"),
        // ── Fuzzy file search ─────────────────────────────────────────────
        local(C::Fuzzy, Fixed("Type"), "Filter files"),
        local(C::Fuzzy, Bind("fuzzy", "down"), "Select the next result"),
        local(C::Fuzzy, Bind("fuzzy", "up"), "Select the previous result"),
        local(C::Fuzzy, Bind("fuzzy", "open"), "Open in $EDITOR"),
        local(C::Fuzzy, Bind("fuzzy", "editor"), "Open in $EDITOR (without closing)"),
        local(C::Fuzzy, Bind("fuzzy", "inline_edit"), "Open in the inline editor"),
        local(C::Fuzzy, Bind("fuzzy", "markdown"), "Open a markdown file in a new tab"),
        local(C::Fuzzy, Bind("fuzzy", "mdr"), "Open a markdown file in mdr (external)"),
        local(C::Fuzzy, Bind("fuzzy", "exit"), "Close"),
        // ── Command palette ───────────────────────────────────────────────
        local(C::Palette, Fixed("Type"), "Filter commands"),
        local(C::Palette, Bind("fuzzy", "down"), "Select the next command"),
        local(C::Palette, Bind("fuzzy", "up"), "Select the previous command"),
        local(C::Palette, Bind("fuzzy", "open"), "Run the selected command"),
        local(C::Palette, Bind("fuzzy", "exit"), "Close"),
        // ── Workspace switcher ────────────────────────────────────────────
        local(C::WorkspaceSwitcher, Fixed("Type"), "Filter workspaces and tabs"),
        local(C::WorkspaceSwitcher, Fixed("↑/↓"), "Select a row"),
        local(C::WorkspaceSwitcher, Raw("ctrl-p"), "Select the previous row"),
        local(C::WorkspaceSwitcher, Raw("ctrl-n"), "Select the next row"),
        local(C::WorkspaceSwitcher, Fixed("Enter"), "Jump to that workspace or tab"),
        local(C::WorkspaceSwitcher, Fixed("Esc"), "Close"),
        // ── Dashboard ─────────────────────────────────────────────────────
        local(C::Dashboard, Bind("dashboard", "down"), "Select the next workspace"),
        local(C::Dashboard, Bind("dashboard", "up"), "Select the previous workspace"),
        local(C::Dashboard, Bind("dashboard", "select"), "Switch to it and focus the main panel"),
        local(C::Dashboard, Bind("dashboard", "exit"), "Close"),
        // ── Logs ──────────────────────────────────────────────────────────
        local(C::Logs, Bind("logs", "down"), "Select the next entry"),
        local(C::Logs, Bind("logs", "up"), "Select the previous entry"),
        local(C::Logs, Bind("logs", "page_down"), "Page down"),
        local(C::Logs, Bind("logs", "page_up"), "Page up"),
        local(C::Logs, Bind("logs", "scroll_top"), "Jump to the top"),
        local(C::Logs, Bind("logs", "scroll_bottom"), "Jump to the bottom"),
        local(C::Logs, Fixed("0-5"), "Filter by level (0 = all)"),
        local(C::Logs, Fixed("/"), "Search entries"),
        local(C::Logs, Fixed("r"), "Toggle auto-refresh (tail)"),
        local(C::Logs, Bind("logs", "copy"), "Copy the selected entry"),
        local(C::Logs, Bind("logs", "exit"), "Close"),
        // ── AI Chat ───────────────────────────────────────────────────────
        local(C::Chat, Fixed("Enter"), "Send the message"),
        local(C::Chat, Fixed("Tab"), "Select the model"),
        local(C::Chat, Raw("ctrl-o"), "Settings (server, URL, system prompt)"),
        local(C::Chat, Raw("ctrl-a"), "Toggle agent mode (tool use)"),
        local(C::Chat, Raw("ctrl-l"), "Clear the conversation"),
        local(C::Chat, Fixed("Esc"), "Hide (keeps state)"),
        // ── Kanban board ──────────────────────────────────────────────────
        local(C::Kanban, Fixed("h/l/j/k"), "Navigate columns and cards"),
        local(C::Kanban, Fixed("H/L"), "Move the card left/right"),
        local(C::Kanban, Fixed("n / a"), "New card"),
        local(C::Kanban, Fixed("e"), "Edit the selected card"),
        local(C::Kanban, Fixed("d"), "Delete the card"),
        local(C::Kanban, Fixed("D"), "Dispatch an agent (branch + AI)"),
        local(C::Kanban, Fixed("Enter"), "Toggle card details"),
        local(C::Kanban, Fixed("r"), "Refresh the board"),
        local(C::Kanban, Fixed("s"), "Toggle sort by priority"),
        local(C::Kanban, Fixed("/"), "Search cards"),
        local(C::Kanban, Fixed("p"), "Filter by project"),
        local(C::Kanban, Fixed("Esc"), "Close"),
        // ── Dispatch agent dialog ─────────────────────────────────────────
        local(C::Dispatch, Fixed("←/→/Tab"), "Step 1: cycle the agent / provider"),
        local(C::Dispatch, Fixed("Type"), "Step 1: add extra prompt instructions"),
        local(C::Dispatch, Fixed("Enter"), "Step 1: continue to the destination"),
        local(C::Dispatch, Fixed("←/→/Tab"), "Step 2: new worktree or current workspace"),
        local(C::Dispatch, Fixed("Enter"), "Step 2: dispatch"),
        local(C::Dispatch, Fixed("Esc"), "Back a step, or cancel"),
        // ── Move dispatched card ──────────────────────────────────────────
        local(C::DispatchCardMove, Fixed("j/k"), "Select the target kanban column"),
        local(C::DispatchCardMove, Fixed("Enter"), "Move the card and delete the workspace"),
        local(C::DispatchCardMove, Fixed("Esc"), "Cancel"),
        // ── Manage agents ─────────────────────────────────────────────────
        local(C::ManageAgents, Fixed("j/k"), "Navigate the agent list"),
        local(C::ManageAgents, Fixed("n"), "New agent (step 1: name + provider)"),
        local(C::ManageAgents, Fixed("e / Enter"), "Edit the selected agent"),
        local(C::ManageAgents, Fixed("d"), "Delete the selected agent"),
        local(C::ManageAgents, Fixed("p"), "Sync the agent to the repo"),
        local(C::ManageAgents, Fixed("i"), "Import agents from the repo"),
        local(C::ManageAgents, Fixed("Esc"), "Close"),
        // ── Agent role editor ─────────────────────────────────────────────
        local(C::AgentRole, Raw("ctrl-s"), "Save the agent and close"),
        local(C::AgentRole, Fixed("Esc"), "Back to step 1 without saving"),
        // ── Import agents ─────────────────────────────────────────────────
        local(C::ImportAgents, Fixed("j/k"), "Navigate the discovered agents"),
        local(C::ImportAgents, Fixed("Space"), "Toggle the selection"),
        local(C::ImportAgents, Fixed("a"), "Toggle select-all"),
        local(C::ImportAgents, Fixed("Enter"), "Import the selected agents"),
        local(C::ImportAgents, Fixed("Esc"), "Cancel"),
        // ── Manage providers ──────────────────────────────────────────────
        local(C::ManageProviders, Fixed("j/k"), "Navigate the provider list"),
        local(C::ManageProviders, Fixed("n"), "New provider"),
        local(C::ManageProviders, Fixed("e / Enter"), "Edit the selected provider"),
        local(C::ManageProviders, Fixed("d"), "Delete the selected provider"),
        local(C::ManageProviders, Fixed("Esc"), "Close"),
        // ── Inline editor ─────────────────────────────────────────────────
        local(C::InlineEditor, Bind("editor", "save"), "Save"),
        local(C::InlineEditor, Bind("editor", "exit"), "Close"),
        // ── Code review ───────────────────────────────────────────────────
        local(C::CodeReview, Fixed("j/k"), "Navigate files / scroll the diff"),
        local(C::CodeReview, Fixed("Enter"), "View the file diff"),
        local(C::CodeReview, Fixed("h/l"), "Switch between the file list and the diff"),
        local(C::CodeReview, Fixed("n/p"), "Next / previous file (in the diff view)"),
        local(C::CodeReview, Fixed("g/G"), "Top / bottom of the diff"),
        local(C::CodeReview, Raw("ctrl-d"), "Page down in the diff"),
        local(C::CodeReview, Raw("ctrl-u"), "Page up in the diff"),
        local(C::CodeReview, Fixed("c"), "Add or edit a comment on the line"),
        local(C::CodeReview, Fixed("d"), "Delete the comment on the line"),
        local(C::CodeReview, Fixed("[ / ]"), "Resize the file list / diff split"),
        local(C::CodeReview, Fixed("s"), "Open the submit-review dialog"),
        local(C::CodeReview, Fixed("r"), "Refresh the PR data"),
        local(C::CodeReview, Fixed("q"), "Close the review (discards state)"),
        local(C::CodeReview, Fixed("Tab"), "Cycle the verdict (in submit)"),
        local(C::CodeReview, Raw("ctrl-shift-d"), "Discard the draft (in submit)"),
    ]
};

/// The `Global` entries that carry a real app action id — what the palette and
/// which-key iterate.
pub fn global_actions() -> impl Iterator<Item = &'static ActionMeta> {
    catalog()
        .iter()
        .filter(|a| a.context == Context::Global && !a.id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every `Bind` must resolve to a real binding — the parity guard that
    /// nothing enforced before. A typo'd action name or a table that lost a key
    /// fails here instead of silently rendering `???` in the help browser.
    #[test]
    fn every_bind_resolves_to_a_real_binding() {
        let cfg = crate::config::Config::default();
        for a in catalog() {
            let Keys::Bind(table, action) = a.keys else {
                continue;
            };
            assert_ne!(
                cfg.get_binding(table, action),
                "???",
                "catalog entry '{}' binds [keybindings.{table}].{action}, which has no default",
                a.label,
            );
        }
    }

    #[test]
    fn global_action_ids_are_unique() {
        let mut seen = HashSet::new();
        for a in global_actions() {
            assert!(seen.insert(a.id), "duplicate catalog id '{}'", a.id);
        }
    }

    /// The which-key overlay renders `short`, so a Global action without one
    /// would show a blank cell.
    #[test]
    fn global_actions_have_a_short_label() {
        for a in global_actions() {
            assert!(!a.short.is_empty(), "global action '{}' has no short label", a.id);
        }
    }

    /// Every context in the catalog must have a slot in the help browser, or
    /// its keys would silently never render.
    #[test]
    fn every_context_is_in_the_help_order() {
        for a in catalog() {
            assert!(
                HELP_ORDER.contains(&a.context),
                "context {:?} ('{}') is missing from HELP_ORDER",
                a.context,
                a.label,
            );
        }
    }

    /// `opened_by` joins against the app table, same as `Keys::Bind`.
    #[test]
    fn opened_by_points_at_a_real_action() {
        let cfg = crate::config::Config::default();
        for ctx in HELP_ORDER {
            let Some(id) = ctx.opened_by() else { continue };
            assert_ne!(
                cfg.get_binding("app", id),
                "???",
                "context {ctx:?} is opened_by '{id}', which is not an app action",
            );
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

        for a in global_actions() {
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
