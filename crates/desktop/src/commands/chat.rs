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

/// Structured agent-loop activity for the chat panel's tool cards
/// (`"chat-agent-event"` Tauri event). Text still streams over
/// `"chat-token"`; this channel carries what a card needs.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum ChatAgentEventPayload {
    /// The LLM asked for these tool calls (args are the parsed JSON).
    ToolCalls {
        calls: Vec<piki_core::chat::ToolCall>,
    },
    ToolExecuting {
        name: String,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        result: String,
        is_error: bool,
    },
    /// A write-tool waits for the user; answer with `chat_approve`.
    ApprovalRequired {
        tool_call_id: String,
        tool_name: String,
        description: String,
    },
}

/// A user's answer to `ApprovalRequired`, as the frontend sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow,
    Deny,
    AllowAll,
}

impl From<ApprovalDecision> for piki_agent::ApprovalResponse {
    fn from(d: ApprovalDecision) -> Self {
        match d {
            ApprovalDecision::Allow => Self::Allow,
            ApprovalDecision::Deny => Self::Deny,
            ApprovalDecision::AllowAll => Self::AllowAll,
        }
    }
}

type PendingApprovals =
    std::collections::HashMap<String, tokio::sync::oneshot::Sender<piki_agent::ApprovalResponse>>;

/// Answer the approval `tool_call_id` is waiting on. Errors when nothing is
/// pending under that id (already answered, or the loop timed out).
pub(crate) fn resolve_approval(
    pending: &mut PendingApprovals,
    tool_call_id: &str,
    decision: ApprovalDecision,
) -> Result<(), String> {
    let tx = pending
        .remove(tool_call_id)
        .ok_or_else(|| format!("No pending approval for tool call {tool_call_id}"))?;
    tx.send(decision.into())
        .map_err(|_| "The agent is no longer waiting for this approval".to_string())
}

/// Longest tool result kept in history (chars); a card shows the rest as
/// "Show more" up to this, the LLM always got the full text.
const TOOL_RESULT_HISTORY_CAP: usize = 4000;

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
            tool_calls: None,
            tool_call_id: None,
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
        server = %config.server_type.label(),
        msg_count = messages.len(),
        "Sending chat message"
    );

    let msgs = piki_agent::wire_conversation(&config, &messages);
    let model = config.model.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // `ChatClient` hides each backend's message format, so this no longer
    // has to know one from the other (see piki_agent::chat_bridge).
    let client = piki_agent::chat_client_for(config.server_type, &config.base_url);
    tokio::spawn(async move {
        if let Err(e) = client.chat_stream(&model, &msgs, None, tx).await {
            tracing::error!(error = %e, "chat_stream failed");
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
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    let mut app = managed.lock();
                    app.chat_messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: full_content,
                        tool_calls: None,
                        tool_call_id: None,
                    });
                    app.chat_streaming = false;
                    return;
                }
                piki_api_client::ChatStreamEvent::ToolCalls(_calls) => {
                    // Tool calls will be handled by agent loop (F5).
                    // In plain chat mode, treat as end of stream.
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    let mut app = managed.lock();
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
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
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
pub async fn chat_get_config(state: State<'_, Mutex<DesktopApp>>) -> Result<ChatConfig, String> {
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
pub async fn chat_clear(state: State<'_, Mutex<DesktopApp>>) -> Result<(), String> {
    let mut app = state.lock();
    app.chat_messages.clear();
    app.chat_pending_approvals.clear();
    Ok(())
}

/// List available models from the configured server.
#[tauri::command]
pub async fn chat_list_models(
    base_url: String,
    server_type: piki_core::chat::ChatServerType,
) -> Result<Vec<ChatModelInfo>, String> {
    tracing::debug!(base_url = %base_url, server = %server_type.label(), "Listing chat models");

    match server_type {
        piki_core::chat::ChatServerType::Ollama => {
            let client = piki_api_client::OllamaClient::new(&base_url);
            let models = client.list_models().await.map_err(|e| {
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
        piki_core::chat::ChatServerType::LlamaCpp => {
            let client = piki_api_client::LlamaCppClient::new(&base_url);
            let models = client.list_models().await.map_err(|e| {
                tracing::error!(base_url = %base_url, error = %e, "Failed to list llama.cpp models");
                format!("Failed to connect to llama.cpp: {e}")
            })?;
            Ok(models
                .into_iter()
                .map(|m| ChatModelInfo {
                    name: m.id,
                    size: 0,
                    modified_at: String::new(),
                })
                .collect())
        }
    }
}

/// Stop the current streaming response.
#[tauri::command]
pub async fn chat_stop(state: State<'_, Mutex<DesktopApp>>) -> Result<(), String> {
    let mut app = state.lock();
    app.chat_streaming = false;
    // Dropping the senders denies whatever the loop was waiting on.
    app.chat_pending_approvals.clear();
    Ok(())
}

/// Send a user message using the agentic tool-use loop.
#[tauri::command]
pub async fn chat_send_agent_message(
    app_handle: AppHandle,
    state: State<'_, Mutex<DesktopApp>>,
    message: String,
) -> Result<(), String> {
    let (config, messages, ws_path) = {
        let mut app = state.lock();

        if app.chat_streaming {
            return Err("A response is already being streamed".to_string());
        }

        app.chat_messages.push(ChatMessage {
            role: ChatRole::User,
            content: message,
            tool_calls: None,
            tool_call_id: None,
        });
        app.chat_streaming = true;
        // A stale approval from a previous run is answered Deny by the drop.
        app.chat_pending_approvals.clear();

        let ws_path = if !app.workspaces.is_empty() {
            app.workspaces[app.active_workspace].info.path.clone()
        } else {
            std::env::current_dir().unwrap_or_default()
        };

        (app.chat_config.clone(), app.chat_messages.clone(), ws_path)
    };

    if config.model.is_empty() {
        let mut app = state.lock();
        app.chat_streaming = false;
        return Err("No model selected.".to_string());
    }

    tracing::info!(
        model = %config.model,
        base_url = %config.base_url,
        server = %config.server_type.label(),
        agent = true,
        "Desktop: sending agent message"
    );

    let client: Box<dyn piki_api_client::ChatClient> = match config.server_type {
        piki_core::chat::ChatServerType::Ollama => {
            Box::new(piki_api_client::OllamaClient::new(&config.base_url))
        }
        piki_core::chat::ChatServerType::LlamaCpp => {
            Box::new(piki_api_client::LlamaCppClient::new(&config.base_url))
        }
    };

    let registry = piki_agent::ToolRegistry::default_all();
    let context = piki_agent::ToolContext {
        workspace_path: ws_path.clone(),
        source_repo: ws_path,
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let model = config.model.clone();
    let system_prompt = config.system_prompt.clone();

    // Spawn the agent loop
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        let mut agent = piki_agent::AgentLoop::new(client, model, registry, context);
        if let Err(e) = agent
            .run(messages, system_prompt, event_tx_clone.clone())
            .await
        {
            tracing::error!(error = %e, "Agent loop error");
            let _ = event_tx_clone.send(piki_agent::AgentEvent::Error(e.to_string()));
        }
    });

    // Spawn the event forwarder
    let handle_for_events = app_handle.clone();
    tokio::spawn(async move {
        let mut full_content = String::new();
        while let Some(event) = event_rx.recv().await {
            match event {
                piki_agent::AgentEvent::Token(token) => {
                    full_content.push_str(&token);
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: token,
                            done: false,
                        },
                    );
                }
                piki_agent::AgentEvent::Done(content) => {
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    let mut app = managed.lock();
                    let final_content = if full_content.is_empty() {
                        content
                    } else {
                        std::mem::take(&mut full_content)
                    };
                    app.chat_messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: final_content,
                        tool_calls: None,
                        tool_call_id: None,
                    });
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: String::new(),
                            done: false,
                        },
                    );
                }
                piki_agent::AgentEvent::ToolCallsStarted(calls) => {
                    full_content.clear();
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: String::new(),
                            done: false,
                        },
                    );
                    let _ = handle_for_events.emit(
                        "chat-agent-event",
                        ChatAgentEventPayload::ToolCalls { calls },
                    );
                }
                piki_agent::AgentEvent::ToolExecuting { name } => {
                    let _ = handle_for_events.emit(
                        "chat-agent-event",
                        ChatAgentEventPayload::ToolExecuting { name },
                    );
                }
                piki_agent::AgentEvent::ToolResult {
                    tool_call_id,
                    name,
                    result,
                    is_error,
                } => {
                    let _ = handle_for_events.emit(
                        "chat-agent-event",
                        ChatAgentEventPayload::ToolResult {
                            tool_call_id: tool_call_id.clone(),
                            name: name.clone(),
                            result: result.clone(),
                            is_error,
                        },
                    );
                    // History keeps the TUI's `[name] [Error] text` shape
                    // (chat-context.ts `parseToolMessage` reads it back), cut
                    // on a char boundary.
                    let prefix = if is_error { "[Error] " } else { "" };
                    let body: String = result.chars().take(TOOL_RESULT_HISTORY_CAP).collect();
                    let ellipsis = if body.len() < result.len() { "..." } else { "" };
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    let mut app = managed.lock();
                    app.chat_messages.push(ChatMessage {
                        role: ChatRole::Tool,
                        content: format!("[{name}] {prefix}{body}{ellipsis}"),
                        tool_calls: None,
                        tool_call_id: Some(tool_call_id),
                    });
                }
                piki_agent::AgentEvent::Finished => {
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: String::new(),
                            done: true,
                        },
                    );
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    let mut app = managed.lock();
                    app.chat_streaming = false;
                    return;
                }
                piki_agent::AgentEvent::Error(e) => {
                    let _ = handle_for_events.emit(
                        "chat-token",
                        ChatTokenPayload {
                            content: format!("\n\n[Agent Error: {e}]"),
                            done: true,
                        },
                    );
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    let mut app = managed.lock();
                    app.chat_streaming = false;
                    return;
                }
                piki_agent::AgentEvent::ApprovalRequired(req) => {
                    // Park the oneshot until `chat_approve` answers it; the
                    // card renders Approve / Deny from this event.
                    let managed: tauri::State<'_, Mutex<DesktopApp>> = handle_for_events.state();
                    {
                        let mut app = managed.lock();
                        app.chat_pending_approvals
                            .insert(req.tool_call_id.clone(), req.response_tx);
                    }
                    let _ = handle_for_events.emit(
                        "chat-agent-event",
                        ChatAgentEventPayload::ApprovalRequired {
                            tool_call_id: req.tool_call_id,
                            tool_name: req.tool_name,
                            description: req.description,
                        },
                    );
                }
            }
        }
    });

    Ok(())
}

/// Answer a pending write-tool approval (`ApprovalRequired` card).
#[tauri::command]
pub async fn chat_approve(
    state: State<'_, Mutex<DesktopApp>>,
    tool_call_id: String,
    decision: ApprovalDecision,
) -> Result<(), String> {
    tracing::info!(tool_call_id = %tool_call_id, ?decision, "Chat tool approval answered");
    let mut app = state.lock();
    resolve_approval(&mut app.chat_pending_approvals, &tool_call_id, decision)
}

/// Set agent mode on/off.
#[tauri::command]
pub async fn chat_set_agent_mode(
    state: State<'_, Mutex<DesktopApp>>,
    enabled: bool,
) -> Result<(), String> {
    let mut app = state.lock();
    app.chat_agent_mode = enabled;
    Ok(())
}

/// Get current agent mode state.
#[tauri::command]
pub async fn chat_get_agent_mode(state: State<'_, Mutex<DesktopApp>>) -> Result<bool, String> {
    let app = state.lock();
    Ok(app.chat_agent_mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use piki_agent::ApprovalResponse;

    #[tokio::test]
    async fn resolve_approval_answers_the_waiting_loop() {
        let mut pending = PendingApprovals::new();
        let (tx, rx) = tokio::sync::oneshot::channel();
        pending.insert("call-1".into(), tx);

        resolve_approval(&mut pending, "call-1", ApprovalDecision::Allow).unwrap();
        assert!(matches!(rx.await, Ok(ApprovalResponse::Allow)));
        assert!(pending.is_empty(), "answered approvals leave the map");
        // A second answer for the same id is an error, not a panic.
        assert!(resolve_approval(&mut pending, "call-1", ApprovalDecision::Deny).is_err());
    }

    #[tokio::test]
    async fn dropping_a_pending_approval_reads_as_deny() {
        let mut pending = PendingApprovals::new();
        let (tx, rx) = tokio::sync::oneshot::channel::<ApprovalResponse>();
        pending.insert("call-2".into(), tx);
        pending.clear();
        // agent_loop.rs maps `Err(RecvError)` to Deny.
        assert!(rx.await.is_err());
    }

    #[test]
    fn approval_decision_deserializes_snake_case() {
        let d: ApprovalDecision = serde_json::from_str("\"allow_all\"").unwrap();
        assert_eq!(d, ApprovalDecision::AllowAll);
    }
}
