use tokio::sync::mpsc;

use piki_api_client::{ChatClient, ChatStreamEvent, ChatWireMessage, RawToolCall};
use piki_core::chat::{ChatMessage, ChatRole, ToolCall};

use crate::context::ToolContext;
use crate::events::AgentEvent;
use crate::prompt;
use crate::tools::ToolRegistry;

/// Orchestrates the LLM <-> tool execution loop.
pub struct AgentLoop {
    client: Box<dyn ChatClient>,
    model: String,
    registry: ToolRegistry,
    context: ToolContext,
    max_iterations: usize,
    auto_approve: bool,
}

impl AgentLoop {
    pub fn new(
        client: Box<dyn ChatClient>,
        model: String,
        registry: ToolRegistry,
        context: ToolContext,
    ) -> Self {
        Self {
            client,
            model,
            registry,
            context,
            max_iterations: 20,
            auto_approve: false,
        }
    }

    /// Run the agentic loop. Sends progress events to `event_tx`.
    pub async fn run(
        &mut self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let tool_defs = self.registry.definitions_json();
        let tool_names: Vec<&str> = messages
            .first()
            .map(|_| {
                // Collect tool names for the system prompt
                tool_defs
                    .iter()
                    .filter_map(|d| d.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Build enriched system prompt
        let enriched_prompt = prompt::build_system_prompt(
            system_prompt.as_deref(),
            &self.context,
            &tool_names,
        )
        .await;

        // Convert ChatMessages to wire format
        let mut wire_messages = self.to_wire_messages(&enriched_prompt, &messages);

        for iteration in 0..self.max_iterations {
            tracing::info!(iteration, "Agent loop iteration");

            // Stream from the LLM
            let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<ChatStreamEvent>();
            let tools = if tool_defs.is_empty() {
                None
            } else {
                Some(tool_defs.as_slice())
            };

            self.client
                .chat_stream(&self.model, &wire_messages, tools, stream_tx)
                .await?;

            // Collect stream events
            let mut content = String::new();
            let mut tool_calls: Vec<RawToolCall> = Vec::new();

            while let Some(event) = stream_rx.recv().await {
                match event {
                    ChatStreamEvent::Token(token) => {
                        content.push_str(&token);
                        let _ = event_tx.send(AgentEvent::Token(token));
                    }
                    ChatStreamEvent::Done(full) => {
                        if content.is_empty() {
                            content = full;
                        }
                        break;
                    }
                    ChatStreamEvent::ToolCalls(calls) => {
                        tool_calls = calls;
                        break;
                    }
                    ChatStreamEvent::Error(e) => {
                        let _ = event_tx.send(AgentEvent::Error(e.clone()));
                        return Err(anyhow::anyhow!(e));
                    }
                }
            }

            // If no tool calls -> final response
            if tool_calls.is_empty() {
                let _ = event_tx.send(AgentEvent::Done(content));
                let _ = event_tx.send(AgentEvent::Finished);
                return Ok(());
            }

            // Convert raw tool calls to domain types
            let domain_calls: Vec<ToolCall> = tool_calls
                .iter()
                .map(|tc| ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: serde_json::from_str(&tc.arguments)
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                })
                .collect();

            let _ = event_tx.send(AgentEvent::ToolCallsStarted(domain_calls.clone()));

            // Add assistant message with tool calls to the conversation
            wire_messages.push(ChatWireMessage {
                role: "assistant".to_string(),
                content: content.clone(),
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
            });

            // Execute each tool call
            for tc in &tool_calls {
                let _ = event_tx.send(AgentEvent::ToolExecuting {
                    name: tc.name.clone(),
                });

                let result = match self.registry.get(&tc.name) {
                    Some(tool) => {
                        // Check if tool requires approval
                        if tool.requires_approval() && !self.auto_approve {
                            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                            let _ = event_tx.send(AgentEvent::ApprovalRequired(
                                crate::context::ApprovalRequest {
                                    tool_call_id: tc.id.clone(),
                                    tool_name: tc.name.clone(),
                                    description: format!(
                                        "{} with args: {}",
                                        tc.name,
                                        tc.arguments
                                    ),
                                    response_tx: resp_tx,
                                },
                            ));

                            // Wait for approval with timeout
                            let approval = tokio::time::timeout(
                                std::time::Duration::from_secs(300),
                                resp_rx,
                            )
                            .await;

                            match approval {
                                Ok(Ok(crate::context::ApprovalResponse::Allow)) => {
                                    // Proceed with execution
                                }
                                Ok(Ok(crate::context::ApprovalResponse::AllowAll)) => {
                                    self.auto_approve = true;
                                    // Proceed with execution
                                }
                                Ok(Ok(crate::context::ApprovalResponse::Deny)) | Ok(Err(_)) | Err(_) => {
                                    let err_msg = "User denied tool execution".to_string();
                                    let _ = event_tx.send(AgentEvent::ToolResult {
                                        tool_call_id: tc.id.clone(),
                                        name: tc.name.clone(),
                                        result: err_msg.clone(),
                                        is_error: true,
                                    });
                                    wire_messages.push(ChatWireMessage {
                                        role: "tool".to_string(),
                                        content: err_msg,
                                        tool_calls: None,
                                        tool_call_id: Some(tc.id.clone()),
                                    });
                                    continue;
                                }
                            }
                        }

                        let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        match tool.execute(args, &self.context).await {
                            Ok(output) => output,
                            Err(e) => {
                                let err_msg = format!("Tool error: {e}");
                                let _ = event_tx.send(AgentEvent::ToolResult {
                                    tool_call_id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    result: err_msg.clone(),
                                    is_error: true,
                                });
                                wire_messages.push(ChatWireMessage {
                                    role: "tool".to_string(),
                                    content: err_msg,
                                    tool_calls: None,
                                    tool_call_id: Some(tc.id.clone()),
                                });
                                continue;
                            }
                        }
                    }
                    None => {
                        let err_msg = format!("Unknown tool: {}", tc.name);
                        let _ = event_tx.send(AgentEvent::ToolResult {
                            tool_call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            result: err_msg.clone(),
                            is_error: true,
                        });
                        wire_messages.push(ChatWireMessage {
                            role: "tool".to_string(),
                            content: err_msg,
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                        continue;
                    }
                };

                let _ = event_tx.send(AgentEvent::ToolResult {
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    result: result.clone(),
                    is_error: false,
                });

                // Add tool result message
                wire_messages.push(ChatWireMessage {
                    role: "tool".to_string(),
                    content: result,
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                });
            }

            // Loop continues — send the enriched conversation back to the LLM
        }

        let _ = event_tx.send(AgentEvent::Error(format!(
            "Agent reached maximum iterations ({})",
            self.max_iterations
        )));
        let _ = event_tx.send(AgentEvent::Finished);
        Ok(())
    }

    fn to_wire_messages(
        &self,
        system_prompt: &str,
        messages: &[ChatMessage],
    ) -> Vec<ChatWireMessage> {
        let mut wire = Vec::with_capacity(messages.len() + 1);

        // System prompt
        if !system_prompt.is_empty() {
            wire.push(ChatWireMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        for msg in messages {
            let role = match msg.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "tool",
            };
            let tool_calls = msg.tool_calls.as_ref().map(|tcs| {
                tcs.iter()
                    .map(|tc| RawToolCall {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                    })
                    .collect()
            });
            wire.push(ChatWireMessage {
                role: role.to_string(),
                content: msg.content.clone(),
                tool_calls,
                tool_call_id: msg.tool_call_id.clone(),
            });
        }

        wire
    }
}
