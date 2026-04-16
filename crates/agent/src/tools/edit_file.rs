use crate::context::ToolContext;
use super::Tool;

pub struct EditFileTool;

#[async_trait::async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file in the workspace using search-and-replace. Provide the old text to find and the new text to replace it with. The old_text must match exactly (including whitespace). For creating new files, use old_text as empty string."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to the workspace root"
                },
                "old_text": {
                    "type": "string",
                    "description": "Exact text to find in the file (empty string to create a new file)"
                },
                "new_text": {
                    "type": "string",
                    "description": "Text to replace old_text with"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn requires_approval(&self) -> bool {
        true
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
        let old_text = args
            .get("old_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_text"))?;
        let new_text = args
            .get("new_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_text"))?;

        let full_path = ctx.workspace_path.join(path_str);
        validate_within_workspace(&full_path, &ctx.workspace_path)?;

        if old_text.is_empty() {
            // Create new file
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&full_path, new_text).await?;
            return Ok(format!("Created {path_str} ({} bytes)", new_text.len()));
        }

        // Read existing file
        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| anyhow::anyhow!("Cannot read {path_str}: {e}"))?;

        // Find and replace
        let count = content.matches(old_text).count();
        if count == 0 {
            anyhow::bail!("old_text not found in {path_str}");
        }
        if count > 1 {
            anyhow::bail!("old_text found {count} times in {path_str} — must be unique");
        }

        let updated = content.replacen(old_text, new_text, 1);
        tokio::fs::write(&full_path, &updated).await?;

        Ok(format!(
            "Edited {path_str}: replaced {} bytes with {} bytes",
            old_text.len(),
            new_text.len()
        ))
    }
}

fn validate_within_workspace(
    path: &std::path::Path,
    workspace: &std::path::Path,
) -> anyhow::Result<()> {
    let normalized = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    };
    for component in normalized.components() {
        if let std::path::Component::ParentDir = component {
            anyhow::bail!("Path contains '..' which may escape workspace");
        }
    }
    let ws_str = workspace.to_string_lossy();
    let norm_str = normalized.to_string_lossy();
    if !norm_str.starts_with(ws_str.as_ref()) {
        anyhow::bail!("Path escapes workspace: {norm_str}");
    }
    Ok(())
}
