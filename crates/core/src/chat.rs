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

/// Which LLM server backend to use for chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChatServerType {
    #[default]
    Ollama,
    LlamaCpp,
}

impl ChatServerType {
    /// Default base URL for this server type.
    pub fn default_url(self) -> &'static str {
        match self {
            Self::Ollama => "http://localhost:11434",
            Self::LlamaCpp => "http://localhost:8080",
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::LlamaCpp => "llama.cpp",
        }
    }

    /// Cycle to the next server type.
    pub fn next(self) -> Self {
        match self {
            Self::Ollama => Self::LlamaCpp,
            Self::LlamaCpp => Self::Ollama,
        }
    }
}

/// Configuration for the global AI chat panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Provider identifier (e.g. "ollama").
    pub provider: String,
    /// Which server backend to use.
    #[serde(default)]
    pub server_type: ChatServerType,
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
            server_type: ChatServerType::default(),
            model: String::new(),
            base_url: "http://localhost:11434".to_string(),
            system_prompt: None,
        }
    }
}
