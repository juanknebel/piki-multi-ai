//! llama.cpp HTTP server client with streaming chat support.
//!
//! Talks to a llama.cpp server (default `http://localhost:8080`) using its
//! OpenAI-compatible REST API. Streaming responses use Server-Sent Events
//! and are delivered token-by-token through a `tokio::sync::mpsc` channel.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::ollama::ChatStreamEvent;

/// A chat message in OpenAI-compatible format (used by llama.cpp).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaCppMessage {
    pub role: String,
    pub content: String,
}

/// A model entry returned by llama.cpp's `/v1/models` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaCppModel {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub owned_by: String,
}

/// HTTP client for a llama.cpp server.
pub struct LlamaCppClient {
    client: reqwest::Client,
    base_url: String,
}

impl LlamaCppClient {
    pub fn new(base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/').to_string();
        tracing::debug!(base_url = %base, "Creating LlamaCppClient");

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

    /// List loaded models via `GET /v1/models`.
    pub async fn list_models(&self) -> anyhow::Result<Vec<LlamaCppModel>> {
        let url = format!("{}/v1/models", self.base_url);
        tracing::debug!(url = %url, "Fetching llama.cpp models");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(url = %url, error = %e, "Failed to connect to llama.cpp server");
                anyhow::anyhow!("Cannot connect to llama.cpp at {}: {e}", self.base_url)
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            tracing::error!(url = %url, status = %status, "llama.cpp API returned error status");
            anyhow::bail!("llama.cpp API returned status {} from {}", status, url);
        }

        let body: ModelsResponse = resp.json().await?;
        tracing::info!(count = body.data.len(), "Loaded llama.cpp models");
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
        messages: &[LlamaCppMessage],
        tx: mpsc::UnboundedSender<ChatStreamEvent>,
    ) -> anyhow::Result<()> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        tracing::info!(model, msg_count = messages.len(), "Starting llama.cpp chat stream");

        let payload = ChatCompletionRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
        };

        let resp = match self.client.post(&url).json(&payload).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(url = %url, error = %e, "Failed to send chat request to llama.cpp");
                let _ = tx.send(ChatStreamEvent::Error(format!(
                    "Cannot connect to llama.cpp at {}: {e}", self.base_url
                )));
                return Err(e.into());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %body, "llama.cpp chat API returned error");
            let msg = format!("llama.cpp API error {status}: {body}");
            let _ = tx.send(ChatStreamEvent::Error(msg.clone()));
            anyhow::bail!(msg);
        }

        tracing::debug!("llama.cpp chat stream connected, reading SSE events");
        let mut stream = resp.bytes_stream();
        let mut full_content = String::new();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "Error reading llama.cpp stream chunk");
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
                            if let Some(ref content) = choice.delta.content
                                && !content.is_empty()
                            {
                                full_content.push_str(content);
                                let _ = tx.send(ChatStreamEvent::Token(content.clone()));
                            }
                            if let Some(ref reason) = choice.finish_reason
                                && reason == "stop"
                            {
                                let _ = tx.send(ChatStreamEvent::Done(full_content));
                                return Ok(());
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(data, error = %e, "Failed to parse llama.cpp SSE chunk");
                    }
                }
            }
        }

        // Stream ended without explicit [DONE]
        if !full_content.is_empty() {
            tracing::debug!(chars = full_content.len(), "llama.cpp stream ended without [DONE], sending accumulated content");
            let _ = tx.send(ChatStreamEvent::Done(full_content));
        } else {
            tracing::warn!("llama.cpp stream ended with no content");
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
    messages: Vec<LlamaCppMessage>,
    stream: bool,
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
}

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<LlamaCppModel>,
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
        let client = LlamaCppClient::new("http://localhost:8080/");
        assert_eq!(client.base_url, "http://localhost:8080");

        let client2 = LlamaCppClient::new("http://localhost:8080");
        assert_eq!(client2.base_url, "http://localhost:8080");
    }
}
