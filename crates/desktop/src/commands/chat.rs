use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use piki_core::chat::{ChatConfig, ChatMessage, ChatRole};

use crate::state::DesktopApp;

// ── Serialized payloads ──────────────────────────────────

#[derive(Clone, Serialize)]
struct ChatTokenPayload {
    content: String,
    done: bool,
}

#[derive(Serialize, Clone)]
pub struct ChatModelInfo {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

// ── Commands ─────────────────────────────────────────────

/// Send a user message and stream the assistant response.
///
/// Appends the user message to history, then spawns a background task that
/// calls the Ollama API with streaming. Tokens are emitted as `"chat-token"`
/// Tauri events. When complete, the full assistant message is appended to
/// the in-memory history.
#[tauri::command]
pub async fn chat_send_message(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    message: String,
) -> Result<(), String> {
    let (config, messages) = {
        let mut app = state.lock();

        if app.chat_streaming {
            return Err("A response is already being streamed".to_string());
        }

        // Append the user message
        app.chat_messages.push(ChatMessage {
            role: ChatRole::User,
            content: message,
        });
        app.chat_streaming = true;

        (app.chat_config.clone(), app.chat_messages.clone())
    };

    if config.model.is_empty() {
        tracing::warn!("Chat send attempted with no model selected");
        let mut app = state.lock();
        app.chat_streaming = false;
        return Err("No model selected. Configure a model in the chat panel settings.".to_string());
    }

    tracing::info!(
        model = %config.model,
        base_url = %config.base_url,
        msg_count = messages.len(),
        "Sending chat message"
    );

    // Convert to Ollama message format
    let mut ollama_msgs: Vec<piki_api_client::OllamaMessage> = Vec::new();

    // Prepend system prompt if configured
    if let Some(ref sys) = config.system_prompt
        && !sys.is_empty()
    {
        ollama_msgs.push(piki_api_client::OllamaMessage {
            role: "system".to_string(),
            content: sys.clone(),
        });
    }

    for msg in &messages {
        ollama_msgs.push(piki_api_client::OllamaMessage {
            role: match msg.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
            }
            .to_string(),
            content: msg.content.clone(),
        });
    }

    let client = piki_api_client::OllamaClient::new(&config.base_url);
    let model = config.model.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Spawn the streaming request
    tokio::spawn(async move {
        if let Err(e) = client.chat_stream(&model, &ollama_msgs, tx).await {
            tracing::error!(error = %e, "Ollama chat_stream failed");
        }
    });

    // Spawn the event forwarder — uses app_handle to access managed state
    // (State<'_> can't escape the function, but AppHandle is 'static)
    let handle_for_events = app_handle.clone();
    tokio::spawn(async move {
        let mut full_content = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                piki_api_client::ChatStreamEvent::Token(token) => {
                    full_content.push_str(&token);
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: token,
                            done: false,
                        },
                    );
                }
                piki_api_client::ChatStreamEvent::Done(_) => {
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: String::new(),
                            done: true,
                        },
                    );
                    // Append assistant message to history
                    let managed: tauri::State<'_, Mutex<DesktopApp>> =
                        handle_for_events.state();
                    let mut app = managed.lock();
                    app.chat_messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: full_content,
                    });
                    app.chat_streaming = false;
                    return;
                }
                piki_api_client::ChatStreamEvent::Error(e) => {
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: format!("\n\n[Error: {e}]"),
                            done: true,
                        },
                    );
                    let managed: tauri::State<'_, Mutex<DesktopApp>> =
                        handle_for_events.state();
                    let mut app = managed.lock();
                    app.chat_streaming = false;
                    return;
                }
            }
        }
    });

    Ok(())
}

/// Get the current chat configuration.
#[tauri::command]
pub async fn chat_get_config(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<ChatConfig, String> {
    let app = state.lock();
    Ok(app.chat_config.clone())
}

/// Update the chat configuration and persist it.
#[tauri::command]
pub async fn chat_set_config(
    state: State<'_, Mutex<DesktopApp>>,
    config: ChatConfig,
) -> Result<(), String> {
    tracing::info!(
        model = %config.model,
        base_url = %config.base_url,
        has_system_prompt = config.system_prompt.is_some(),
        "Updating chat config"
    );

    let mut app = state.lock();
    app.chat_config = config.clone();

    // Persist to settings
    if let Some(ref prefs) = app.storage.ui_prefs {
        let json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
        let _ = prefs.set_preference("chat_config", &json);
    }

    Ok(())
}

/// Get all chat messages.
#[tauri::command]
pub async fn chat_get_messages(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Vec<ChatMessage>, String> {
    let app = state.lock();
    Ok(app.chat_messages.clone())
}

/// Clear chat history.
#[tauri::command]
pub async fn chat_clear(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<(), String> {
    let mut app = state.lock();
    app.chat_messages.clear();
    Ok(())
}

/// List available Ollama models.
#[tauri::command]
pub async fn chat_list_models(
    base_url: String,
) -> Result<Vec<ChatModelInfo>, String> {
    tracing::debug!(base_url = %base_url, "Listing Ollama models");
    let client = piki_api_client::OllamaClient::new(&base_url);
    let models = client
        .list_models()
        .await
        .map_err(|e| {
            tracing::error!(base_url = %base_url, error = %e, "Failed to list Ollama models");
            format!("Failed to connect to Ollama: {e}")
        })?;

    Ok(models
        .into_iter()
        .map(|m| ChatModelInfo {
            name: m.name,
            size: m.size,
            modified_at: m.modified_at,
        })
        .collect())
}

/// Stop the current streaming response.
#[tauri::command]
pub async fn chat_stop(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<(), String> {
    let mut app = state.lock();
    app.chat_streaming = false;
    Ok(())
}
