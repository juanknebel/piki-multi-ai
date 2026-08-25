import type { IconName } from "./components/icons";

export type AIProvider =
  | "Shell"
  | "Kanban"
  | "CodeReview"
  | "Api"
  | "Markdown"
  | "CodeEditor"
  | "WebPreview"
  | { Custom: string };

export type WorkspaceStatus = "Idle" | "Busy" | "Done" | { Error: string };

export type WorkspaceType = "Worktree" | "Simple" | "Project";

/** Where a workspace's files originated. Drives source-control visibility
 *  and the "Create Worktree" action (GitHub-only). Mirrors
 *  `piki_core::WorkspaceOrigin`. */
export type WorkspaceOrigin =
  | { kind: "Local" }
  | { kind: "GitHub"; url: string };

export type FileStatus =
  | "Modified"
  | "Added"
  | "Deleted"
  | "Renamed"
  | "Untracked"
  | "Conflicted"
  | "Staged"
  | "StagedModified";

export interface ChangedFile {
  path: string;
  status: FileStatus;
}

/** Mirrors `piki_core::EntryKind`. */
export type EntryKind = "File" | "Dir" | "Symlink";

/** One level of a directory listing. Mirrors `piki_core::DirEntry`. */
export interface DirEntry {
  name: string;
  kind: EntryKind;
  size: number;
  /** Milliseconds since the Unix epoch; 0 if unavailable. */
  mtime: number;
}

export interface WorkspaceInfo {
  name: string;
  description: string;
  prompt: string;
  kanban_path: string | null;
  path: string;
  source_repo: string;
  source_repo_display: string;
  workspace_type: WorkspaceType;
  order: number;
  dispatch_card_id: string | null;
  dispatch_source_kanban: string | null;
  dispatch_agent_name: string | null;
  origin: WorkspaceOrigin;
}

export interface TabInfo {
  id: string;
  provider: AIProvider;
  alive: boolean;
  custom_title?: string | null;
}

export interface WorkspaceDetail {
  info: WorkspaceInfo;
  status: WorkspaceStatus;
  changed_files: ChangedFile[];
  ahead_behind: [number, number] | null;
  /** Current git branch, refreshed in the background. `null` until the
   * first refresh completes, or if the workspace isn't a git repo. */
  branch: string | null;
  tabs: TabInfo[];
  active_tab: number;
}

export interface PtyOutputEvent {
  tab_id: string;
  data: string; // base64
}

export interface PtyExitEvent {
  tab_id: string;
  exit_code: number | null;
}

export interface GitRefreshEvent {
  workspace_idx: number;
  files: ChangedFile[];
  ahead_behind: [number, number] | null;
  branch: string | null;
}

export interface ToastEvent {
  message: string;
  level: "info" | "success" | "error";
}

/** Shell-integration events extracted from PTY OSC 133/7 markers. */
export type PtyShellEventKind =
  | "prompt-start"
  | "command-input-start"
  | "command-output-start"
  | "command-end"
  | "cwd-changed";

export interface PtyShellEvent {
  tab_id: string;
  kind: PtyShellEventKind;
  exit_code?: number;
  cwd?: string;
}

/** Coarse status of a Claude Code agent tab, derived from its structured
 *  OSC 777 lifecycle events. */
export type CliAgentStatus =
  | "running"
  | "waiting-permission"
  | "idle"
  | "done";

/** A structured Claude Code lifecycle event (Warp-style, delivered in-band
 *  via OSC 777). Drives the per-tab agent status glyph + summary. */
export interface PtyAgentEvent {
  tab_id: string;
  status: CliAgentStatus;
  /** cli-agent event name: `session_start`, `prompt_submit`,
   *  `tool_complete`, `permission_request`, `notification`, `stop`. */
  kind: string;
  summary?: string;
  /** The tab has news the user hasn't looked at. Already false when the
   *  event landed on the tab on screen (that counts as seen); cleared
   *  later by a `pty-agent-ack`. */
  attention: boolean;
}

/** The backend cleared a tab's agent attention marker — the user is looking
 *  at it (tab switch, workspace switch, or news landing on the visible
 *  tab). Mirrors `events::PtyAgentAckPayload`. */
export interface PtyAgentAckEvent {
  tab_id: string;
}

/** Compact elapsed label — mirror of `piki_core::cli_agent::format_elapsed`
 *  (`45s`, `3m 12s`, `1h 02m`); the TUI Agents pane shows the same. */
export function formatElapsed(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  const pad = (n: number) => String(n).padStart(2, "0");
  if (h > 0) return `${h}h ${pad(m)}m`;
  if (m > 0) return `${m}m ${pad(s)}s`;
  return `${s}s`;
}

/** Icon / label / theme color for a Claude agent status. Shared by the
 *  status bar (full), the workspace tab bar (dot only) and the Agents panel.
 *  `attention` gates the shouting: an idle agent only reads "needs you" when
 *  it has news the user hasn't looked at (same rule as the TUI). The view
 *  returns the icon NAME (`components/icons.ts`); renderers call `icon()`. */
export interface AgentStatusView {
  icon: IconName;
  label: string;
  color: string;
}

export function cliAgentStatusView(status: CliAgentStatus, attention = false): AgentStatusView {
  switch (status) {
    case "waiting-permission":
      return { icon: "warning", label: "needs permission", color: "var(--accent-warm)" };
    case "idle":
      return attention
        ? { icon: "dot", label: "needs you", color: "var(--accent-warm)" }
        : { icon: "clock", label: "waiting for input", color: "var(--accent-primary)" };
    case "done":
      return { icon: "check", label: "done", color: "var(--git-added)" };
    case "running":
    default:
      return { icon: "play", label: "running", color: "var(--text-muted)" };
  }
}

/** Mirror of `piki_core::cli_agent::status_severity` — precedence for an
 *  agent (status, attention) pair, worst first: needs-permission > unseen
 *  news > running > everything else. Keep in sync with core. */
export function agentStatusSeverity(status: CliAgentStatus, attention: boolean): number {
  if (status === "waiting-permission") return 4;
  if ((status === "idle" || status === "done") && attention) return 3;
  if (status === "running") return 2;
  return 0;
}

/** Status icon for ambient chrome (workspace list rollup). Only actionable
 *  states surface here — running/done stay in the Agents panel. Mirrors the
 *  TUI's `actionable_status_view`. */
export function actionableStatusView(
  status: CliAgentStatus,
  attention: boolean,
): AgentStatusView | null {
  if (status === "waiting-permission")
    return { icon: "warning", label: "needs permission", color: "var(--accent-warm)" };
  // "Has news you haven't seen" propagates; quiet idle/done doesn't.
  if ((status === "idle" || status === "done") && attention)
    return { icon: "dot", label: "needs you", color: "var(--accent-warm)" };
  return null;
}

/** One row in the Agents sidebar panel — a (workspace, tab) running an AI
 *  agent, across ALL workspaces. Mirrors `commands::agents::AgentRow`. */
export interface AgentRow {
  workspace_idx: number;
  workspace_name: string;
  /** Index into that workspace's tab list — feed to `setActiveTab` to jump. */
  tab_idx: number;
  tab_id: string;
  label: string;
  alive: boolean;
  status: CliAgentStatus | null;
  attention: boolean;
  summary: string | null;
  /** Seconds since the run began (session start / last prompt), `null`
   *  once stopped. Snapshot at fetch time — see `liveElapsedSecs`. */
  elapsed_secs: number | null;
}

/** One row of the Sessions dialog. Mirrors `SessionRow` in commands/session.rs. */
export interface SessionRow {
  id: string;
  name: string;
  workspace: string;
  state: "attached" | "detached" | "exited";
  attached: number;
  exit_code: number | null;
  local_workspace_idx: number | null;
  local_tab_idx: number | null;
  /** Loaded workspace matching the session's recorded path — default Adopt target. */
  workspace_idx: number | null;
}

/** Status-bar segment: daemon health + live session count (all clients). */
export interface SessionStatus {
  state: "on" | "off" | "unavailable";
  live: number;
  daemon_pid: number | null;
}

/** What startup re-attach restored. */
export interface RestoreSummary {
  sessions: number;
  workspaces: number[];
}

/** Live tabs by what quitting does to them. */
export interface QuitSummary {
  persistent: number;
  local: number;
}

export interface SessionsSnapshot {
  connected: boolean;
  daemon_pid: number | null;
  sessions: SessionRow[];
  error: string | null;
}

/** Workspace-level "needs attention" signal. Sources: `provider-idle` (a
 *  provider tab fell silent), `shell-command-end` (a shell tab finished a
 *  command — emitted by the frontend when it observes a `pty-shell-event` of
 *  kind `command-end`). */
export interface PtyAttentionEvent {
  workspace_idx: number;
  tab_id: string;
  source: "provider-idle" | "shell-command-end" | "cli-agent";
}

// Built-in provider labels (Custom providers use their name)
const BUILTIN_PROVIDER_LABELS: Record<string, string> = {
  Shell: "Shell",
  Kanban: "Kanban Board",
  CodeReview: "Code Review",
  Api: "API Explorer",
  Markdown: "Markdown",
  CodeEditor: "Code Editor",
  WebPreview: "Web Preview",
};

const BUILTIN_PROVIDER_ICONS: Record<string, string> = {
  Shell: "$",
  Kanban: "B",
  CodeReview: "R",
  Api: "A",
  Markdown: "M",
  CodeEditor: "E",
  WebPreview: "W",
};

/** Get the display label for any provider (built-in or custom). */
export function getProviderLabel(provider: AIProvider): string {
  if (typeof provider === "string") {
    return BUILTIN_PROVIDER_LABELS[provider] ?? provider;
  }
  return provider.Custom;
}

/** Display label for a tab: custom_title > terminal title > provider label.
 *  `termTitle` is what the program set via OSC 0/2 (`TabShellState.title`,
 *  xterm `onTitleChange`) — a fallback only: a user rename always wins, and
 *  an empty/whitespace title is ignored. */
export function getTabLabel(tab: TabInfo, termTitle?: string | null): string {
  if (tab.custom_title && tab.custom_title.trim().length > 0) return tab.custom_title;
  if (termTitle && termTitle.trim().length > 0) return termTitle.trim();
  return getProviderLabel(tab.provider);
}

/** Get the icon character for any provider (built-in or custom). */
export function getProviderIcon(provider: AIProvider): string {
  if (typeof provider === "string") {
    return BUILTIN_PROVIDER_ICONS[provider] ?? provider.charAt(0).toUpperCase();
  }
  return provider.Custom.charAt(0).toUpperCase();
}

/** Get the provider key for serialization (built-in name or Custom wrapper). */
export function getProviderKey(provider: AIProvider): string {
  if (typeof provider === "string") {
    return provider;
  }
  return provider.Custom;
}

/** User-configurable provider from providers.toml */
export interface ProviderInfo {
  name: string;
  description: string;
  command: string;
  dispatchable: boolean;
  agent_dir: string | null;
}

// Keep old exports for backwards compatibility with existing code that uses them
export const PROVIDER_LABELS = BUILTIN_PROVIDER_LABELS;
export const PROVIDER_ICONS = BUILTIN_PROVIDER_ICONS;

// Kanban types
export interface KanbanCard {
  id: string;
  title: string;
  description: string;
  priority: string;
  assignee: string;
  project: string;
}

export interface KanbanColumn {
  id: string;
  cards: KanbanCard[];
}

export interface KanbanBoard {
  columns: KanbanColumn[];
}

export const PRIORITY_CSS: Record<string, string> = {
  Bug: "priority-bug",
  High: "priority-high",
  Medium: "priority-medium",
  Low: "priority-low",
  Wishlist: "priority-wishlist",
};

export const FILE_STATUS_LABELS: Record<FileStatus, string> = {
  Modified: "M",
  Added: "A",
  Deleted: "D",
  Renamed: "R",
  Untracked: "?",
  Conflicted: "C",
  Staged: "S",
  StagedModified: "SM",
};

// ── Chat types ─────────────────────────────────────

export interface ChatMessage {
  role: "System" | "User" | "Assistant" | "Tool";
  content: string;
}

export type ChatServerType = "Ollama" | "LlamaCpp";

export interface ChatConfig {
  provider: string;
  server_type: ChatServerType;
  model: string;
  base_url: string;
  system_prompt: string | null;
}

export interface ChatModelInfo {
  name: string;
  size: number;
  modified_at: string;
}

export interface ChatTokenEvent {
  content: string;
  done: boolean;
}

export const FILE_STATUS_CSS: Record<FileStatus, string> = {
  Modified: "modified",
  Added: "added",
  Deleted: "deleted",
  Renamed: "renamed",
  Untracked: "untracked",
  Conflicted: "conflicted",
  Staged: "staged",
  StagedModified: "staged-modified",
};
