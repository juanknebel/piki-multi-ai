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
            if input.is_empty() || app.chat_panel.streaming || app.chat_panel.config.model.is_empty()
            {
                if app.chat_panel.config.model.is_empty() {
                    app.set_toast("No model selected. Press Tab to pick one.", ToastLevel::Error);
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

                let client: Box<dyn piki_api_client::ChatClient> = match server_type {
                    piki_core::chat::ChatServerType::Ollama => {
                        Box::new(piki_api_client::OllamaClient::new(&base_url))
                    }
                    piki_core::chat::ChatServerType::LlamaCpp => {
                        Box::new(piki_api_client::LlamaCppClient::new(&base_url))
                    }
                };

                let registry = piki_agent::ToolRegistry::default_all();
                let context = piki_agent::ToolContext {
                    workspace_path: ws_path,
                    source_repo,
                };

                tokio::spawn(async move {
                    let mut agent = piki_agent::AgentLoop::new(
                        client, model, registry, context,
                    );
                    if let Err(e) = agent.run(messages, system_prompt, event_tx.clone()).await {
                        tracing::error!(error = %e, "Agent loop error");
                        let _ = event_tx.send(piki_agent::AgentEvent::Error(e.to_string()));
                    }
                });
            } else {
                // ── Plain chat mode (existing behavior) ──
                let tx = app.chat_token_tx.clone();

                let mut role_contents: Vec<(&str, String)> = Vec::new();
                if let Some(ref sys) = app.chat_panel.config.system_prompt
                    && !sys.is_empty()
                {
                    role_contents.push(("system", sys.clone()));
                }
                for msg in &app.chat_panel.messages {
                    let role = match msg.role {
                        piki_core::chat::ChatRole::System => "system",
                        piki_core::chat::ChatRole::User => "user",
                        piki_core::chat::ChatRole::Assistant => "assistant",
                        piki_core::chat::ChatRole::Tool => "tool",
                    };
                    role_contents.push((role, msg.content.clone()));
                }

                tracing::info!(
                    model = %model,
                    base_url = %base_url,
                    server = %server_type.label(),
                    msg_count = role_contents.len(),
                    "TUI: sending chat message"
                );

                match server_type {
                    piki_core::chat::ChatServerType::Ollama => {
                        let msgs: Vec<piki_api_client::OllamaMessage> = role_contents
                            .into_iter()
                            .map(|(r, c)| piki_api_client::OllamaMessage {
                                role: r.to_string(),
                                content: c,
                                tool_calls: None,
                            })
                            .collect();
                        let client = piki_api_client::OllamaClient::new(&base_url);
                        tokio::spawn(async move {
                            if let Err(e) = client.chat_stream(&model, &msgs, tx).await {
                                tracing::error!(error = %e, "Ollama chat_stream error");
                            }
                        });
                    }
                    piki_core::chat::ChatServerType::LlamaCpp => {
                        let msgs: Vec<piki_api_client::LlamaCppMessage> = role_contents
                            .into_iter()
                            .map(|(r, c)| piki_api_client::LlamaCppMessage {
                                role: r.to_string(),
                                content: c,
                                tool_calls: None,
                                tool_call_id: None,
                            })
                            .collect();
                        let client = piki_api_client::LlamaCppClient::new(&base_url);
                        tokio::spawn(async move {
                            if let Err(e) = client.chat_stream(&model, &msgs, tx).await {
                                tracing::error!(error = %e, "llama.cpp chat_stream error");
                            }
                        });
                    }
                }
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
                                let _ = chat_tx
                                    .send(piki_api_client::ChatStreamEvent::Done(payload));
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
                                let names: Vec<String> =
                                    models.into_iter().map(|m| m.id).collect();
                                let payload = format!("__MODELS__{}", names.join("\n"));
                                let _ = chat_tx
                                    .send(piki_api_client::ChatStreamEvent::Done(payload));
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
            }
        }
        other => unreachable!("non-chat action routed to action::chat: {other:?}"),
    }
    Ok(())
}
