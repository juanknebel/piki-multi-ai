pub mod agent_loop;
pub mod chat_bridge;
pub mod context;
pub mod events;
pub mod prompt;
pub mod tools;

pub use agent_loop::AgentLoop;
pub use chat_bridge::{
    chat_client_for, chat_client_for_with_key, chat_client_for_with_key_and_search, to_wire,
    wire_conversation,
};
pub use context::{ApprovalRequest, ApprovalResponse, ToolContext};
pub use events::AgentEvent;
pub use tools::{Tool, ToolRegistry};
