use crate::context::ToolContext;
use super::Tool;

pub struct GitDiffTool;

#[async_trait::async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Get the git diff showing detailed changes in the workspace. Without arguments, shows all unstaged changes. Optionally diff a specific file or show staged changes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to workspace root to diff (default: all files)"
                },
                "staged": {
                    "type": "boolean",
                    "description": "Show staged (cached) changes instead of unstaged (default: false)"
                }
            },
            "required": []
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<String> {
        let path = args.get("path").and_then(|v| v.as_str());
        let staged = args.get("staged").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut cmd = piki_core::shell_env::command("git");
        cmd.arg("diff");
        if staged {
            cmd.arg("--cached");
        }
        cmd.arg("--no-color");
        if let Some(p) = path {
            cmd.arg("--").arg(p);
        }
        cmd.current_dir(&ctx.workspace_path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let output = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            let scope = if staged { "staged" } else { "unstaged" };
            return Ok(format!("No {scope} changes{}", path.map_or(String::new(), |p| format!(" in {p}"))));
        }

        // Truncate very large diffs
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() > 300 {
            let truncated: String = lines[..300].join("\n");
            Ok(format!("{truncated}\n\n... ({} total lines, showing first 300)", lines.len()))
        } else {
            Ok(stdout.to_string())
        }
    }
}
