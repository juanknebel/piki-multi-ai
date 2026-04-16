use crate::context::ToolContext;
use super::Tool;

pub struct ListFilesTool;

#[async_trait::async_trait]
impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List files and directories in the workspace. Without arguments, lists the workspace root."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path relative to the workspace root (default: root)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List files recursively (default: false, max 500 entries)"
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
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);

        let target = ctx.workspace_path.join(path_str);

        if !target.is_dir() {
            anyhow::bail!("Not a directory: {}", path_str);
        }

        let mut entries = Vec::new();
        let max_entries = 500;

        if recursive {
            collect_recursive(&target, &ctx.workspace_path, &mut entries, max_entries).await?;
        } else {
            let mut dir = tokio::fs::read_dir(&target).await?;
            while let Some(entry) = dir.next_entry().await? {
                if entries.len() >= max_entries {
                    entries.push("... (truncated)".to_string());
                    break;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let meta = entry.metadata().await?;
                let suffix = if meta.is_dir() { "/" } else { "" };
                let rel = entry
                    .path()
                    .strip_prefix(&ctx.workspace_path)
                    .unwrap_or(&entry.path())
                    .to_string_lossy()
                    .to_string();
                entries.push(format!("{rel}{suffix}  ({} bytes)", meta.len()));
                // Skip hidden files starting with .git
                if name.starts_with(".git") {
                    continue;
                }
            }
        }

        entries.sort();
        Ok(entries.join("\n"))
    }
}

async fn collect_recursive(
    dir: &std::path::Path,
    workspace: &std::path::Path,
    entries: &mut Vec<String>,
    max: usize,
) -> anyhow::Result<()> {
    let mut read_dir = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        if entries.len() >= max {
            entries.push("... (truncated)".to_string());
            return Ok(());
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip .git directories
        if name == ".git" {
            continue;
        }
        let meta = entry.metadata().await?;
        let rel = entry
            .path()
            .strip_prefix(workspace)
            .unwrap_or(&entry.path())
            .to_string_lossy()
            .to_string();
        if meta.is_dir() {
            entries.push(format!("{rel}/"));
            Box::pin(collect_recursive(&entry.path(), workspace, entries, max)).await?;
        } else {
            entries.push(rel);
        }
    }
    Ok(())
}
