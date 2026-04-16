pub mod edit_file;
pub mod git_diff;
pub mod git_status;
pub mod list_files;
pub mod read_file;
pub mod search_code;
pub mod shell;

use crate::context::ToolContext;

/// A tool that the agent can invoke.
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Unique name used in LLM tool definitions.
    fn name(&self) -> &str;

    /// Human-readable description for the LLM.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Whether this tool modifies state and requires user approval.
    fn requires_approval(&self) -> bool {
        false
    }

    /// Execute the tool with the given arguments.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<String>;
}

/// Registry of available tools.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Create a registry with the default read-only tools.
    pub fn default_read_only() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(git_status::GitStatusTool));
        reg.register(Box::new(read_file::ReadFileTool));
        reg.register(Box::new(list_files::ListFilesTool));
        reg.register(Box::new(search_code::SearchCodeTool));
        reg.register(Box::new(git_diff::GitDiffTool));
        reg
    }

    /// Create a registry with all tools including write tools (edit_file, shell).
    pub fn default_all() -> Self {
        let mut reg = Self::default_read_only();
        reg.register(Box::new(edit_file::EditFileTool));
        reg.register(Box::new(shell::ShellTool));
        reg
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|t| t.name() == name).map(|t| &**t)
    }

    /// Generate tool definitions in OpenAI/Ollama JSON format for the LLM.
    pub fn definitions_json(&self) -> Vec<serde_json::Value> {
        self.tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.parameters_schema(),
                    }
                })
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
