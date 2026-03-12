use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub keybindings: Keybindings,
    #[serde(default)]
    pub kanban: KanbanConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            keybindings: Keybindings::default(),
            kanban: KanbanConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybindings {
    #[serde(default = "default_navigation")]
    pub navigation: HashMap<String, String>,
    #[serde(default = "default_interaction")]
    pub interaction: HashMap<String, String>,
    #[serde(default = "default_markdown")]
    pub markdown: HashMap<String, String>,
    #[serde(default = "default_diff")]
    pub diff: HashMap<String, String>,
    #[serde(default = "default_workspace_list")]
    pub workspace_list: HashMap<String, String>,
    #[serde(default = "default_file_list")]
    pub file_list: HashMap<String, String>,
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
    #[serde(default = "default_commit")]
    pub commit: HashMap<String, String>,
    #[serde(default = "default_merge")]
    pub merge: HashMap<String, String>,
    #[serde(default = "default_new_tab")]
    pub new_tab: HashMap<String, String>,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            navigation: default_navigation(),
            interaction: default_interaction(),
            markdown: default_markdown(),
            diff: default_diff(),
            workspace_list: default_workspace_list(),
            file_list: default_file_list(),
            help: default_help(),
            about: default_about(),
            workspace_info: default_workspace_info(),
            fuzzy: default_fuzzy(),
            editor: default_editor(),
            new_workspace: default_new_workspace(),
            commit: default_commit(),
            merge: default_merge(),
            new_tab: default_new_tab(),
        }
    }
}

fn default_navigation() -> HashMap<String, String> {
    let mut m = HashMap::new();
    // Pane navigation
    m.insert("left".to_string(), "h".to_string());
    m.insert("right".to_string(), "l".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down".to_string(), "j".to_string());
    m.insert("left_alt".to_string(), "left".to_string());
    m.insert("right_alt".to_string(), "right".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("down_alt".to_string(), "down".to_string());

    // App state
    m.insert("enter_pane".to_string(), "enter".to_string());
    m.insert("quit".to_string(), "q".to_string());
    m.insert("help".to_string(), "?".to_string());
    m.insert("about".to_string(), "a".to_string());
    m.insert("workspace_info".to_string(), "i".to_string());
    m.insert("edit_workspace".to_string(), "e".to_string());
    m.insert("kanban".to_string(), "b".to_string());
    m.insert("new_workspace".to_string(), "n".to_string());
    m.insert("delete_workspace".to_string(), "d".to_string());
    m.insert("clone_workspace".to_string(), "r".to_string());
    m.insert("commit".to_string(), "c".to_string());
    m.insert("merge".to_string(), "M".to_string());
    m.insert("push".to_string(), "P".to_string());
    m.insert("undo".to_string(), "ctrl-z".to_string());

    // Tabs & Workspaces
    m.insert("next_workspace".to_string(), "tab".to_string());
    m.insert("prev_workspace".to_string(), "shift-tab".to_string());
    m.insert("next_tab".to_string(), "g".to_string());
    m.insert("prev_tab".to_string(), "G".to_string());
    m.insert("new_tab".to_string(), "t".to_string());
    m.insert("close_tab".to_string(), "w".to_string());

    // Scrolling
    m.insert("scroll_up".to_string(), "K".to_string());
    m.insert("scroll_down".to_string(), "J".to_string());
    m.insert("page_up".to_string(), "pageup".to_string());
    m.insert("page_down".to_string(), "pagedown".to_string());

    // Clipboard & Search
    m.insert("copy".to_string(), "ctrl-shift-c".to_string());
    m.insert("fuzzy_search".to_string(), "/".to_string());
    m.insert("fuzzy_search_alt".to_string(), "ctrl-f".to_string());

    // Resizing
    m.insert("sidebar_shrink".to_string(), "<".to_string());
    m.insert("sidebar_shrink_alt".to_string(), ",".to_string());
    m.insert("sidebar_grow".to_string(), ">".to_string());
    m.insert("sidebar_grow_alt".to_string(), ".".to_string());
    m.insert("split_up".to_string(), "+".to_string());
    m.insert("split_up_alt".to_string(), "=".to_string());
    m.insert("split_down".to_string(), "-".to_string());

    m
}

fn default_interaction() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit_interaction".to_string(), "ctrl-g".to_string());
    m.insert("paste".to_string(), "ctrl-shift-v".to_string());
    m.insert("copy".to_string(), "ctrl-shift-c".to_string());
    m.insert("search".to_string(), "ctrl-shift-f".to_string());
    m
}

fn default_markdown() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit_interaction".to_string(), "ctrl-g".to_string());
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

fn default_diff() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit".to_string(), "esc".to_string());
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("page_down".to_string(), "ctrl-d".to_string());
    m.insert("page_up".to_string(), "ctrl-u".to_string());
    m.insert("scroll_top".to_string(), "g".to_string());
    m.insert("scroll_bottom".to_string(), "G".to_string());
    m.insert("next_file".to_string(), "n".to_string());
    m.insert("prev_file".to_string(), "p".to_string());
    m
}

fn default_workspace_list() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit_interaction".to_string(), "ctrl-g".to_string());
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("select".to_string(), "enter".to_string());
    m.insert("delete".to_string(), "d".to_string());
    m
}

fn default_file_list() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("exit_interaction".to_string(), "ctrl-g".to_string());
    m.insert("down".to_string(), "j".to_string());
    m.insert("up".to_string(), "k".to_string());
    m.insert("down_alt".to_string(), "down".to_string());
    m.insert("up_alt".to_string(), "up".to_string());
    m.insert("diff".to_string(), "enter".to_string());
    m.insert("edit_external".to_string(), "e".to_string());
    m.insert("edit_inline".to_string(), "v".to_string());
    m.insert("stage".to_string(), "s".to_string());
    m.insert("unstage".to_string(), "u".to_string());
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
    m.insert("diff".to_string(), "enter".to_string());
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

fn default_commit() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("commit".to_string(), "enter".to_string());
    m.insert("exit".to_string(), "esc".to_string());
    m
}

fn default_merge() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("merge".to_string(), "m".to_string());
    m.insert("rebase".to_string(), "r".to_string());
    m.insert("exit".to_string(), "esc".to_string());
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

    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            let default_config = Self::default();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(toml) = toml::to_string_pretty(&default_config) {
                let _ = std::fs::write(&path, toml);
            }
            return default_config;
        }

        std::fs::read_to_string(&path)
            .context("failed to read config file")
            .and_then(|data| toml::from_str(&data).context("failed to parse config file"))
            .unwrap_or_else(|e| {
                tracing::warn!(?path, %e, "failed to load config, using defaults");
                Self::default()
            })
    }

    pub fn matches_navigation(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.navigation.get(action) {
            key_matches(event, binding)
        } else {
            let defaults = default_navigation();
            defaults
                .get(action)
                .map_or(false, |b| key_matches(event, b))
        }
    }

    pub fn matches_interaction(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.interaction.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_markdown(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.markdown.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_diff(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.diff.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_workspace_list(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.workspace_list.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_file_list(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.file_list.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_help(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.help.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_about(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.about.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn matches_workspace_info(&self, event: KeyEvent, action: &str) -> bool {
        if let Some(binding) = self.keybindings.workspace_info.get(action) {
            key_matches(event, binding)
        } else {
            false
        }
    }

    pub fn get_binding(&self, section: &str, action: &str) -> String {
        let binding = match section {
            "navigation" => self
                .keybindings
                .navigation
                .get(action)
                .cloned()
                .or_else(|| default_navigation().get(action).cloned()),
            "interaction" => self
                .keybindings
                .interaction
                .get(action)
                .cloned()
                .or_else(|| default_interaction().get(action).cloned()),
            "markdown" => self
                .keybindings
                .markdown
                .get(action)
                .cloned()
                .or_else(|| default_markdown().get(action).cloned()),
            "diff" => self
                .keybindings
                .diff
                .get(action)
                .cloned()
                .or_else(|| default_diff().get(action).cloned()),
            "workspace_list" => self
                .keybindings
                .workspace_list
                .get(action)
                .cloned()
                .or_else(|| default_workspace_list().get(action).cloned()),
            "file_list" => self
                .keybindings
                .file_list
                .get(action)
                .cloned()
                .or_else(|| default_file_list().get(action).cloned()),
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
            "commit" => self
                .keybindings
                .commit
                .get(action)
                .cloned()
                .or_else(|| default_commit().get(action).cloned()),
            "merge" => self
                .keybindings
                .merge
                .get(action)
                .cloned()
                .or_else(|| default_merge().get(action).cloned()),
            "new_tab" => self
                .keybindings
                .new_tab
                .get(action)
                .cloned()
                .or_else(|| default_new_tab().get(action).cloned()),
            _ => None,
        };
        binding.unwrap_or_else(|| "???".to_string())
    }

    fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".config/piki-multi/config.toml")
    }
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
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            // If it's an uppercase char, implicitly add SHIFT modifier
            if c.is_uppercase() {
                modifiers.insert(KeyModifiers::SHIFT);
            }
            KeyCode::Char(c)
        }
        s if s.starts_with('f') && s.len() > 1 => {
            let n = s[1..].parse::<u8>().ok()?;
            KeyCode::F(n)
        }
        _ => return None,
    };

    Some(KeyEvent::new(code, modifiers))
}

pub fn key_matches(event: KeyEvent, binding: &str) -> bool {
    if let Some(target) = parse_key_event(binding) {
        // Only compare code and modifiers.
        event.code == target.code && event.modifiers == target.modifiers
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
        // All navigation bindings should parse successfully
        for (action, binding) in &cfg.keybindings.navigation {
            assert!(
                parse_key_event(binding).is_some(),
                "navigation binding '{}' = '{}' failed to parse",
                action,
                binding,
            );
        }
    }

    #[test]
    fn test_all_default_sections_parse() {
        let cfg = Config::default();
        let sections: Vec<(&str, &HashMap<String, String>)> = vec![
            ("navigation", &cfg.keybindings.navigation),
            ("interaction", &cfg.keybindings.interaction),
            ("diff", &cfg.keybindings.diff),
            ("help", &cfg.keybindings.help),
            ("fuzzy", &cfg.keybindings.fuzzy),
            ("editor", &cfg.keybindings.editor),
            ("commit", &cfg.keybindings.commit),
            ("merge", &cfg.keybindings.merge),
            ("new_tab", &cfg.keybindings.new_tab),
            ("new_workspace", &cfg.keybindings.new_workspace),
            ("file_list", &cfg.keybindings.file_list),
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
    fn test_key_matches_ctrl_modifier() {
        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert!(key_matches(event, "ctrl-g"));
        assert!(!key_matches(event, "g"));
        assert!(!key_matches(event, "alt-g"));
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
[keybindings.navigation]
quit = "ctrl-q"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        // Overridden binding
        assert_eq!(cfg.keybindings.navigation.get("quit").unwrap(), "ctrl-q");
        // Other sections still get defaults
        assert!(cfg.keybindings.help.contains_key("exit"));
        assert!(cfg.keybindings.diff.contains_key("down"));
    }

    #[test]
    fn test_invalid_toml_fallback() {
        let result: Result<Config, _> = toml::from_str("{{invalid toml}}");
        assert!(result.is_err());
        // Config::load() would fall back to defaults — verify defaults are valid
        let cfg = Config::default();
        assert!(!cfg.keybindings.navigation.is_empty());
    }

    #[test]
    fn test_get_binding_falls_back_to_default() {
        let cfg = Config::default();
        assert_eq!(cfg.get_binding("navigation", "quit"), "q");
        assert_eq!(cfg.get_binding("navigation", "help"), "?");
        // Unknown action returns "???"
        assert_eq!(cfg.get_binding("navigation", "nonexistent"), "???");
        // Unknown section returns "???"
        assert_eq!(cfg.get_binding("nonexistent_section", "quit"), "???");
    }

    #[test]
    fn test_matches_navigation_with_defaults() {
        let cfg = Config::default();
        let q_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert!(cfg.matches_navigation(q_event, "quit"));
        assert!(!cfg.matches_navigation(q_event, "help"));
    }

    #[test]
    fn test_generate_default_toml_roundtrips() {
        let toml_str = Config::generate_default_toml();
        let cfg: Config = toml::from_str(&toml_str).unwrap();
        // Verify key bindings survived the roundtrip
        assert_eq!(cfg.keybindings.navigation.get("quit").unwrap(), "q");
        assert_eq!(cfg.keybindings.navigation.get("help").unwrap(), "?");
    }
}
