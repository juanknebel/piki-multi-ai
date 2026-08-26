use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatRole {
    /// The role string every OpenAI-shaped chat API expects. Was written out
    /// by hand at three call sites before this existed.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    /// Tool calls requested by the assistant (present when role == Assistant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID of the tool call this message is a response to (present when role == Tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Schema definition for a tool the LLM can invoke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters: serde_json::Value,
}

/// A tool invocation requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The result of executing a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Which LLM server backend to use for chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChatServerType {
    #[default]
    Ollama,
    LlamaCpp,
    OpenRouter,
}

impl ChatServerType {
    /// Default base URL for this server type.
    pub fn default_url(self) -> &'static str {
        match self {
            Self::Ollama => "http://localhost:11434",
            Self::LlamaCpp => "http://localhost:8080",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::LlamaCpp => "llama.cpp",
            Self::OpenRouter => "OpenRouter",
        }
    }

    /// Cycle to the next server type.
    pub fn next(self) -> Self {
        match self {
            Self::Ollama => Self::LlamaCpp,
            Self::LlamaCpp => Self::OpenRouter,
            Self::OpenRouter => Self::Ollama,
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
    /// API key for remote providers (OpenRouter). Not logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            server_type: ChatServerType::default(),
            model: String::new(),
            base_url: "http://localhost:11434".to_string(),
            system_prompt: None,
            api_key: None,
        }
    }
}

impl ChatConfig {
    /// Effective API key, preferring stored key, then OPENROUTER_API_KEY env var (piki-ai parity).
    /// Kept for backwards compat; new code should use `effective_api_key_with_paths`.
    pub fn effective_api_key(&self) -> Option<String> {
        if let Some(k) = self.api_key.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(k.clone());
        }
        if let Some(k) = Self::key_from_config_file(&crate::paths::DataPaths::default_paths()) {
            return Some(k);
        }
        std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    /// Like `effective_api_key` but reads `config.toml` from the given `DataPaths`
    /// (so `--data-dir` isolation is honoured). Priority: stored `api_key` > `config.toml`
    /// `[chat] openrouter_api_key` > `OPENROUTER_API_KEY` env.
    pub fn effective_api_key_with_paths(&self, paths: &crate::paths::DataPaths) -> Option<String> {
        if let Some(k) = self.api_key.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(k.clone());
        }
        if let Some(k) = Self::key_from_config_file(paths) {
            return Some(k);
        }
        std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    fn key_from_config_file(paths: &crate::paths::DataPaths) -> Option<String> {
        let data = std::fs::read_to_string(paths.config_path()).ok()?;
        let val: toml::Value = toml::from_str(&data).ok()?;
        let key = val.get("chat")?.get("openrouter_api_key")?.as_str()?;
        let k = key.trim();
        if k.is_empty() {
            None
        } else {
            Some(k.to_string())
        }
    }
}
