use crate::context::ToolContext;

/// Maximum characters for the project context snippet.
const MAX_SNIPPET_CHARS: usize = 800;

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

    // Project type detection
    let project_type = detect_project_type(&ctx.workspace_path).await;
    if !project_type.is_empty() {
        ws_context.push_str(&format!("\nProject type: {project_type}"));
    }

    // Changed files summary
    if let Ok(files) = piki_core::git::get_changed_files(&ctx.workspace_path).await
        && !files.is_empty()
    {
        let count = files.len();
        ws_context.push_str(&format!("\nChanged files: {count}"));
        for f in files.iter().take(15) {
            ws_context.push_str(&format!("\n  {} {}", format_status(&f.status), f.path));
        }
        if count > 15 {
            ws_context.push_str(&format!("\n  ... and {} more", count - 15));
        }
    }
    parts.push(ws_context);

    // Project documentation snippet (CLAUDE.md > README.md)
    if let Some(snippet) = read_project_snippet(&ctx.workspace_path).await {
        parts.push(format!("Project documentation:\n{snippet}"));
    }

    // Available tools with descriptions
    if !tool_names.is_empty() {
        let tools_section = build_tools_section(tool_names);
        parts.push(tools_section);
    }

    // Agent behavior instructions
    parts.push(
        "Instructions: You are an AI assistant with access to workspace tools. \
         Use tools to gather information before answering. \
         Be concise in tool usage — prefer specific file reads over broad searches. \
         After gathering enough context, provide a clear final answer. \
         If a tool fails, explain the error and try an alternative approach."
            .to_string(),
    );

    parts.join("\n\n")
}

/// Detect project type from config files present in the workspace.
async fn detect_project_type(workspace: &std::path::Path) -> String {
    let mut types = Vec::new();
    let checks = [
        ("Cargo.toml", "Rust (Cargo)"),
        ("package.json", "JavaScript/TypeScript (npm)"),
        ("go.mod", "Go"),
        ("pyproject.toml", "Python"),
        ("Makefile", "Make"),
        ("CMakeLists.txt", "C/C++ (CMake)"),
        ("pom.xml", "Java (Maven)"),
        ("build.gradle", "Java (Gradle)"),
    ];

    for (file, label) in checks {
        if workspace.join(file).exists() {
            types.push(label);
        }
    }

    types.join(", ")
}

/// Read a project documentation snippet from CLAUDE.md or README.md.
async fn read_project_snippet(workspace: &std::path::Path) -> Option<String> {
    // Prefer CLAUDE.md (project-specific instructions)
    for name in ["CLAUDE.md", "README.md"] {
        let path = workspace.join(name);
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            if content.is_empty() {
                continue;
            }
            // Truncate to keep prompt reasonable
            let truncated = if content.len() > MAX_SNIPPET_CHARS {
                format!("{}...\n(truncated from {})", &content[..MAX_SNIPPET_CHARS], name)
            } else {
                content
            };
            return Some(truncated);
        }
    }
    None
}

/// Build the tools section with categorized descriptions.
fn build_tools_section(tool_names: &[&str]) -> String {
    let write_set = ["edit_file", "shell"];

    let read_tools: Vec<&str> = tool_names
        .iter()
        .filter(|n| !write_set.contains(n))
        .copied()
        .collect();
    let write_tools: Vec<&str> = tool_names
        .iter()
        .filter(|n| write_set.contains(n))
        .copied()
        .collect();

    let mut section = String::from("Available tools:");

    if !read_tools.is_empty() {
        section.push_str("\n  Read: ");
        section.push_str(&read_tools.join(", "));
    }
    if !write_tools.is_empty() {
        section.push_str("\n  Write (requires approval): ");
        section.push_str(&write_tools.join(", "));
    }

    section.push_str(
        "\nUse read tools freely. Write tools (edit_file, shell) require user approval before execution.",
    );

    section
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
