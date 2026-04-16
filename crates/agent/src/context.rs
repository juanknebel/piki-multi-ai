use std::path::PathBuf;

/// Context passed to tool executions.
pub struct ToolContext {
    /// Path to the active workspace directory.
    pub workspace_path: PathBuf,
    /// Path to the source git repository root.
    pub source_repo: PathBuf,
}

/// User response to a tool approval request.
pub enum ApprovalResponse {
    Allow,
    Deny,
    /// Allow this and all future write-tool calls in this session.
    AllowAll,
}

/// Request sent to the UI when a write-tool needs user approval.
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub description: String,
    pub response_tx: tokio::sync::oneshot::Sender<ApprovalResponse>,
}
