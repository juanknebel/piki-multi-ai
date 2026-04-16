use crate::context::ToolContext;

/// Build an enriched system prompt with workspace context and tool descriptions.
pub async fn build_system_prompt(
    user_prompt: Option<&str>,
    ctx: &ToolContext,
    tool_names: &[&str],
) -> String {
    let mut parts = Vec::new();

    // User's custom system prompt
    if let Some(prompt) = user_prompt
        && !prompt.is_empty()
    {
        parts.push(prompt.to_string());
    }

    // Workspace context
    let mut ws_context = format!("Current workspace: {}", ctx.workspace_path.display());

    // Git branch
    if let Ok(output) = tokio::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&ctx.workspace_path)
        .output()
        .await
    {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            ws_context.push_str(&format!("\nGit branch: {branch}"));
        }
    }

    // Changed files summary
    if let Ok(files) = piki_core::git::get_changed_files(&ctx.workspace_path).await
        && !files.is_empty()
    {
        let count = files.len();
        ws_context.push_str(&format!("\nChanged files: {count}"));
        for f in files.iter().take(10) {
            ws_context.push_str(&format!("\n  {} {}", format_status(&f.status), f.path));
        }
        if count > 10 {
            ws_context.push_str(&format!("\n  ... and {} more", count - 10));
        }
    }
    parts.push(ws_context);

    // Available tools
    if !tool_names.is_empty() {
        let tools_desc = format!(
            "You have access to the following tools: {}. Use them when the user's question requires inspecting code, files, or git state. Call tools as needed, then provide a final answer.",
            tool_names.join(", ")
        );
        parts.push(tools_desc);
    }

    parts.join("\n\n")
}

fn format_status(s: &piki_core::domain::FileStatus) -> &'static str {
    match s {
        piki_core::domain::FileStatus::Modified => "M",
        piki_core::domain::FileStatus::Added => "A",
        piki_core::domain::FileStatus::Deleted => "D",
        piki_core::domain::FileStatus::Renamed => "R",
        piki_core::domain::FileStatus::Untracked => "?",
        piki_core::domain::FileStatus::Conflicted => "U",
        piki_core::domain::FileStatus::Staged => "S",
        piki_core::domain::FileStatus::StagedModified => "SM",
    }
}
