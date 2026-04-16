use crate::context::ApprovalRequest;

/// Events emitted by the agent loop to the UI layer.
pub enum AgentEvent {
    /// A streamed content token from the LLM.
    Token(String),
    /// The LLM finished with a text-only response (no tool calls).
    Done(String),
    /// The LLM requested tool calls.
    ToolCallsStarted(Vec<piki_core::chat::ToolCall>),
    /// A tool is about to be executed.
    ToolExecuting { name: String },
    /// A tool finished executing.
    ToolResult {
        tool_call_id: String,
        name: String,
        result: String,
        is_error: bool,
    },
    /// A write-tool requires user approval before execution.
    ApprovalRequired(ApprovalRequest),
    /// The agent loop finished (all tool calls resolved, final answer delivered).
    Finished,
    /// An error occurred.
    Error(String),
}
