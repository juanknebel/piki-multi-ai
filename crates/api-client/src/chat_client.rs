//! Unified trait for LLM chat clients with optional tool-use support.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::ollama::{ChatStreamEvent, RawToolCall};

/// Provider-agnostic chat message for the wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatWireMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<RawToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Abstraction over Ollama and llama.cpp chat clients.
#[async_trait::async_trait]
pub trait ChatClient: Send + Sync {
    /// Stream a chat completion, optionally with tool definitions.
    async fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatWireMessage],
        tools: Option<&[serde_json::Value]>,
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()>;
}

// ── Implementations ──────────────────────────────────────────

#[async_trait::async_trait]
impl ChatClient for crate::ollama::OllamaClient {
    async fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatWireMessage],
        tools: Option<&[serde_json::Value]>,
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        let msgs: Vec<crate::ollama::OllamaMessage> = messages
            .iter()
            .map(|m| {
                let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| crate::ollama::OllamaToolCallRef {
                            function: crate::ollama::OllamaFunctionRef {
                                name: tc.name.clone(),
                                arguments: serde_json::from_str(&tc.arguments)
                                    .unwrap_or(serde_json::Value::Object(Default::default())),
                            },
                        })
                        .collect()
                });
                crate::ollama::OllamaMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    tool_calls,
                }
            })
            .collect();
        self.chat_stream_with_tools(model, &msgs, tools, tx).await
    }
}

#[async_trait::async_trait]
impl ChatClient for crate::llamacpp::LlamaCppClient {
    async fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatWireMessage],
        tools: Option<&[serde_json::Value]>,
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        let msgs: Vec<crate::llamacpp::LlamaCppMessage> = messages
            .iter()
            .map(|m| {
                let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| crate::llamacpp::LlamaCppToolCallRef {
                            id: tc.id.clone(),
                            call_type: "function".to_string(),
                            function: crate::llamacpp::LlamaCppFunctionRef {
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            },
                        })
                        .collect()
                });
                crate::llamacpp::LlamaCppMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    tool_calls,
                    tool_call_id: m.tool_call_id.clone(),
                }
            })
            .collect();
        self.chat_stream_with_tools(model, &msgs, tools, tx).await
    }
}
