use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::chat::ChatServerType;

/// A single chat provider configuration (LLM backend).
/// Stored in `chat-providers.toml` estilo `providers.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatProviderConfig {
    /// Unique name id, e.g. "ollama", "llama.cpp", "openrouter"
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub server_type: ChatServerType,
    /// Base URL for the API (e.g. http://localhost:11434 or https://openrouter.ai/api/v1)
    pub base_url: String,
    /// Default model for this provider (e.g. "llama3.2" or "openai/gpt-4o")
    #[serde(default)]
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatProvidersFile {
    #[serde(default)]
    chat_providers: Vec<ChatProviderConfig>,
}

#[derive(Debug, Clone)]
pub struct ChatProviderManager {
    providers: Vec<ChatProviderConfig>,
}

impl ChatProviderManager {
    pub fn empty() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn load_or_init(path: &Path) -> Self {
        if let Ok(contents) = std::fs::read_to_string(path) {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                // Try new key chat_providers first, then fallback to [[chat_provider]] or providers
                if let Ok(file) = toml::from_str::<ChatProvidersFile>(trimmed)
                    && !file.chat_providers.is_empty()
                {
                    return Self {
                        providers: file.chat_providers,
                    };
                }
                // fallback: try providers key (old)
                #[derive(Deserialize)]
                struct Alt {
                    providers: Vec<ChatProviderConfig>,
                }
                if let Ok(alt) = toml::from_str::<Alt>(trimmed)
                    && !alt.providers.is_empty()
                {
                    return Self {
                        providers: alt.providers,
                    };
                }
            }
        }
        let defaults = Self::default_providers();
        let m = Self {
            providers: defaults,
        };
        let _ = m.save(path);
        m
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = ChatProvidersFile {
            chat_providers: self.providers.clone(),
        };
        let s = toml::to_string_pretty(&file)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    pub fn all(&self) -> &[ChatProviderConfig] {
        &self.providers
    }
    pub fn get(&self, name: &str) -> Option<&ChatProviderConfig> {
        self.providers.iter().find(|p| p.name == name)
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ChatProviderConfig> {
        self.providers.iter_mut().find(|p| p.name == name)
    }
    pub fn upsert(&mut self, cfg: ChatProviderConfig) {
        if let Some(e) = self.providers.iter_mut().find(|p| p.name == cfg.name) {
            *e = cfg;
        } else {
            self.providers.push(cfg);
        }
    }
    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.providers.len();
        self.providers.retain(|p| p.name != name);
        self.providers.len() < n
    }

    fn default_providers() -> Vec<ChatProviderConfig> {
        vec![
            ChatProviderConfig {
                name: "ollama".to_string(),
                description: "Ollama local".to_string(),
                server_type: ChatServerType::Ollama,
                base_url: ChatServerType::Ollama.default_url().to_string(),
                model: String::new(),
                system_prompt: None,
            },
            ChatProviderConfig {
                name: "llama.cpp".to_string(),
                description: "llama.cpp server".to_string(),
                server_type: ChatServerType::LlamaCpp,
                base_url: ChatServerType::LlamaCpp.default_url().to_string(),
                model: String::new(),
                system_prompt: None,
            },
            ChatProviderConfig {
                name: "openrouter".to_string(),
                description: "OpenRouter cloud".to_string(),
                server_type: ChatServerType::OpenRouter,
                base_url: ChatServerType::OpenRouter.default_url().to_string(),
                model: String::new(),
                system_prompt: None,
            },
        ]
    }
}
