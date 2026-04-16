use crate::context::ToolContext;
use super::Tool;

pub struct ShellTool;

/// Commands that are never allowed, even with approval.
const DENY_LIST: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=",
    "> /dev/sd",
    ":(){ :|:& };:",
];

#[async_trait::async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace directory. Use for build commands, running tests, installing dependencies, or other operations not covered by the other tools. The command runs with the workspace as the working directory."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                }
            },
            "required": ["command"]
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
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;

        // Check deny list
        for denied in DENY_LIST {
            if command.contains(denied) {
                anyhow::bail!("Command denied: contains blocked pattern '{denied}'");
            }
        }

        let mut cmd = piki_core::shell_env::command("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(&ctx.workspace_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after 60 seconds"))?
        .map_err(|e| anyhow::anyhow!("Failed to execute command: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = format!("Exit code: {exit_code}\n");

        if !stdout.is_empty() {
            // Truncate long output
            let lines: Vec<&str> = stdout.lines().collect();
            if lines.len() > 100 {
                result.push_str("stdout:\n");
                result.push_str(&lines[..100].join("\n"));
                result.push_str(&format!("\n... ({} total lines, showing first 100)", lines.len()));
            } else {
                result.push_str("stdout:\n");
                result.push_str(&stdout);
            }
        }

        if !stderr.is_empty() {
            let lines: Vec<&str> = stderr.lines().collect();
            if lines.len() > 50 {
                result.push_str("\nstderr:\n");
                result.push_str(&lines[..50].join("\n"));
                result.push_str(&format!("\n... ({} total lines)", lines.len()));
            } else {
                result.push_str("\nstderr:\n");
                result.push_str(&stderr);
            }
        }

        Ok(result)
    }
}
