use std::collections::HashMap;

use anyhow::Context;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// Detected operating system, used for platform-aware keybindings.
/// On macOS, `ctrl-*` bindings also accept `Cmd` (Super) and display as `cmd-*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
}

impl Platform {
    pub fn detect() -> Self {
        match std::env::consts::OS {
            "macos" => Self::MacOs,
            _ => Self::Linux,
        }
    }

    pub fn is_macos(self) -> bool {
        self == Self::MacOs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanConfig {
    pub provider: String,
    pub path: Option<String>,
}

impl Default for KanbanConfig {
    fn default() -> Self {
        Self {
            provider: "local".to_string(),
            path: Some("~/.config/flow/boards/default".to_string()),
        }
    }
}

/// `[notifications]` — how background agent events reach the user.
/// `delivery`: `"system"` (OS desktop toast, default), `"terminal"` (OSC 9
/// escape so the host terminal emulator notifies — works inside tmux/ssh),
/// or `"off"`. `sound` toggles the built-in chimes (done/attention),
/// independent of `delivery`; the `sound_*_path` overrides point at custom
/// audio files (any format your system player decodes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    pub delivery: String,
    pub sound: bool,
    pub sound_path: Option<String>,
    pub sound_done_path: Option<String>,
    pub sound_attention_path: Option<String>,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            delivery: "system".to_string(),
            sound: false,
            sound_path: None,
            sound_done_path: None,
            sound_attention_path: None,
        }
    }
}

impl NotificationsConfig {
    /// Parse `delivery` into the core enum, warning (once, at call time) on
    /// unknown values and falling back to the default.
    pub fn parsed_delivery(&self) -> piki_core::notifications::NotificationDelivery {
        use piki_core::notifications::NotificationDelivery as D;
        match self.delivery.as_str() {
            "off" => D::Off,
            "system" => D::System,
            "terminal" => D::Terminal,
            other => {
                tracing::warn!(
                    "unknown notifications.delivery '{other}' (expected off|system|terminal); using 'system'"
                );
                D::System
            }
        }
    }

    pub fn sound_settings(&self) -> piki_core::sound::SoundSettings {
        // Expand a leading `~/` so config paths like "~/sounds/ding.wav" work.
        let p = |s: &Option<String>| {
            s.as_ref().map(|s| match s.strip_prefix("~/") {
                Some(rest) => piki_core::xdg::home_dir().join(rest),
                None => std::path::PathBuf::from(s),
            })
        };
        piki_core::sound::SoundSettings {
            enabled: self.sound,
            path: p(&self.sound_path),
            done_path: p(&self.sound_done_path),
            attention_path: p(&self.sound_attention_path),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub theme: String,
    #[serde(default = "default_syntax_theme")]
    pub syntax_theme: String,
    #[serde(default)]
    pub keybindings: Keybindings,
    #[serde(default)]
    pub kanban: KanbanConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    /// Runtime-detected platform (not serialized).
    #[serde(skip)]
    pub platform: Platform,
}

fn default_syntax_theme() -> String {
    "base16-ocean.dark".to_string()
}

impl Default for Platform {
    fn default() -> Self {
        Self::detect()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            syntax_theme: default_syntax_theme(),
            keybindings: Keybindings::default(),
            kanban: KanbanConfig::default(),
            notifications: NotificationsConfig::default(),
            platform: Platform::detect(),
        }
    }
}

/// A binding value in `[keybindings.app]`: either a single binding string or a
/// list of alternatives. Strings starting with `prefix-` fire after the prefix
/// key (tmux-style); anything else is a direct chord.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindingValue {
    One(String),
    Many(Vec<String>),
}

impl BindingValue {
    pub fn one(s: &str) -> Self {
        Self::One(s.to_string())
    }

    pub fn many(items: &[&str]) -> Self {
        Self::Many(items.iter().map(|s| s.to_string()).collect())
    }

    pub fn values(&self) -> Vec<&str> {
        match self {
            Self::One(s) => vec![s.as_str()],
            Self::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// How a binding string in the `app` table is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingTrigger {
    /// Fires directly (e.g. `ctrl-shift-c`).
    Direct(KeyEvent),
    /// Fires after the prefix key (e.g. `prefix-c`).
    Prefix(KeyEvent),
}

/// Parse an `app`-table binding string into its trigger.
pub fn parse_binding_trigger(s: &str) -> Option<BindingTrigger> {
    match s.strip_prefix("prefix-") {
        Some(rest) => parse_key_event(rest).map(BindingTrigger::Prefix),
        None => parse_key_event(s).map(BindingTrigger::Direct),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybindings {
    /// The tmux-style prefix key. Pressing it twice sends the key literally to
    /// the terminal. Must come before the table fields for TOML serialization.
    #[serde(default = "default_prefix_key")]
    pub prefix_key: String,
    #[serde(default = "default_app")]
    pub app: HashMap<String, BindingValue>,
    #[serde(default = "default_scroll")]
    pub scroll: HashMap<String, String>,
    #[serde(default = "default_agents")]
    pub agents: HashMap<String, String>,
    #[serde(default = "default_markdown")]
    pub markdown: HashMap<String, String>,
    #[serde(default = "default_workspace_list")]
    pub workspace_list: HashMap<String, String>,
    #[serde(default = "default_help")]
    pub help: HashMap<String, String>,
    #[serde(default = "default_about")]
    pub about: HashMap<String, String>,
    #[serde(default = "default_workspace_info")]
    pub workspace_info: HashMap<String, String>,
    #[serde(default = "default_fuzzy")]
    pub fuzzy: HashMap<String, String>,
    #[serde(default = "default_editor")]
    pub editor: HashMap<String, String>,
    #[serde(default = "default_new_workspace")]
    pub new_workspace: HashMap<String, String>,
    #[serde(default = "default_new_tab")]
    pub new_tab: HashMap<String, String>,
    #[serde(default = "default_dashboard")]
    pub dashboard: HashMap<String, String>,
    #[serde(default = "default_logs")]
    pub logs: HashMap<String, String>,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            prefix_key: default_prefix_key(),
            app: default_app(),
            scroll: default_scroll(),
            agents: default_agents(),
            markdown: default_markdown(),
            workspace_list: default_workspace_list(),
            help: default_help(),
            about: default_about(),
            workspace_info: default_workspace_info(),
            fuzzy: default_fuzzy(),
            editor: default_editor(),
            new_workspace: default_new_workspace(),
            new_tab: default_new_tab(),
            dashboard: default_dashboard(),
            logs: default_logs(),
        }
    }
}

fn default_prefix_key() -> String {
    "ctrl-g".to_string()
}

/// Global app actions. All defaults are behind the prefix key except the
/// terminal clipboard/search chords; users can promote any action to a direct
/// chord (e.g. `next_tab = "alt-n"`) or supply alternatives as an array.
fn default_app() -> HashMap<String, BindingValue> {
    let mut m = HashMap::new();
    // Focus movement between panes
    m.insert("focus_left".to_string(), BindingValue::many(&["prefix-h", "prefix-left"]));
    m.insert("focus_down".to_string(), BindingValue::many(&["prefix-j", "prefix-down"]));
    m.insert("focus_up".to_string(), BindingValue::many(&["prefix-k", "prefix-up"]));
    m.insert("focus_right".to_string(), BindingValue::many(&["prefix-l", "prefix-right"]));

    // Tabs
    m.insert("new_tab".to_string(), BindingValue::one("prefix-c"));
    m.insert("close_tab".to_string(), BindingValue::one("prefix-x"));
    m.insert("next_tab".to_string(), BindingValue::one("prefix-n"));
    m.insert("prev_tab".to_string(), BindingValue::one("prefix-p"));

    // Workspaces
    m.insert("workspace_switcher".to_string(), BindingValue::one("prefix-w"));
    m.insert("next_workspace".to_string(), BindingValue::one("prefix-)"));
    m.insert("prev_workspace".to_string(), BindingValue::one("prefix-("));
    m.insert("toggle_prev_workspace".to_string(), BindingValue::one("prefix-`"));
    m.insert("new_workspace".to_string(), BindingValue::one("prefix-N"));
    m.insert("edit_workspace".to_string(), BindingValue::one("prefix-e"));
    m.insert("delete_workspace".to_string(), BindingValue::one("prefix-d"));
    m.insert("workspace_info".to_string(), BindingValue::one("prefix-i"));
    m.insert("clone_workspace".to_string(), BindingValue::one("prefix-R"));

    // Git (everything else is delegated to the lazygit tab)
    m.insert("git".to_string(), BindingValue::one("prefix-g"));

    // App
    m.insert("help".to_string(), BindingValue::one("prefix-?"));
    m.insert("about".to_string(), BindingValue::one("prefix-a"));
    m.insert("dashboard".to_string(), BindingValue::one("prefix-D"));
    m.insert("command_palette".to_string(), BindingValue::one("prefix-:"));
    m.insert("fuzzy_search".to_string(), BindingValue::one("prefix-/"));
    m.insert("chat_panel".to_string(), BindingValue::one("prefix-y"));
    m.insert("quit".to_string(), BindingValue::one("prefix-q"));
    m.insert("manage_agents".to_string(), BindingValue::one("prefix-A"));
    m.insert("manage_providers".to_string(), BindingValue::one("prefix-V"));
    m.insert("logs".to_string(), BindingValue::one("prefix-o"));
    m.insert("scroll_mode".to_string(), BindingValue::one("prefix-["));

    // Layout
    m.insert("sidebar_shrink".to_string(), BindingValue::many(&["prefix-<", "prefix-,"]));
    m.insert("sidebar_grow".to_string(), BindingValue::many(&["prefix->", "prefix-."]));
    m.insert("split_up".to_string(), BindingValue::many(&["prefix-+", "prefix-="]));
    m.insert("split_down".to_string(), BindingValue::one("prefix--"));

    // Terminal clipboard: direct chords, never sent to the PTY.
    m.insert("copy".to_string(), BindingValue::one("ctrl-shift-c"));
    m.insert("paste".to_string(), BindingValue::one("ctrl-shift-v"));
    // Terminal search is a prefix chord (not a direct Ctrl+Shift+F) so it
    // can't be swallowed by the terminal emulator's own bindings (ghostty).
    m.insert("terminal_search".to_string(), BindingValue::one("prefix-f"));

    m
}

/// Terminal scroll mode (entered with the `scroll_mode` app action).
fn default_scroll() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("page_down".to_string(), "ctrl-d".to_string());
    m.insert("page_up".to_string(), "ctrl-u".to_string());
    m.insert("page_down_alt".to_string(), "pagedown".to_string());
    m.insert("page_up_alt".to_string(), "pageup".to_string());
    m.insert("top".to_string(), "g".to_string());
    m.insert("bottom".to_string(), "G".to_string());
    m.insert("search".to_string(), "/".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m.insert("exit_alt".to_string(), "q".to_string());
    m
}

/// Agents pane (bottom-left): navigate active agents and jump to them.
fn default_agents() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("select".to_string(), "enter".to_string());
    m
}

fn default_markdown() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("page_down".to_string(), "ctrl-d".to_string());
    m.insert("page_up".to_string(), "ctrl-u".to_string());
    m.insert("scroll_top".to_string(), "g".to_string());
    m.insert("scroll_bottom".to_string(), "G".to_string());
    m
}


fn default_workspace_list() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("select".to_string(), "enter".to_string());
    m.insert("delete".to_string(), "d".to_string());
    m.insert("edit".to_string(), "e".to_string());
    m
}


fn default_help() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("page_down".to_string(), "ctrl-d".to_string());
    m.insert("page_up".to_string(), "ctrl-u".to_string());
    m.insert("scroll_top".to_string(), "g".to_string());
    m.insert("scroll_bottom".to_string(), "G".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m.insert("exit_alt".to_string(), "q".to_string());
    m.insert("exit_help".to_string(), "?".to_string());
    m
}

fn default_about() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit".to_string(), "esc".to_string());
    m
}

fn default_workspace_info() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("right".to_string(), "l".to_string());
    m.insert("right_alt".to_string(), "right".to_string());
    m.insert("left".to_string(), "h".to_string());
    m.insert("left_alt".to_string(), "left".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m.insert("exit_info".to_string(), "i".to_string());
    m
}

fn default_fuzzy() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("up".to_string(), "up".to_string());
    m.insert("down".to_string(), "down".to_string());
    m.insert("open".to_string(), "enter".to_string());
    m.insert("editor".to_string(), "ctrl-e".to_string());
    m.insert("inline_edit".to_string(), "ctrl-v".to_string());
    m.insert("markdown".to_string(), "ctrl-o".to_string());
    m.insert("mdr".to_string(), "alt-m".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m
}

fn default_editor() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("save".to_string(), "ctrl-s".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m
}

fn default_new_workspace() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("switch_field".to_string(), "tab".to_string());
    m.insert("create".to_string(), "enter".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m
}



fn default_dashboard() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("select".to_string(), "enter".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m.insert("exit_alt".to_string(), "D".to_string());
    m
}

fn default_logs() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("page_down".to_string(), "ctrl-d".to_string());
    m.insert("page_up".to_string(), "ctrl-u".to_string());
    m.insert("scroll_top".to_string(), "g".to_string());
    m.insert("scroll_bottom".to_string(), "G".to_string());
    m.insert("left".to_string(), "h".to_string());
    m.insert("right".to_string(), "l".to_string());
    m.insert("left_alt".to_string(), "left".to_string());
    m.insert("right_alt".to_string(), "right".to_string());
    m.insert("copy".to_string(), "enter".to_string());
    m.insert("copy_alt".to_string(), "y".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m.insert("exit_alt".to_string(), "ctrl-l".to_string());
    m
}

fn default_new_tab() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit".to_string(), "esc".to_string());
    m
}




impl Config {
    pub fn generate_default_toml() -> String {
        toml::to_string_pretty(&Self::default()).unwrap_or_default()
    }

    pub fn load_from(paths: &piki_core::paths::DataPaths) -> Self {
        let path = paths.config_path();
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &std::path::Path) -> Self {
        if !path.exists() {
            let default_config = Self::default();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(toml) = toml::to_string_pretty(&default_config) {
                let _ = std::fs::write(path, toml);
            }
            return default_config;
        }

        let mut cfg: Self = std::fs::read_to_string(path)
            .context("failed to read config file")
            .and_then(|data| toml::from_str(&data).context("failed to parse config file"))
            .unwrap_or_else(|e| {
                tracing::warn!(?path, %e, "failed to load config, using defaults");
                Self::default()
            });
        cfg.platform = Platform::detect();
        cfg.validate_keybindings();
        cfg
    }

    /// Whether this key event is the tmux-style prefix key.
    pub fn is_prefix_key(&self, event: KeyEvent) -> bool {
        key_matches_platform(event, &self.keybindings.prefix_key, self.platform)
    }

    /// The prefix key formatted for display (e.g. "C-g").
    pub fn prefix_display(&self) -> String {
        self.format_binding(&self.keybindings.prefix_key)
    }

    fn app_binding_strings(&self, action: &str) -> Vec<String> {
        self.keybindings
            .app
            .get(action)
            .cloned()
            .or_else(|| default_app().get(action).cloned())
            .map(|v| v.values().iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    /// Match a key against the prefix-mode bindings of an `app` action
    /// (binding strings starting with `prefix-`).
    pub fn matches_app_prefix(&self, event: KeyEvent, action: &str) -> bool {
        self.app_binding_strings(action).iter().any(|b| {
            b.strip_prefix("prefix-")
                .is_some_and(|rest| key_matches_platform(event, rest, self.platform))
        })
    }

    /// Match a key against the direct-chord bindings of an `app` action
    /// (binding strings without the `prefix-` marker).
    pub fn matches_app_direct(&self, event: KeyEvent, action: &str) -> bool {
        self.app_binding_strings(action).iter().any(|b| {
            !b.starts_with("prefix-") && key_matches_platform(event, b, self.platform)
        })
    }

    pub fn matches_scroll(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.scroll.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            let defaults = default_scroll();
            defaults
                .get(action)
                .is_some_and(|b| key_matches_platform(event, b, self.platform))
        }
    }

    /// Log warnings for misconfigured keybindings: unparseable strings,
    /// bindings that collide with the prefix key (reserved for the literal
    /// send), duplicate triggers, and direct chords without modifiers (which
    /// would shadow terminal input).
    pub fn validate_keybindings(&self) {
        let prefix = parse_key_event(&self.keybindings.prefix_key);
        if prefix.is_none() {
            tracing::warn!(
                prefix_key = %self.keybindings.prefix_key,
                "invalid prefix_key, falling back to ctrl-g"
            );
        }
        let mut seen: HashMap<(bool, KeyCode, KeyModifiers), String> = HashMap::new();
        let mut actions: Vec<&String> = self.keybindings.app.keys().collect();
        let defaults = default_app();
        actions.extend(defaults.keys().filter(|k| !self.keybindings.app.contains_key(*k)));
        for action in actions {
            for binding in self.app_binding_strings(action) {
                let Some(trigger) = parse_binding_trigger(&binding) else {
                    tracing::warn!(%action, %binding, "unparseable keybinding, ignored");
                    continue;
                };
                let (is_prefix, event) = match trigger {
                    BindingTrigger::Prefix(e) => (true, e),
                    BindingTrigger::Direct(e) => (false, e),
                };
                if !is_prefix
                    && prefix.is_some_and(|p| p.code == event.code && p.modifiers == event.modifiers)
                {
                    tracing::warn!(
                        %action, %binding,
                        "binding collides with the prefix key (reserved for literal send), ignored"
                    );
                    continue;
                }
                if !is_prefix
                    && event.modifiers.is_empty()
                    && matches!(event.code, KeyCode::Char(_))
                {
                    tracing::warn!(
                        %action, %binding,
                        "direct binding without modifiers shadows terminal input"
                    );
                }
                if let Some(other) = seen
                    .insert((is_prefix, event.code, event.modifiers), action.clone())
                    .filter(|other| other != action)
                {
                    tracing::warn!(
                        %binding, first = %other, second = %action,
                        "duplicate keybinding, the first match wins"
                    );
                }
            }
        }
    }

    pub fn matches_agents(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.agents.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            let defaults = default_agents();
            defaults
                .get(action)
                .is_some_and(|b| key_matches_platform(event, b, self.platform))
        }
    }

    pub fn matches_markdown(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.markdown.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            let defaults = default_markdown();
            defaults
                .get(action)
                .is_some_and(|b| key_matches_platform(event, b, self.platform))
        }
    }


    pub fn matches_workspace_list(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.workspace_list.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            let defaults = default_workspace_list();
            defaults
                .get(action)
                .is_some_and(|b| key_matches_platform(event, b, self.platform))
        }
    }


    pub fn matches_help(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.help.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            false
        }
    }

    pub fn matches_about(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.about.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            false
        }
    }

    pub fn matches_workspace_info(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.workspace_info.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            false
        }
    }

    pub fn matches_dashboard(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.dashboard.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            false
        }
    }

    pub fn matches_logs(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.logs.get(action) {
            key_matches_platform(event, binding, self.platform)
        } else {
            false
        }
    }




    /// Canonical display form of a binding — the single key grammar every
    /// surface (footer, help, palette, dialog hints) renders through:
    /// platform mapping first (macOS `ctrl-` → `cmd-`), then compact
    /// modifiers (`C-`, `M-`, `S-`, `Cmd-`) and capitalized special keys
    /// (`Esc`, `Enter`, arrows as glyphs). `"ctrl-shift-c"` → `"C-S-c"`.
    pub fn format_binding(&self, binding: &str) -> String {
        compact_binding_display(&format_binding_for_platform(binding, self.platform))
    }

    pub fn get_binding(&self, section: &str, action: &str) -> String {
        if section == "app" {
            return self
                .app_binding_strings(action)
                .first()
                .map(|b| self.display_app_binding(b))
                .unwrap_or_else(|| "???".to_string());
        }
        let binding = match section {
            "scroll" => self
                .keybindings
                .scroll
                .get(action)
                .cloned()
                .or_else(|| default_scroll().get(action).cloned()),
            "agents" => self
                .keybindings
                .agents
                .get(action)
                .cloned()
                .or_else(|| default_agents().get(action).cloned()),
            "markdown" => self
                .keybindings
                .markdown
                .get(action)
                .cloned()
                .or_else(|| default_markdown().get(action).cloned()),
            "workspace_list" => self
                .keybindings
                .workspace_list
                .get(action)
                .cloned()
                .or_else(|| default_workspace_list().get(action).cloned()),
            "help" => self
                .keybindings
                .help
                .get(action)
                .cloned()
                .or_else(|| default_help().get(action).cloned()),
            "about" => self
                .keybindings
                .about
                .get(action)
                .cloned()
                .or_else(|| default_about().get(action).cloned()),
            "workspace_info" => self
                .keybindings
                .workspace_info
                .get(action)
                .cloned()
                .or_else(|| default_workspace_info().get(action).cloned()),
            "fuzzy" => self
                .keybindings
                .fuzzy
                .get(action)
                .cloned()
                .or_else(|| default_fuzzy().get(action).cloned()),
            "editor" => self
                .keybindings
                .editor
                .get(action)
                .cloned()
                .or_else(|| default_editor().get(action).cloned()),
            "new_workspace" => self
                .keybindings
                .new_workspace
                .get(action)
                .cloned()
                .or_else(|| default_new_workspace().get(action).cloned()),
            "new_tab" => self
                .keybindings
                .new_tab
                .get(action)
                .cloned()
                .or_else(|| default_new_tab().get(action).cloned()),
            "dashboard" => self
                .keybindings
                .dashboard
                .get(action)
                .cloned()
                .or_else(|| default_dashboard().get(action).cloned()),
            "logs" => self
                .keybindings
                .logs
                .get(action)
                .cloned()
                .or_else(|| default_logs().get(action).cloned()),
            _ => None,
        };
        binding
            .map(|b| self.format_binding(&b))
            .unwrap_or_else(|| "???".to_string())
    }

    /// Display form of an `app` binding: `"prefix-c"` → `"C-g c"`,
    /// `"ctrl-shift-c"` → `"ctrl-shift-c"` (platform-formatted).
    fn display_app_binding(&self, binding: &str) -> String {
        match binding.strip_prefix("prefix-") {
            Some(rest) => format!("{} {}", self.prefix_display(), self.format_binding(rest)),
            None => self.format_binding(binding),
        }
    }
}

/// Compact modifier spelling plus capitalized special keys: `ctrl-g` →
/// `C-g`, `alt-p` → `M-p`, `esc` → `Esc`, `ctrl-pagedown` → `C-PgDn`,
/// `up` → `↑`.
fn compact_binding_display(binding: &str) -> String {
    let compacted = binding
        .replace("ctrl-", "C-")
        .replace("alt-", "M-")
        .replace("shift-", "S-")
        .replace("cmd-", "Cmd-");
    let (mods, key) = match compacted.rfind('-') {
        Some(i) if i + 1 < compacted.len() => (&compacted[..=i], &compacted[i + 1..]),
        _ => ("", compacted.as_str()),
    };
    let key_disp = match key {
        "esc" => "Esc",
        "enter" => "Enter",
        "tab" => "Tab",
        "space" => "Space",
        "backspace" => "Backspace",
        "delete" => "Del",
        "insert" => "Ins",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PgUp",
        "pagedown" => "PgDn",
        "up" => "↑",
        "down" => "↓",
        "left" => "←",
        "right" => "→",
        _ => key,
    };
    format!("{mods}{key_disp}")
}

/// Check if modifiers include Ctrl (or Super on macOS).
/// Use this instead of `key.modifiers.contains(KeyModifiers::CONTROL)` for
/// platform-aware key matching in input handlers.
pub fn has_ctrl(modifiers: KeyModifiers, platform: Platform) -> bool {
    modifiers.contains(KeyModifiers::CONTROL)
        || (platform.is_macos() && modifiers.contains(KeyModifiers::SUPER))
}

/// Check if modifiers include Alt (or Super on macOS, since Option doesn't send ALT).
pub fn has_alt(modifiers: KeyModifiers, platform: Platform) -> bool {
    modifiers.contains(KeyModifiers::ALT)
        || (platform.is_macos() && modifiers.contains(KeyModifiers::SUPER))
}

/// Format a binding string for display on the given platform.
/// On macOS, `ctrl-` and `alt-` become `cmd-` so users see the expected modifier.
pub fn format_binding_for_platform(binding: &str, platform: Platform) -> String {
    if platform.is_macos() {
        let lower = binding.to_lowercase();
        if lower.starts_with("ctrl-") {
            return format!("cmd-{}", &binding[5..]);
        }
        if lower.starts_with("alt-") {
            return format!("cmd-{}", &binding[4..]);
        }
    }
    binding.to_string()
}

pub fn parse_key_event(s: &str) -> Option<KeyEvent> {
    // Special-case: literal "-" character
    if s == "-" {
        return Some(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::empty()));
    }

    let parts: Vec<&str> = s.split('-').collect();
    let mut modifiers = KeyModifiers::empty();
    let code_str = if parts.len() > 1 {
        for &mod_str in &parts[..parts.len() - 1] {
            match mod_str.to_lowercase().as_str() {
                "ctrl" => modifiers.insert(KeyModifiers::CONTROL),
                "alt" => modifiers.insert(KeyModifiers::ALT),
                "shift" => modifiers.insert(KeyModifiers::SHIFT),
                "super" | "cmd" => modifiers.insert(KeyModifiers::SUPER),
                _ => return None,
            }
        }
        parts[parts.len() - 1]
    } else {
        parts[0]
    };

    let code = match code_str.to_lowercase().as_str() {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "esc" => KeyCode::Esc,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "insert" => KeyCode::Insert,
        "delete" => KeyCode::Delete,
        "space" => KeyCode::Char(' '),
        s if s.chars().count() == 1 => {
            // Use the original code_str to preserve case (the match lowercases it)
            let original_c = code_str.chars().next()?;
            // If the original char is uppercase, implicitly add SHIFT modifier
            if original_c.is_uppercase() {
                modifiers.insert(KeyModifiers::SHIFT);
            }
            KeyCode::Char(s.chars().next()?)
        }
        s if s.starts_with('f') && s.len() > 1 => {
            let n = s[1..].parse::<u8>().ok()?;
            KeyCode::F(n)
        }
        _ => return None,
    };

    Some(KeyEvent::new(code, modifiers))
}

/// Match a key event against a binding string (uses runtime OS detection).
/// Prefer `key_matches_platform` when `Platform` is already known.
#[cfg(test)]
pub fn key_matches(event: KeyEvent, binding: &str) -> bool {
    key_matches_platform(event, binding, Platform::detect())
}

/// Platform-aware key matching. On macOS, `ctrl-*` and `alt-*` bindings also
/// accept `super-*` (Cmd), because macOS Option key does not send ALT to terminals.
pub fn key_matches_platform(event: KeyEvent, binding: &str, platform: Platform) -> bool {
    if let Some(target) = parse_key_event(binding) {
        // Compare modifiers and key code. For Char variants, compare case-insensitively
        // because crossterm may send 'C' (uppercase) when Shift is held, while the
        // binding parser produces 'c' (lowercase).
        let code_match = match (event.code, target.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => a.eq_ignore_ascii_case(&b),
            (a, b) => a == b,
        };
        if code_match && event.modifiers == target.modifiers {
            return true;
        }
        // On macOS, also accept Super (Cmd) where the binding specifies Ctrl.
        if platform.is_macos() && code_match && target.modifiers.contains(KeyModifiers::CONTROL) {
            let macos_mods =
                (target.modifiers - KeyModifiers::CONTROL) | KeyModifiers::SUPER;
            if event.modifiers == macos_mods {
                return true;
            }
        }
        // On macOS, also accept Super (Cmd) where the binding specifies Alt,
        // because macOS Option key sends special characters instead of ALT.
        if platform.is_macos() && code_match && target.modifiers.contains(KeyModifiers::ALT) {
            let macos_mods =
                (target.modifiers - KeyModifiers::ALT) | KeyModifiers::SUPER;
            if event.modifiers == macos_mods {
                return true;
            }
        }
        false
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_valid_bindings() {
        let cfg = Config::default();
        // All app-table bindings should parse successfully (prefix or direct)
        for (action, value) in &cfg.keybindings.app {
            for binding in value.values() {
                assert!(
                    parse_binding_trigger(binding).is_some(),
                    "app binding '{}' = '{}' failed to parse",
                    action,
                    binding,
                );
            }
        }
        assert!(parse_key_event(&cfg.keybindings.prefix_key).is_some());
    }

    #[test]
    fn test_all_default_sections_parse() {
        let cfg = Config::default();
        let sections: Vec<(&str, &HashMap<String, String>)> = vec![
            ("scroll", &cfg.keybindings.scroll),
            ("agents", &cfg.keybindings.agents),
            ("help", &cfg.keybindings.help),
            ("fuzzy", &cfg.keybindings.fuzzy),
            ("editor", &cfg.keybindings.editor),
            ("new_tab", &cfg.keybindings.new_tab),
            ("new_workspace", &cfg.keybindings.new_workspace),
            ("workspace_list", &cfg.keybindings.workspace_list),
            ("about", &cfg.keybindings.about),
            ("workspace_info", &cfg.keybindings.workspace_info),
            ("markdown", &cfg.keybindings.markdown),
        ];
        for (section, bindings) in sections {
            for (action, binding) in bindings {
                assert!(
                    parse_key_event(binding).is_some(),
                    "section '{}' action '{}' = '{}' failed to parse",
                    section,
                    action,
                    binding,
                );
            }
        }
    }

    #[test]
    fn test_default_app_bindings_parse_and_do_not_collide() {
        let mut seen: HashMap<(bool, KeyCode, KeyModifiers), String> = HashMap::new();
        for (action, value) in default_app() {
            for binding in value.values() {
                let trigger = parse_binding_trigger(binding);
                assert!(trigger.is_some(), "app binding '{action}' = '{binding}' failed to parse");
                let (is_prefix, event) = match trigger.unwrap() {
                    BindingTrigger::Prefix(e) => (true, e),
                    BindingTrigger::Direct(e) => (false, e),
                };
                if let Some(other) =
                    seen.insert((is_prefix, event.code, event.modifiers), action.clone())
                {
                    panic!("app bindings collide: '{other}' and '{action}' share '{binding}'");
                }
            }
        }
    }

    #[test]
    fn test_default_scroll_bindings_parse() {
        for (action, binding) in default_scroll() {
            assert!(
                parse_key_event(&binding).is_some(),
                "scroll binding '{action}' = '{binding}' failed to parse",
            );
        }
    }

    #[test]
    fn test_parse_binding_trigger() {
        match parse_binding_trigger("prefix-C") {
            Some(BindingTrigger::Prefix(e)) => {
                // parse_key_event stores the lowercased char plus SHIFT
                assert_eq!(e.code, KeyCode::Char('c'));
                assert_eq!(e.modifiers, KeyModifiers::SHIFT);
            }
            other => panic!("expected Prefix trigger, got {other:?}"),
        }
        match parse_binding_trigger("ctrl-shift-c") {
            Some(BindingTrigger::Direct(e)) => {
                assert_eq!(e.code, KeyCode::Char('c'));
                assert_eq!(e.modifiers, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
            }
            other => panic!("expected Direct trigger, got {other:?}"),
        }
        // "prefix--" is prefix + literal '-'
        match parse_binding_trigger("prefix--") {
            Some(BindingTrigger::Prefix(e)) => assert_eq!(e.code, KeyCode::Char('-')),
            other => panic!("expected Prefix('-'), got {other:?}"),
        }
        assert!(parse_binding_trigger("prefix-bogus").is_none());
    }

    #[test]
    fn test_binding_value_toml_forms() {
        let toml_str = r#"
[keybindings.app]
next_tab = "alt-n"
focus_left = ["prefix-h", "prefix-left"]
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            cfg.keybindings.app.get("next_tab").unwrap().values(),
            vec!["alt-n"]
        );
        assert_eq!(
            cfg.keybindings.app.get("focus_left").unwrap().values(),
            vec!["prefix-h", "prefix-left"]
        );
        // Direct override matches directly, not behind the prefix
        let alt_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT);
        assert!(cfg.matches_app_direct(alt_n, "next_tab"));
        assert!(!cfg.matches_app_prefix(alt_n, "next_tab"));
        // Non-overridden actions fall back to defaults
        let c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty());
        assert!(cfg.matches_app_prefix(c, "new_tab"));
    }

    #[test]
    fn test_is_prefix_key_default_and_custom() {
        let cfg = Config::default();
        assert!(cfg.is_prefix_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)));
        assert!(!cfg.is_prefix_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty())));

        let toml_str = r#"
[keybindings]
prefix_key = "ctrl-a"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert!(cfg.is_prefix_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)));
        assert!(!cfg.is_prefix_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)));
    }

    #[test]
    fn test_get_binding_app_display() {
        // Pin the platform: on macOS the display would be "Cmd-g c"
        let cfg = Config {
            platform: Platform::Linux,
            ..Config::default()
        };
        assert_eq!(cfg.get_binding("app", "new_tab"), "C-g c");
        // Direct chords compact through the same grammar as prefix chords
        assert_eq!(cfg.get_binding("app", "copy"), "C-S-c");
        assert_eq!(cfg.get_binding("app", "nonexistent"), "???");
    }

    #[test]
    fn test_obsolete_config_sections_are_ignored() {
        // A pre-prefix config.toml with the old navigation/interaction tables
        // must still deserialize (unknown keys inside known tables are kept,
        // whole unknown tables are ignored by serde).
        let toml_str = r#"
[keybindings.navigation]
quit = "ctrl-q"

[keybindings.obsolete_table]
foo = "bar"
"#;
        let cfg: Result<Config, _> = toml::from_str(toml_str);
        assert!(cfg.is_ok());
    }

    #[test]
    fn test_key_matches_ctrl_modifier() {
        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert!(key_matches(event, "ctrl-g"));
        assert!(!key_matches(event, "g"));
        assert!(!key_matches(event, "alt-g"));
    }

    #[test]
    fn test_key_matches_ctrl_shift_c() {
        // Kitty protocol reports lowercase key + SHIFT modifier
        let event = KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert!(key_matches(event, "ctrl-shift-c"));
        assert!(!key_matches(event, "ctrl-c"));
        assert!(!key_matches(event, "c"));

        // Some terminals report uppercase key + SHIFT modifier
        let event = KeyEvent::new(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert!(key_matches(event, "ctrl-shift-c"));
    }

    #[test]
    fn test_key_matches_no_match() {
        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert!(key_matches(event, "q"));
        assert!(!key_matches(event, "w"));
        assert!(!key_matches(event, "ctrl-q"));
    }

    #[test]
    fn test_partial_toml_merge_with_defaults() {
        let toml_str = r#"
[keybindings.app]
quit = "prefix-Q"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        // Overridden binding
        assert_eq!(
            cfg.keybindings.app.get("quit").unwrap().values(),
            vec!["prefix-Q"]
        );
        // Other sections still get defaults
        assert!(cfg.keybindings.help.contains_key("exit"));
        assert!(cfg.keybindings.agents.contains_key("down"));
    }

    #[test]
    fn test_invalid_toml_fallback() {
        let result: Result<Config, _> = toml::from_str("{{invalid toml}}");
        assert!(result.is_err());
        // Config::load() would fall back to defaults — verify defaults are valid
        let cfg = Config::default();
        assert!(!cfg.keybindings.app.is_empty());
    }

    #[test]
    fn test_get_binding_falls_back_to_default() {
        // Pin the platform: on macOS the display would be "Cmd-g q"
        let cfg = Config {
            platform: Platform::Linux,
            ..Config::default()
        };
        assert_eq!(cfg.get_binding("app", "quit"), "C-g q");
        assert_eq!(cfg.get_binding("app", "help"), "C-g ?");
        // Unknown action returns "???"
        assert_eq!(cfg.get_binding("app", "nonexistent"), "???");
        // Unknown section returns "???"
        assert_eq!(cfg.get_binding("nonexistent_section", "quit"), "???");
    }

    #[test]
    fn test_matches_app_prefix_with_defaults() {
        let cfg = Config::default();
        let q_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert!(cfg.matches_app_prefix(q_event, "quit"));
        assert!(!cfg.matches_app_prefix(q_event, "help"));
        // A prefix binding is not a direct chord
        assert!(!cfg.matches_app_direct(q_event, "quit"));
    }

    #[test]
    fn test_notifications_config_defaults_and_parse() {
        use piki_core::notifications::NotificationDelivery as D;
        // Absent section → defaults
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.notifications.parsed_delivery(), D::System);
        assert!(!cfg.notifications.sound);

        let cfg: Config = toml::from_str(
            "[notifications]\ndelivery = \"terminal\"\nsound = true\nsound_done_path = \"/tmp/d.wav\"\n",
        )
        .unwrap();
        assert_eq!(cfg.notifications.parsed_delivery(), D::Terminal);
        let s = cfg.notifications.sound_settings();
        assert!(s.enabled);
        assert_eq!(s.done_path, Some(std::path::PathBuf::from("/tmp/d.wav")));
        assert_eq!(s.attention_path, None);

        // Unknown delivery falls back to system instead of failing
        let cfg: Config = toml::from_str("[notifications]\ndelivery = \"banana\"\n").unwrap();
        assert_eq!(cfg.notifications.parsed_delivery(), D::System);
    }

    #[test]
    fn test_generate_default_toml_roundtrips() {
        let toml_str = Config::generate_default_toml();
        let cfg: Config = toml::from_str(&toml_str).unwrap();
        // Verify key bindings survived the roundtrip
        assert_eq!(cfg.keybindings.prefix_key, "ctrl-g");
        assert_eq!(
            cfg.keybindings.app.get("quit").unwrap().values(),
            vec!["prefix-q"]
        );
        assert_eq!(
            cfg.keybindings.app.get("focus_left").unwrap().values(),
            vec!["prefix-h", "prefix-left"]
        );
    }

    // --- Platform-aware keybinding tests ---

    #[test]
    fn test_parse_super_modifier() {
        let event = parse_key_event("super-s").unwrap();
        assert_eq!(event.code, KeyCode::Char('s'));
        assert!(event.modifiers.contains(KeyModifiers::SUPER));
    }

    #[test]
    fn test_parse_cmd_modifier() {
        let event = parse_key_event("cmd-g").unwrap();
        assert_eq!(event.code, KeyCode::Char('g'));
        assert!(event.modifiers.contains(KeyModifiers::SUPER));
    }

    #[test]
    fn test_macos_ctrl_binding_accepts_super() {
        // On macOS, a ctrl-g binding should also match Super+g (Cmd+g)
        let super_event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SUPER);
        assert!(key_matches_platform(super_event, "ctrl-g", Platform::MacOs));
        // The original ctrl-g should still work on macOS
        let ctrl_event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert!(key_matches_platform(ctrl_event, "ctrl-g", Platform::MacOs));
    }

    #[test]
    fn test_linux_ctrl_binding_does_not_accept_super() {
        let super_event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SUPER);
        assert!(!key_matches_platform(super_event, "ctrl-g", Platform::Linux));
        // ctrl-g should still work on Linux
        let ctrl_event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert!(key_matches_platform(ctrl_event, "ctrl-g", Platform::Linux));
    }

    #[test]
    fn test_macos_ctrl_shift_binding_accepts_super_shift() {
        // ctrl-shift-c on macOS should match Super+Shift+c (Cmd+Shift+c)
        let event = KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        );
        assert!(key_matches_platform(event, "ctrl-shift-c", Platform::MacOs));
    }

    #[test]
    fn test_plain_bindings_unaffected_by_platform() {
        let q_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert!(key_matches_platform(q_event, "q", Platform::Linux));
        assert!(key_matches_platform(q_event, "q", Platform::MacOs));
    }

    #[test]
    fn test_macos_alt_binding_accepts_super() {
        // On macOS, alt-m should also match Super+m (Cmd+m) because Option
        // sends special characters instead of ALT in most terminals.
        let super_event = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::SUPER);
        assert!(key_matches_platform(super_event, "alt-m", Platform::MacOs));
        // Original ALT should still work on macOS (for terminals configured with Option as Meta)
        let alt_event = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::ALT);
        assert!(key_matches_platform(alt_event, "alt-m", Platform::MacOs));
    }

    #[test]
    fn test_linux_alt_binding_does_not_accept_super() {
        let super_event = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::SUPER);
        assert!(!key_matches_platform(super_event, "alt-m", Platform::Linux));
        // ALT should work on Linux
        let alt_event = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::ALT);
        assert!(key_matches_platform(alt_event, "alt-m", Platform::Linux));
    }

    #[test]
    fn test_format_binding_for_platform_linux() {
        assert_eq!(
            format_binding_for_platform("ctrl-g", Platform::Linux),
            "ctrl-g"
        );
        assert_eq!(
            format_binding_for_platform("ctrl-shift-c", Platform::Linux),
            "ctrl-shift-c"
        );
        assert_eq!(format_binding_for_platform("q", Platform::Linux), "q");
    }

    #[test]
    fn test_format_binding_for_platform_macos() {
        assert_eq!(
            format_binding_for_platform("ctrl-g", Platform::MacOs),
            "cmd-g"
        );
        assert_eq!(
            format_binding_for_platform("ctrl-shift-c", Platform::MacOs),
            "cmd-shift-c"
        );
        // alt-* also maps to cmd-* on macOS (Option doesn't send ALT)
        assert_eq!(
            format_binding_for_platform("alt-m", Platform::MacOs),
            "cmd-m"
        );
        // Plain bindings unchanged
        assert_eq!(format_binding_for_platform("q", Platform::MacOs), "q");
    }

    #[test]
    fn test_has_ctrl_linux() {
        assert!(has_ctrl(KeyModifiers::CONTROL, Platform::Linux));
        assert!(!has_ctrl(KeyModifiers::SUPER, Platform::Linux));
        assert!(!has_ctrl(KeyModifiers::ALT, Platform::Linux));
    }

    #[test]
    fn test_has_ctrl_macos() {
        assert!(has_ctrl(KeyModifiers::CONTROL, Platform::MacOs));
        assert!(has_ctrl(KeyModifiers::SUPER, Platform::MacOs));
        assert!(!has_ctrl(KeyModifiers::ALT, Platform::MacOs));
    }

    #[test]
    fn test_has_alt_linux() {
        assert!(has_alt(KeyModifiers::ALT, Platform::Linux));
        assert!(!has_alt(KeyModifiers::SUPER, Platform::Linux));
        assert!(!has_alt(KeyModifiers::CONTROL, Platform::Linux));
    }

    #[test]
    fn test_has_alt_macos() {
        assert!(has_alt(KeyModifiers::ALT, Platform::MacOs));
        assert!(has_alt(KeyModifiers::SUPER, Platform::MacOs));
        assert!(!has_alt(KeyModifiers::CONTROL, Platform::MacOs));
    }

    #[test]
    fn test_config_get_binding_macos_display() {
        let cfg = Config {
            platform: Platform::MacOs,
            ..Config::default()
        };
        // ctrl-shift-c should display with Cmd- on macOS
        assert_eq!(cfg.get_binding("app", "copy"), "Cmd-S-c");
        // Special keys render capitalized
        assert_eq!(cfg.get_binding("scroll", "exit"), "Esc");
    }
}
