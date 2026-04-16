//! Ollama HTTP API client with streaming chat support.
//!
//! Talks to the Ollama server (default `http://localhost:11434`) using its
//! REST API. Streaming responses are delivered token-by-token through a
//! `tokio::sync::mpsc` channel.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// A chat message in Ollama's API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    /// Tool calls returned by the assistant (for multi-turn tool use).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCallRef>>,
}

/// Reference to a tool call, used when sending tool-call messages back to Ollama.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCallRef {
    pub function: OllamaFunctionRef,
}

/// Function reference within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunctionRef {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A raw tool call parsed from an LLM response (provider-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawToolCall {
    pub id: String,
    pub name: String,
    /// JSON string of the arguments.
    pub arguments: String,
}

/// Events emitted during a streaming chat response.
#[derive(Debug, Clone)]
pub enum ChatStreamEvent {
    /// A partial content token.
    Token(String),
    /// Streaming is complete; carries the full assembled response.
    Done(String),
    /// The LLM requested tool calls instead of (or in addition to) content.
    ToolCalls(Vec<RawToolCall>),
    /// An error occurred during streaming.
    Error(String),
}

/// An installed Ollama model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
}

/// HTTP client for the Ollama API.
pub struct OllamaClient {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/').to_string();
        tracing::debug!(base_url = %base, "Creating OllamaClient");

        // Ollama typically runs on localhost over plain HTTP.
        // Try without root certs first (avoids TLS init failures on minimal
        // systems), then fall back to the default builder.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .tls_built_in_root_certs(false)
            .build()
            .or_else(|e| {
                tracing::warn!(error = %e, "reqwest builder failed without root certs, retrying with defaults");
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()
            })
            .inspect_err(|e| {
                tracing::error!(error = %e, "reqwest client builder failed completely");
            })
            .unwrap_or_default();

        Self { client, base_url: base }
    }

    /// List installed models via `GET /api/tags`.
    pub async fn list_models(&self) -> anyhow::Result<Vec<OllamaModel>> {
        let url = format!("{}/api/tags", self.base_url);
        tracing::debug!(url = %url, "Fetching Ollama models");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(url = %url, error = %e, "Failed to connect to Ollama");
                anyhow::anyhow!("Cannot connect to Ollama at {}: {e}", self.base_url)
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            tracing::error!(url = %url, status = %status, "Ollama API returned error status");
            anyhow::bail!("Ollama API returned status {} from {}", status, url);
        }

        let body: TagsResponse = resp.json().await?;
        tracing::info!(count = body.models.len(), "Loaded Ollama models");
        Ok(body.models)
    }

    /// Send a chat request and stream the response token-by-token.
    ///
    /// Each token is sent as `ChatStreamEvent::Token`. When the response is
    /// complete, `ChatStreamEvent::Done` is sent with the full content.
    /// Errors are sent as `ChatStreamEvent::Error`.
    pub async fn chat_stream(
        &self,
        model: &str,
        messages: &[OllamaMessage],
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        self.chat_stream_with_tools(model, messages, None, tx).await
    }

    /// Send a chat request with optional tool definitions and stream the response.
    pub async fn chat_stream_with_tools(
        &self,
        model: &str,
        messages: &[OllamaMessage],
        tools: Option<&[serde_json::Value]>,
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        let url = format!("{}/api/chat", self.base_url);
        tracing::info!(model, msg_count = messages.len(), has_tools = tools.is_some(), "Starting Ollama chat stream");

        let payload = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
            tools: tools.map(|t| t.to_vec()),
        };

        let resp = match self.client.post(&url).json(&payload).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(url = %url, error = %e, "Failed to send chat request to Ollama");
                let _ = tx.send(ChatStreamEvent::Error(format!(
                    "Cannot connect to Ollama at {}: {e}", self.base_url
                )));
                return Err(e.into());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "Ollama chat API returned error");
            let msg = format!("Ollama API error {status}: {body}");
            let _ = tx.send(ChatStreamEvent::Error(msg.clone()));
            anyhow::bail!(msg);
        }

        tracing::debug!("Ollama chat stream connected, reading tokens");
        let mut stream = resp.bytes_stream();
        let mut full_content = String::new();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "Error reading Ollama stream chunk");
                    let _ = tx.send(ChatStreamEvent::Error(e.to_string()));
                    return Err(e.into());
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines (newline-delimited JSON)
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<ChatStreamResponse>(&line) {
                    Ok(parsed) => {
                        // Check for tool calls in the response
                        if let Some(ref tool_calls) = parsed.message.tool_calls
                            && !tool_calls.is_empty()
                        {
                            let raw_calls: Vec<RawToolCall> = tool_calls
                                .iter()
                                .enumerate()
                                .map(|(i, tc)| RawToolCall {
                                    id: format!("call_{i}"),
                                    name: tc.function.name.clone(),
                                    arguments: serde_json::to_string(&tc.function.arguments)
                                        .unwrap_or_default(),
                                })
                                .collect();
                            tracing::info!(count = raw_calls.len(), "Ollama returned tool calls");
                            let _ = tx.send(ChatStreamEvent::ToolCalls(raw_calls));
                            return Ok(());
                        }

                        let token = parsed.message.content;
                        if !token.is_empty() {
                            full_content.push_str(&token);
                            let _ = tx.send(ChatStreamEvent::Token(token));
                        }
                        if parsed.done {
                            let _ = tx.send(ChatStreamEvent::Done(full_content));
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        tracing::warn!(line, error = %e, "Failed to parse Ollama stream line");
                    }
                }
            }
        }

        // Stream ended without a done=true — send what we have
        if !full_content.is_empty() {
            tracing::debug!(chars = full_content.len(), "Ollama stream ended (no done flag), sending accumulated content");
            let _ = tx.send(ChatStreamEvent::Done(full_content));
        } else {
            tracing::warn!("Ollama stream ended with no content and no done flag");
            let _ = tx.send(ChatStreamEvent::Error(
                "Stream ended unexpectedly".to_string(),
            ));
        }

        Ok(())
    }

    /// Send a non-streaming chat request and return the full response.
    pub async fn chat(
        &self,
        model: &str,
        messages: &[OllamaMessage],
    ) -> anyhow::Result<String> {
        let url = format!("{}/api/chat", self.base_url);
        tracing::info!(model, msg_count = messages.len(), "Sending non-streaming Ollama chat request");

        let payload = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: false,
            tools: None,
        };

        let resp = self.client.post(&url).json(&payload).send().await
            .map_err(|e| {
                tracing::error!(url = %url, error = %e, "Failed to send chat request");
                e
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "Ollama chat API returned error");
            anyhow::bail!("Ollama API error {status}: {body}");
        }

        let parsed: ChatStreamResponse = resp.json().await?;
        tracing::debug!(chars = parsed.message.content.len(), "Ollama chat response received");
        Ok(parsed.message.content)
    }
}

// ── Ollama API types (private, for serde) ──────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct ChatStreamResponse {
    message: ChatStreamResponseMessage,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct ChatStreamResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCallResponse>>,
}

/// Tool call as returned by Ollama in the streaming response.
#[derive(Deserialize)]
struct OllamaToolCallResponse {
    function: OllamaFunctionResponse,
}

#[derive(Deserialize)]
struct OllamaFunctionResponse {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_response() {
        let json = r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let parsed: ChatStreamResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.message.content, "Hello");
        assert!(!parsed.done);
    }

    #[test]
    fn test_parse_stream_done() {
        let json = r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"total_duration":123456}"#;
        let parsed: ChatStreamResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.done);
        assert_eq!(parsed.message.content, "");
    }

    #[test]
    fn test_parse_tags_response() {
        let json = r#"{"models":[{"name":"llama3.2:latest","size":2000000000,"modified_at":"2025-01-01T00:00:00Z"}]}"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].name, "llama3.2:latest");
    }

    #[test]
    fn test_parse_tags_empty() {
        let json = r#"{"models":[]}"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.models.is_empty());
    }

    #[test]
    fn test_ollama_client_url_normalization() {
        let client = OllamaClient::new("http://localhost:11434/");
        assert_eq!(client.base_url, "http://localhost:11434");

        let client2 = OllamaClient::new("http://localhost:11434");
        assert_eq!(client2.base_url, "http://localhost:11434");
    }
}
