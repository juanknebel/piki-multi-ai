use std::path::Path;

use serde::{Deserialize, Serialize};

/// How a provider accepts prompt text on the command line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value")]
pub enum PromptFormat {
    /// Prompt is passed as a bare positional argument: `<command> "<prompt>"`
    Positional,
    /// Prompt is passed via a flag: `<command> --flag "<prompt>"`
    Flag(String),
    /// Provider does not accept prompts via CLI
    None,
}

/// A single provider configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Unique identifier (used as the AIProvider::Custom key and tab label)
    pub name: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Path or name of the binary to execute (resolved via $PATH)
    pub command: String,
    /// Default arguments always passed to the binary (before prompt args)
    #[serde(default)]
    pub default_args: Vec<String>,
    /// How this provider receives a prompt on the CLI
    #[serde(default = "default_prompt_format")]
    pub prompt_format: PromptFormat,
    /// Whether this provider can be dispatched as an agent
    #[serde(default)]
    pub dispatchable: bool,
    /// Subdirectory under the repo root for agent config files (e.g. ".claude/agents").
    /// When set, agent profiles are synced/scanned from `<repo>/<agent_dir>/`.
    #[serde(default)]
    pub agent_dir: Option<String>,
}

fn default_prompt_format() -> PromptFormat {
    PromptFormat::Positional
}

/// Top-level TOML structure for `providers.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProvidersFile {
    #[serde(default)]
    providers: Vec<ProviderConfig>,
}

/// Manages user-configurable providers loaded from `providers.toml`.
#[derive(Debug, Clone)]
pub struct ProviderManager {
    providers: Vec<ProviderConfig>,
}

impl ProviderManager {
    /// Load providers from a TOML file. If the file is missing or empty,
    /// bootstrap it with a default Claude provider entry and return that.
    pub fn load_or_init(path: &Path) -> Self {
        // Try to read existing file
        if let Ok(contents) = std::fs::read_to_string(path) {
            let trimmed = contents.trim();
            if !trimmed.is_empty()
                && let Ok(file) = toml::from_str::<ProvidersFile>(trimmed)
                && !file.providers.is_empty()
            {
                return Self {
                    providers: file.providers,
                };
            }
        }

        // File missing, empty, or has no providers — bootstrap with defaults
        let defaults = Self::default_providers();
        let manager = Self {
            providers: defaults,
        };
        // Best-effort write
        let _ = manager.save(path);
        manager
    }

    /// Save current providers to a TOML file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = ProvidersFile {
            providers: self.providers.clone(),
        };
        let contents = toml::to_string_pretty(&file)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// All configured providers.
    pub fn all(&self) -> &[ProviderConfig] {
        &self.providers
    }

    /// Look up a provider by name (case-sensitive).
    pub fn get(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.name == name)
    }

    /// Providers that can be dispatched as agents.
    pub fn dispatchable(&self) -> Vec<&ProviderConfig> {
        self.providers.iter().filter(|p| p.dispatchable).collect()
    }

    /// Add or update a provider. If a provider with the same name exists, it is replaced.
    pub fn upsert(&mut self, config: ProviderConfig) {
        if let Some(existing) = self.providers.iter_mut().find(|p| p.name == config.name) {
            *existing = config;
        } else {
            self.providers.push(config);
        }
    }

    /// Remove a provider by name. Returns true if it was found and removed.
    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.providers.len();
        self.providers.retain(|p| p.name != name);
        self.providers.len() < len_before
    }

    /// Build CLI arguments for passing a prompt to a provider.
    pub fn prompt_args(config: &ProviderConfig, prompt: &str) -> Vec<String> {
        if prompt.is_empty() {
            return Vec::new();
        }
        match &config.prompt_format {
            PromptFormat::Positional => vec![prompt.to_string()],
            PromptFormat::Flag(flag) => vec![flag.clone(), prompt.to_string()],
            PromptFormat::None => Vec::new(),
        }
    }

    /// Default provider configurations matching the built-in AIProvider variants.
    fn default_providers() -> Vec<ProviderConfig> {
        vec![ProviderConfig {
            name: "Claude Code".to_string(),
            description: "Anthropic's Claude Code CLI agent".to_string(),
            command: "claude".to_string(),
            default_args: Vec::new(),
            prompt_format: PromptFormat::Positional,
            dispatchable: true,
            agent_dir: Some(".claude/agents".to_string()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_providers() {
        let defaults = ProviderManager::default_providers();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "Claude Code");
        assert_eq!(defaults[0].command, "claude");
        assert!(defaults[0].dispatchable);
    }

    #[test]
    fn test_prompt_args_positional() {
        let config = ProviderConfig {
            name: "Test".into(),
            description: String::new(),
            command: "test".into(),
            default_args: Vec::new(),
            prompt_format: PromptFormat::Positional,
            dispatchable: false,
            agent_dir: None,
        };
        let args = ProviderManager::prompt_args(&config, "hello");
        assert_eq!(args, vec!["hello"]);
    }

    #[test]
    fn test_prompt_args_flag() {
        let config = ProviderConfig {
            name: "Test".into(),
            description: String::new(),
            command: "test".into(),
            default_args: Vec::new(),
            prompt_format: PromptFormat::Flag("--prompt".into()),
            dispatchable: false,
            agent_dir: None,
        };
        let args = ProviderManager::prompt_args(&config, "hello");
        assert_eq!(args, vec!["--prompt", "hello"]);
    }

    #[test]
    fn test_prompt_args_none() {
        let config = ProviderConfig {
            name: "Test".into(),
            description: String::new(),
            command: "test".into(),
            default_args: Vec::new(),
            prompt_format: PromptFormat::None,
            dispatchable: false,
            agent_dir: None,
        };
        let args = ProviderManager::prompt_args(&config, "hello");
        assert!(args.is_empty());
    }

    #[test]
    fn test_roundtrip_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");

        let manager = ProviderManager {
            providers: vec![
                ProviderConfig {
                    name: "My AI".into(),
                    description: "Custom AI tool".into(),
                    command: "/usr/bin/my-ai".into(),
                    default_args: vec!["--json".into()],
                    prompt_format: PromptFormat::Flag("--task".into()),
                    dispatchable: true,
                    agent_dir: Some(".my-ai/agents".into()),
                },
            ],
        };
        manager.save(&path).unwrap();

        let loaded = ProviderManager::load_or_init(&path);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].name, "My AI");
        assert_eq!(loaded.providers[0].command, "/usr/bin/my-ai");
        assert_eq!(loaded.providers[0].default_args, vec!["--json"]);
        assert_eq!(
            loaded.providers[0].prompt_format,
            PromptFormat::Flag("--task".into())
        );
        assert!(loaded.providers[0].dispatchable);
        assert_eq!(
            loaded.providers[0].agent_dir,
            Some(".my-ai/agents".into())
        );
    }

    #[test]
    fn test_load_or_init_creates_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");

        // File doesn't exist — should bootstrap
        let manager = ProviderManager::load_or_init(&path);
        assert_eq!(manager.providers.len(), 1);
        assert_eq!(manager.providers[0].name, "Claude Code");

        // File should now exist on disk
        assert!(path.exists());
    }

    #[test]
    fn test_load_or_init_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");
        std::fs::write(&path, "").unwrap();

        let manager = ProviderManager::load_or_init(&path);
        assert_eq!(manager.providers.len(), 1);
        assert_eq!(manager.providers[0].name, "Claude Code");
    }

    #[test]
    fn test_get_and_dispatchable() {
        let manager = ProviderManager {
            providers: vec![
                ProviderConfig {
                    name: "A".into(),
                    description: String::new(),
                    command: "a".into(),
                    default_args: Vec::new(),
                    prompt_format: PromptFormat::Positional,
                    dispatchable: true,
                    agent_dir: None,
                },
                ProviderConfig {
                    name: "B".into(),
                    description: String::new(),
                    command: "b".into(),
                    default_args: Vec::new(),
                    prompt_format: PromptFormat::None,
                    dispatchable: false,
                    agent_dir: None,
                },
            ],
        };
        assert!(manager.get("A").is_some());
        assert!(manager.get("C").is_none());
        assert_eq!(manager.dispatchable().len(), 1);
        assert_eq!(manager.dispatchable()[0].name, "A");
    }
}
