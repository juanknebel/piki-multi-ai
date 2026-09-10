//! Bridge between `piki_core`'s chat domain types and `piki_api_client`'s
//! wire types.
//!
//! It lives here because this is the only crate that depends on both: `core`
//! must not depend on `api-client`, and making `api-client` depend on `core`
//! would drag SQLite, notify and the PTY stack into an HTTP client.
//!
//! Both frontends previously did this conversion inline, each with its own
//! copy of the per-server-type message struct mapping — even though
//! `ChatClient` already existed to hide exactly that difference, and the
//! agent path already used it. Three copies of the client selection, two of
//! the message conversion.

use piki_api_client::{
    ChatClient, ChatWireMessage, LlamaCppClient, OllamaClient, OpenRouterClient, RawToolCall,
};
use piki_core::chat::{ChatConfig, ChatMessage, ChatServerType};

/// The chat client for `server_type`, behind the trait that hides each
/// backend's message format.
pub fn chat_client_for(server_type: ChatServerType, base_url: &str) -> Box<dyn ChatClient> {
    chat_client_for_with_key(server_type, base_url, None)
}

/// Like `chat_client_for` but with an optional API key (for OpenRouter).
pub fn chat_client_for_with_key(
    server_type: ChatServerType,
    base_url: &str,
    api_key: Option<String>,
) -> Box<dyn ChatClient> {
    chat_client_for_with_key_and_search(server_type, base_url, api_key, false)
}

/// Like `chat_client_for_with_key` but with web_search toggle for OpenRouter.
pub fn chat_client_for_with_key_and_search(
    server_type: ChatServerType,
    base_url: &str,
    api_key: Option<String>,
    web_search: bool,
) -> Box<dyn ChatClient> {
    // Env var fallback for piki-ai parity
    let effective_key = api_key.or_else(|| {
        std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
    });
    match server_type {
        ChatServerType::Ollama => Box::new(OllamaClient::new(base_url)),
        ChatServerType::LlamaCpp => Box::new(LlamaCppClient::new(base_url)),
        ChatServerType::OpenRouter => Box::new(
            OpenRouterClient::new_with_key(base_url, effective_key).with_web_search(web_search),
        ),
    }
}

/// Convert one domain message to its wire form, preserving tool-call
/// metadata (a `Tool` message without its `tool_call_id` is rejected by
/// OpenAI-shaped APIs, and the hand-written conversions used to drop it).
pub fn to_wire(message: &ChatMessage) -> ChatWireMessage {
    ChatWireMessage {
        role: message.role.as_wire_str().to_string(),
        content: message.content.clone(),
        // The domain type holds arguments as JSON; the wire carries them as
        // a string.
        tool_calls: message.tool_calls.as_ref().map(|calls| {
            calls
                .iter()
                .map(|tc| RawToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                })
                .collect()
        }),
        tool_call_id: message.tool_call_id.clone(),
    }
}

/// The full wire conversation: the configured system prompt (when non-empty)
/// followed by `messages`.
pub fn wire_conversation(config: &ChatConfig, messages: &[ChatMessage]) -> Vec<ChatWireMessage> {
    let mut wire = Vec::with_capacity(messages.len() + 1);
    if let Some(sys) = config.system_prompt.as_ref().filter(|s| !s.is_empty()) {
        wire.push(ChatWireMessage {
            role: "system".to_string(),
            content: sys.clone(),
            tool_calls: None,
            tool_call_id: None,
        });
    }
    wire.extend(messages.iter().map(to_wire));
    wire
}

#[cfg(test)]
mod tests {
    use super::*;
    use piki_core::chat::ChatRole;

    fn msg(role: ChatRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn config(system_prompt: Option<&str>) -> ChatConfig {
        ChatConfig {
            system_prompt: system_prompt.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn system_prompt_leads_the_conversation() {
        let wire = wire_conversation(&config(Some("be terse")), &[msg(ChatRole::User, "hola")]);
        assert_eq!(wire.len(), 2);
        assert_eq!(wire[0].role, "system");
        assert_eq!(wire[0].content, "be terse");
        assert_eq!(wire[1].role, "user");
    }

    #[test]
    fn an_absent_or_empty_system_prompt_adds_no_message() {
        for sys in [None, Some("")] {
            let wire = wire_conversation(&config(sys), &[msg(ChatRole::User, "hola")]);
            assert_eq!(wire.len(), 1, "sys={sys:?}");
            assert_eq!(wire[0].role, "user");
        }
    }

    #[test]
    fn every_role_maps_to_its_wire_string() {
        let roles = [
            (ChatRole::System, "system"),
            (ChatRole::User, "user"),
            (ChatRole::Assistant, "assistant"),
            (ChatRole::Tool, "tool"),
        ];
        for (role, expected) in roles {
            assert_eq!(to_wire(&msg(role, "x")).role, expected);
        }
    }

    /// The inline conversions hardcoded `tool_call_id: None`, which makes a
    /// Tool message invalid on OpenAI-shaped backends.
    #[test]
    fn tool_call_metadata_survives_the_conversion() {
        let mut m = msg(ChatRole::Tool, "result");
        m.tool_call_id = Some("call_42".into());
        let wire = to_wire(&m);
        assert_eq!(wire.tool_call_id.as_deref(), Some("call_42"));
    }
}
