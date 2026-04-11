use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

/// Configuration for the global AI chat panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Provider identifier (e.g. "ollama").
    pub provider: String,
    /// Model name (e.g. "llama3.2").
    pub model: String,
    /// Base URL for the provider API.
    pub base_url: String,
    /// Optional system prompt prepended to every conversation.
    pub system_prompt: Option<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: String::new(),
            base_url: "http://localhost:11434".to_string(),
            system_prompt: None,
        }
    }
}
