# piki-agent

Agentic tool-use engine for AI chat. **Depends on `piki-core` and `piki-api-client`. Must NOT depend on `piki-tui` or `piki-desktop`.**

## Modules

- `agent_loop.rs` — `AgentLoop` struct orchestrating the LLM ↔ tool execution loop. Sends messages with tool definitions, executes tool calls, feeds results back, repeats until text-only response or max iterations (20).
- `context.rs` — `ToolContext` (workspace path, source repo), `ApprovalRequest`, `ApprovalResponse` for write-tool approval flow.
- `events.rs` — `AgentEvent` enum for UI communication: `Token`, `Done`, `ToolCallsStarted`, `ToolExecuting`, `ToolResult`, `ApprovalRequired`, `Finished`, `Error`.
- `prompt.rs` — `build_system_prompt()` enriches user prompt with workspace context (git branch, changed files, tool descriptions).
- `tools/mod.rs` — `Tool` trait (`name`, `description`, `parameters_schema`, `requires_approval`, `execute`), `ToolRegistry` with `default_read_only()` and `default_all()` factories.
- `tools/git_status.rs` — Wraps `piki_core::git::get_changed_files()`.
- `tools/git_diff.rs` — Runs `git diff` with optional `--cached` and per-file filtering. Truncates at 300 lines.
- `tools/read_file.rs` — Reads files with path sandboxing (canonicalize + prefix check). Supports offset/limit.
- `tools/list_files.rs` — Lists directory contents, optional recursive mode (max 500 entries).
- `tools/search_code.rs` — Grep via `piki_core::shell_env::command("grep")`, truncated to 50 matches.
- `tools/edit_file.rs` — Search-and-replace file editing (`requires_approval = true`). Creates new files when `old_text` is empty. Validates unique match.
- `tools/shell.rs` — Execute shell commands (`requires_approval = true`). 60s timeout. Deny-list for dangerous patterns. Truncated output.

## Conventions

- Error handling: `anyhow::Result` for application errors.
- Path security: All file tools validate paths resolve within `workspace_path`.
- Tool trait: `requires_approval()` defaults to false; override for write tools.
- Uses `ChatClient` trait from `piki-api-client` for LLM transport.
