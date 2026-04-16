use crate::context::ToolContext;
use super::Tool;

pub struct GitStatusTool;

#[async_trait::async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Get the git status of the current workspace, showing modified, added, deleted, and untracked files."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<String> {
        let files = piki_core::git::get_changed_files(&ctx.workspace_path).await?;
        if files.is_empty() {
            return Ok("Working tree clean — no changes.".to_string());
        }
        let mut out = String::new();
        for f in &files {
            let status = match f.status {
                piki_core::domain::FileStatus::Modified => "M ",
                piki_core::domain::FileStatus::Added => "A ",
                piki_core::domain::FileStatus::Deleted => "D ",
                piki_core::domain::FileStatus::Renamed => "R ",
                piki_core::domain::FileStatus::Untracked => "? ",
                piki_core::domain::FileStatus::Conflicted => "U ",
                piki_core::domain::FileStatus::Staged => "S ",
                piki_core::domain::FileStatus::StagedModified => "SM",
            };
            out.push_str(status);
            out.push_str(&f.path);
            out.push('\n');
        }
        Ok(out)
    }
}
