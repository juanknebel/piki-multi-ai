//! OpenRouter HTTP server client with streaming chat support.
//!
//! Talks to a OpenRouter server (default `http://localhost:8080`) using its
//! OpenAI-compatible REST API. Streaming responses use Server-Sent Events
//! and are delivered token-by-token through a `tokio::sync::mpsc` channel.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::ollama::{ChatStreamEvent, RawToolCall};

/// A chat message in OpenAI-compatible format (used by OpenRouter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    /// Tool calls from the assistant response (for multi-turn tool use).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenRouterToolCallRef>>,
    /// ID of the tool call this message responds to (role=tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Tool call reference in OpenAI format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterToolCallRef {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenRouterFunctionRef,
}

/// Function reference within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterFunctionRef {
    pub name: String,
    pub arguments: String,
}

/// A model entry returned by OpenRouter's `/v1/models` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterModel {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub owned_by: String,
}

/// HTTP client for a OpenRouter server.
pub struct OpenRouterClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl OpenRouterClient {
    pub fn new(base_url: &str) -> Self {
        Self::new_with_key(base_url, None)
    }

    pub fn new_with_key(base_url: &str, api_key: Option<String>) -> Self {
        let base = base_url.trim_end_matches('/').to_string();
        let key_len = api_key.as_ref().map(|k| k.len()).unwrap_or(0);
        tracing::debug!(base_url = %base, has_key = key_len > 0, "Creating OpenRouterClient");

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

        Self {
            client,
            base_url: base,
            api_key,
        }
    }

    /// List loaded models via `GET /models` (OpenRouter: https://openrouter.ai/api/v1/models).
    pub async fn list_models(&self) -> anyhow::Result<Vec<OpenRouterModel>> {
        let url = format!("{}/models", self.base_url);
        tracing::debug!(url = %url, "Fetching OpenRouter models");

        let mut req = self.client.get(&url);
        if let Some(key) = self.api_key.as_ref().filter(|k| !k.trim().is_empty()) {
            req = req.header("Authorization", format!("Bearer {}", key));
            req = req.header(
                "HTTP-Referer",
                "https://github.com/juanknebel/piki-multi-ai",
            );
            req = req.header("X-Title", "piki-multi-ai");
        }
        let resp = req.send().await.map_err(|e| {
            tracing::error!(url = %url, error = %e, "Failed to connect to OpenRouter server");
            anyhow::anyhow!("Cannot connect to OpenRouter at {}: {e}", self.base_url)
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            tracing::error!(url = %url, status = %status, "OpenRouter API returned error status");
            anyhow::bail!("OpenRouter API returned status {} from {}", status, url);
        }

        let body: ModelsResponse = resp.json().await?;
        tracing::info!(count = body.data.len(), "Loaded OpenRouter models");
        Ok(body.data)
    }

    /// Send a chat completion request and stream the response token-by-token.
    ///
    /// Uses the OpenAI-compatible `/v1/chat/completions` endpoint with SSE
    /// streaming. Each token is sent as `ChatStreamEvent::Token`. When the
    /// response is complete, `ChatStreamEvent::Done` is sent with the full
    /// content.
    pub async fn chat_stream(
        &self,
        model: &str,
        messages: &[OpenRouterMessage],
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        self.chat_stream_with_tools(model, messages, None, tx).await
    }

    /// Send a chat completion request with optional tool definitions and stream the response.
    pub async fn chat_stream_with_tools(
        &self,
        model: &str,
        messages: &[OpenRouterMessage],
        tools: Option<&[serde_json::Value]>,
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        let url = format!("{}/chat/completions", self.base_url);
        tracing::info!(
            model,
            msg_count = messages.len(),
            has_tools = tools.is_some(),
            "Starting OpenRouter chat stream"
        );

        let payload = ChatCompletionRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
            tools: tools.map(|t| t.to_vec()),
            tool_choice: tools.map(|_| "auto".to_string()),
        };

        let mut req = self.client.post(&url).json(&payload);
        if let Some(key) = self.api_key.as_ref().filter(|k| !k.trim().is_empty()) {
            req = req.header("Authorization", format!("Bearer {}", key));
            req = req.header(
                "HTTP-Referer",
                "https://github.com/juanknebel/piki-multi-ai",
            );
            req = req.header("X-Title", "piki-multi-ai");
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(url = %url, error = %e, "Failed to send chat request to OpenRouter");
                let _ = tx.send(ChatStreamEvent::Error(format!(
                    "Cannot connect to OpenRouter at {}: {e}",
                    self.base_url
                )));
                return Err(e.into());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "OpenRouter chat API returned error");
            let msg = format!("OpenRouter API error {status}: {body}");
            let _ = tx.send(ChatStreamEvent::Error(msg.clone()));
            anyhow::bail!(msg);
        }

        tracing::debug!("OpenRouter chat stream connected, reading SSE events");
        let mut stream = resp.bytes_stream();
        let mut full_content = String::new();
        let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "Error reading OpenRouter stream chunk");
                    let _ = tx.send(ChatStreamEvent::Error(e.to_string()));
                    return Err(e.into());
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE lines: "data: {...}" or "data: [DONE]"
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                let Some(data) = line.strip_prefix("data: ") else {
                    // Skip non-data lines (e.g. "event:" or comments)
                    continue;
                };
                let data = data.trim();

                if data == "[DONE]" {
                    let _ = tx.send(ChatStreamEvent::Done(full_content));
                    return Ok(());
                }

                match serde_json::from_str::<ChatCompletionChunk>(data) {
                    Ok(parsed) => {
                        for choice in &parsed.choices {
                            // Accumulate streamed tool call fragments
                            if let Some(ref tcs) = choice.delta.tool_calls {
                                for tc in tcs {
                                    let idx = tc.index;
                                    // Grow the accumulator if needed
                                    while pending_tool_calls.len() <= idx {
                                        pending_tool_calls.push(PendingToolCall::default());
                                    }
                                    if let Some(ref id) = tc.id {
                                        pending_tool_calls[idx].id.clone_from(id);
                                    }
                                    if let Some(ref f) = tc.function {
                                        if let Some(ref name) = f.name {
                                            pending_tool_calls[idx].name.clone_from(name);
                                        }
                                        if let Some(ref args) = f.arguments {
                                            pending_tool_calls[idx].arguments.push_str(args);
                                        }
                                    }
                                }
                            }

                            if let Some(ref content) = choice.delta.content
                                && !content.is_empty()
                            {
                                full_content.push_str(content);
                                let _ = tx.send(ChatStreamEvent::Token(content.clone()));
                            }

                            if let Some(ref reason) = choice.finish_reason {
                                if reason == "tool_calls" && !pending_tool_calls.is_empty() {
                                    let raw_calls: Vec<RawToolCall> = pending_tool_calls
                                        .drain(..)
                                        .enumerate()
                                        .map(|(i, ptc)| RawToolCall {
                                            id: if ptc.id.is_empty() {
                                                format!("call_{i}")
                                            } else {
                                                ptc.id
                                            },
                                            name: ptc.name,
                                            arguments: ptc.arguments,
                                        })
                                        .collect();
                                    tracing::info!(
                                        count = raw_calls.len(),
                                        "OpenRouter returned tool calls"
                                    );
                                    let _ = tx.send(ChatStreamEvent::ToolCalls(raw_calls));
                                    return Ok(());
                                }
                                if reason == "stop" {
                                    let _ = tx.send(ChatStreamEvent::Done(full_content));
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(data, error = %e, "Failed to parse OpenRouter SSE chunk");
                    }
                }
            }
        }

        // Stream ended without explicit [DONE]
        if !full_content.is_empty() {
            tracing::debug!(
                chars = full_content.len(),
                "OpenRouter stream ended without [DONE], sending accumulated content"
            );
            let _ = tx.send(ChatStreamEvent::Done(full_content));
        } else {
            tracing::warn!("OpenRouter stream ended with no content");
            let _ = tx.send(ChatStreamEvent::Error(
                "Stream ended unexpectedly".to_string(),
            ));
        }

        Ok(())
    }
}

// ── OpenAI-compatible API types (private, for serde) ─────

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ChunkDeltaToolCall>>,
}

/// Streamed tool call fragment in OpenAI format.
#[derive(Deserialize)]
struct ChunkDeltaToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ChunkDeltaFunction>,
}

#[derive(Deserialize)]
struct ChunkDeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// Accumulator for streamed tool call fragments.
#[derive(Default)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<OpenRouterModel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_chunk() {
        let json = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let parsed: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(parsed.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_parse_sse_done_chunk() {
        let json = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let parsed: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_parse_models_response() {
        let json = r#"{"object":"list","data":[{"id":"my-model","object":"model","owned_by":"llamacpp"}]}"#;
        let parsed: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.data[0].id, "my-model");
    }

    #[test]
    fn test_parse_models_empty() {
        let json = r#"{"object":"list","data":[]}"#;
        let parsed: ModelsResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn test_llamacpp_client_url_normalization() {
        let client = OpenRouterClient::new("http://localhost:8080/");
        assert_eq!(client.base_url, "http://localhost:8080");

        let client2 = OpenRouterClient::new("http://localhost:8080");
        assert_eq!(client2.base_url, "http://localhost:8080");
    }
}
