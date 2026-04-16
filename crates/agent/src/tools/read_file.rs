use crate::context::ToolContext;
use super::Tool;

pub struct ReadFileTool;

#[async_trait::async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file in the workspace. Returns the file content with line numbers. Use offset and limit to read specific portions of large files."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to the workspace root"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (0-based, default: 0)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: 200)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(200) as usize;

        let full_path = ctx.workspace_path.join(path_str);
        validate_path(&full_path, &ctx.workspace_path)?;

        let content = tokio::fs::read_to_string(&full_path).await.map_err(|e| {
            anyhow::anyhow!("Cannot read {}: {e}", path_str)
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let start = offset.min(total);
        let end = (start + limit).min(total);

        let mut out = format!("File: {} ({} lines total, showing {}-{})\n", path_str, total, start + 1, end);
        for (i, line) in lines[start..end].iter().enumerate() {
            out.push_str(&format!("{:>4} | {}\n", start + i + 1, line));
        }
        Ok(out)
    }
}

/// Validate that a path resolves within the workspace (no path traversal).
fn validate_path(path: &std::path::Path, workspace: &std::path::Path) -> anyhow::Result<()> {
    // Normalize by joining with workspace
    let normalized = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    };

    // Check that the path doesn't escape the workspace via ..
    // We check the string representation since the file may not exist yet
    let ws_str = workspace.to_string_lossy();
    let norm_str = normalized.to_string_lossy();

    // Simple check: after joining, the path should still start with the workspace
    if !norm_str.starts_with(ws_str.as_ref()) {
        anyhow::bail!("Path escapes workspace: {}", norm_str);
    }

    // Also check for .. components
    for component in normalized.components() {
        if let std::path::Component::ParentDir = component {
            anyhow::bail!("Path contains '..' which may escape workspace");
        }
    }

    Ok(())
}
