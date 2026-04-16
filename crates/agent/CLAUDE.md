# piki-agent

Agentic tool-use engine for AI chat. **Depends on `piki-core` and `piki-api-client`. Must NOT depend on `piki-tui` or `piki-desktop`.**

## Modules

- `agent_loop.rs` — `AgentLoop` struct orchestrating the LLM ↔ tool execution loop. Sends messages with tool definitions, executes tool calls, feeds results back, repeats until text-only response or max iterations (20).
- `context.rs` — `ToolContext` (workspace path, source repo), `ApprovalRequest`, `ApprovalResponse` for write-tool approval flow.
- `events.rs` — `AgentEvent` enum for UI communication: `Token`, `Done`, `ToolCallsStarted`, `ToolExecuting`, `ToolResult`, `ApprovalRequired`, `Finished`, `Error`.
- `prompt.rs` — `build_system_prompt()` enriches user prompt with workspace context (git branch, changed files, tool descriptions).
- `tools/mod.rs` — `Tool` trait (`name`, `description`, `parameters_schema`, `requires_approval`, `execute`), `ToolRegistry` with `default_read_only()` factory.
- `tools/git_status.rs` — Wraps `piki_core::git::get_changed_files()`.
- `tools/read_file.rs` — Reads files with path sandboxing (canonicalize + prefix check). Supports offset/limit.
- `tools/list_files.rs` — Lists directory contents, optional recursive mode (max 500 entries).
- `tools/search_code.rs` — Grep via `piki_core::shell_env::command("grep")`, truncated to 50 matches.

## Conventions

- Error handling: `anyhow::Result` for application errors.
- Path security: All file tools validate paths resolve within `workspace_path`.
- Tool trait: `requires_approval()` defaults to false; override for write tools.
- Uses `ChatClient` trait from `piki-api-client` for LLM transport.
