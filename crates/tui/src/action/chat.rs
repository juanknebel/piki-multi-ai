use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{App, ToastLevel};
use piki_core::workspace::WorkspaceManager;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::ChatSendMessage => {
            let input = std::mem::take(&mut app.chat_panel.input);
            let input = input.trim().to_string();
            if input.is_empty()
                || app.chat_panel.streaming
                || app.chat_panel.config.model.is_empty()
            {
                if app.chat_panel.config.model.is_empty() {
                    app.set_toast(
                        "No model selected. Press Tab to pick one.",
                        ToastLevel::Error,
                    );
                }
                return Ok(());
            }

            // Append user message
            app.chat_panel.messages.push(piki_core::chat::ChatMessage {
                role: piki_core::chat::ChatRole::User,
                content: input,
                tool_calls: None,
                tool_call_id: None,
            });
            app.chat_panel.input_cursor = 0;
            app.chat_panel.streaming = true;
            app.chat_panel.current_response.clear();

            let model = app.chat_panel.config.model.clone();
            let base_url = app.chat_panel.config.base_url.clone();
            let server_type = app.chat_panel.config.server_type;
            if server_type == piki_core::chat::ChatServerType::OpenRouter {
                let has_key = app
                    .config
                    .chat
                    .openrouter_api_key
                    .as_ref()
                    .map(|k| !k.trim().is_empty())
                    .unwrap_or(false)
                    || app.chat_panel.config.effective_api_key().is_some();
                if !has_key {
                    app.chat_panel.streaming = false;
                    app.chat_panel.current_response.clear();
                    // Remove the just-pushed user message since we won't send it
                    app.chat_panel.messages.pop();
                    let cfg_path = app.paths.config_path();
                    app.set_toast(format!("No OpenRouter API key. Set [chat] openrouter_api_key in {} or OPENROUTER_API_KEY env.", cfg_path.display()), crate::app::ToastLevel::Error);
                    return Ok(());
                }
                if model.trim().is_empty() {
                    app.chat_panel.streaming = false;
                    app.chat_panel.current_response.clear();
                    app.chat_panel.messages.pop();
                    app.set_toast("No model selected for OpenRouter. Press Tab to list models (needs API key) and pick one.", crate::app::ToastLevel::Error);
                    return Ok(());
                }
            }

            if app.chat_panel.agent_mode {
                // ── Agent mode: use AgentLoop with tools ──
                let messages = app.chat_panel.messages.clone();
                let system_prompt = app.chat_panel.config.system_prompt.clone();
                let event_tx = app.agent_event_tx.clone();

                // Get workspace path for tool context
                let ws_path = if !app.workspaces.is_empty() {
                    app.workspaces[app.active_workspace].info.path.clone()
                } else {
                    std::env::current_dir().unwrap_or_default()
                };
                let source_repo = ws_path.clone();

                tracing::info!(
                    model = %model,
                    base_url = %base_url,
                    server = %server_type.label(),
                    agent = true,
                    "TUI: sending agent message"
                );

                let api_key = app
                    .config
                    .chat
                    .openrouter_api_key
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| app.chat_panel.config.effective_api_key());
                let client = piki_agent::chat_client_for_with_key(server_type, &base_url, api_key);

                let registry = piki_agent::ToolRegistry::default_all();
                let context = piki_agent::ToolContext {
                    workspace_path: ws_path,
                    source_repo,
                };

                let task = tokio::spawn(async move {
                    let mut agent = piki_agent::AgentLoop::new(client, model, registry, context);
                    if let Err(e) = agent.run(messages, system_prompt, event_tx.clone()).await {
                        tracing::error!(error = %e, "Agent loop error");
                        let _ = event_tx.send(piki_agent::AgentEvent::Error(e.to_string()));
                    }
                });
                app.chat_panel.stream_abort = Some(task.abort_handle());
                app.chat_panel.last_stream_activity = Some(std::time::Instant::now());
            } else {
                // ── Plain chat mode ──
                let tx = app.chat_token_tx.clone();
                let msgs =
                    piki_agent::wire_conversation(&app.chat_panel.config, &app.chat_panel.messages);

                tracing::info!(
                    model = %model,
                    base_url = %base_url,
                    server = %server_type.label(),
                    msg_count = msgs.len(),
                    "TUI: sending chat message"
                );

                // `ChatClient` hides each backend's message format, so this
                // no longer has to know one from the other.
                let api_key = app
                    .config
                    .chat
                    .openrouter_api_key
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| app.chat_panel.config.effective_api_key());
                let client = piki_agent::chat_client_for_with_key(server_type, &base_url, api_key);
                let task = tokio::spawn(async move {
                    if let Err(e) = client.chat_stream(&model, &msgs, None, tx).await {
                        tracing::error!(error = %e, "chat_stream error");
                    }
                });
                app.chat_panel.stream_abort = Some(task.abort_handle());
                app.chat_panel.last_stream_activity = Some(std::time::Instant::now());
            }
        }
        Action::ChatLoadModels => {
            let base_url = app.chat_panel.config.base_url.clone();
            let server_type = app.chat_panel.config.server_type;
            let status_tx = app.status_tx.clone();
            let chat_tx = app.chat_token_tx.clone();
            tracing::debug!(base_url = %base_url, server = %server_type.label(), "TUI: loading chat models");

            match server_type {
                piki_core::chat::ChatServerType::Ollama => {
                    tokio::spawn(async move {
                        let client = piki_api_client::OllamaClient::new(&base_url);
                        match client.list_models().await {
                            Ok(models) => {
                                let names: Vec<String> =
                                    models.into_iter().map(|m| m.name).collect();
                                let payload = format!("__MODELS__{}", names.join("\n"));
                                let _ =
                                    chat_tx.send(piki_api_client::ChatStreamEvent::Done(payload));
                            }
                            Err(e) => {
                                let msg = format!("{e}. Is Ollama running? (ollama serve)");
                                let _ = status_tx.send(msg);
                            }
                        }
                    });
                }
                piki_core::chat::ChatServerType::LlamaCpp => {
                    tokio::spawn(async move {
                        let client = piki_api_client::LlamaCppClient::new(&base_url);
                        match client.list_models().await {
                            Ok(models) => {
                                let names: Vec<String> = models.into_iter().map(|m| m.id).collect();
                                let payload = format!("__MODELS__{}", names.join("\n"));
                                let _ =
                                    chat_tx.send(piki_api_client::ChatStreamEvent::Done(payload));
                            }
                            Err(e) => {
                                let msg = format!(
                                    "{e}. Is llama-server running? (llama-server -m model.gguf)"
                                );
                                let _ = status_tx.send(msg);
                            }
                        }
                    });
                }
                piki_core::chat::ChatServerType::OpenRouter => {
                    let api_key = app
                        .config
                        .chat
                        .openrouter_api_key
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| app.chat_panel.config.effective_api_key());
                    if api_key
                        .as_ref()
                        .map(|k| k.trim().is_empty())
                        .unwrap_or(true)
                    {
                        let cfg_path = app.paths.config_path();
                        let msg = format!(
                            "No OpenRouter API key. Set [chat] openrouter_api_key in {} or OPENROUTER_API_KEY env, then Tab again.",
                            cfg_path.display()
                        );
                        let _ = status_tx.send(msg);
                    } else {
                        tokio::spawn(async move {
                            let client =
                                piki_api_client::OpenRouterClient::new_with_key(&base_url, api_key);
                            match client.list_models().await {
                                Ok(models) => {
                                    let names: Vec<String> =
                                        models.into_iter().map(|m| m.id).collect();
                                    let payload = format!("__MODELS__{}", names.join("\n"));
                                    let _ = chat_tx
                                        .send(piki_api_client::ChatStreamEvent::Done(payload));
                                }
                                Err(e) => {
                                    let msg = format!(
                                        "{e}. Check [chat] openrouter_api_key in config.toml / OPENROUTER_API_KEY env and base URL https://openrouter.ai/api/v1."
                                    );
                                    let _ = status_tx.send(msg);
                                }
                            }
                        });
                    }
                }
            }
        }
        other => unreachable!("non-chat action routed to action::chat: {other:?}"),
    }
    Ok(())
}
