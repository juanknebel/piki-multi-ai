use crate::context::ToolContext;
use super::Tool;

pub struct SearchCodeTool;

#[async_trait::async_trait]
impl Tool for SearchCodeTool {
    fn name(&self) -> &str {
        "search_code"
    }

    fn description(&self) -> &str {
        "Search for a pattern in the workspace files using grep. Returns matching lines with file paths and line numbers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (regular expression)"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (relative to workspace root, default: entire workspace)"
                },
                "glob": {
                    "type": "string",
                    "description": "File glob pattern to filter (e.g. '*.rs', '*.ts')"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pattern"))?;
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let glob = args.get("glob").and_then(|v| v.as_str());

        let search_dir = ctx.workspace_path.join(path);

        let mut cmd = piki_core::shell_env::command("grep");
        cmd.arg("-rn")
            .arg("--color=never")
            .arg("-E")
            .arg(pattern);

        if let Some(g) = glob {
            cmd.arg("--include").arg(g);
        }

        cmd.arg(&search_dir);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let output = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Limit output to 50 matches
        let lines: Vec<&str> = stdout.lines().take(50).collect();
        let total_matches = stdout.lines().count();

        if lines.is_empty() {
            return Ok(format!("No matches found for pattern: {pattern}"));
        }

        // Strip the workspace path prefix from results for readability
        let ws_prefix = ctx.workspace_path.to_string_lossy();
        let mut out = String::new();
        for line in &lines {
            let cleaned = line
                .strip_prefix(ws_prefix.as_ref())
                .and_then(|l| l.strip_prefix('/'))
                .unwrap_or(line);
            out.push_str(cleaned);
            out.push('\n');
        }

        if total_matches > 50 {
            out.push_str(&format!("\n... ({total_matches} total matches, showing first 50)"));
        }

        Ok(out)
    }
}
