export type AIProvider =
  | "Claude"
  | "Gemini"
  | "OpenCode"
  | "Kilo"
  | "Codex"
  | "Shell"
  | "Kanban"
  | "CodeReview"
  | "Api";

export type WorkspaceStatus = "Idle" | "Busy" | "Done" | { Error: string };

export type WorkspaceType = "Worktree" | "Simple" | "Project";

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

export interface WorkspaceInfo {
  name: string;
  description: string;
  prompt: string;
  kanban_path: string | null;
  branch: string;
  path: string;
  source_repo: string;
  source_repo_display: string;
  workspace_type: WorkspaceType;
  group: string | null;
  order: number;
  dispatch_card_id: string | null;
  dispatch_source_kanban: string | null;
  dispatch_agent_name: string | null;
}

export interface TabInfo {
  id: string;
  provider: AIProvider;
  alive: boolean;
}

export interface WorkspaceDetail {
  info: WorkspaceInfo;
  status: WorkspaceStatus;
  changed_files: ChangedFile[];
  ahead_behind: [number, number] | null;
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
}

export interface ToastEvent {
  message: string;
  level: "info" | "success" | "error";
}

export const PROVIDER_LABELS: Record<AIProvider, string> = {
  Claude: "Claude Code",
  Gemini: "Gemini",
  OpenCode: "OpenCode",
  Kilo: "Kilo",
  Codex: "Codex",
  Shell: "Shell",
  Kanban: "Kanban Board",
  CodeReview: "Code Review",
  Api: "API Explorer",
};

export const PROVIDER_ICONS: Record<AIProvider, string> = {
  Claude: "C",
  Gemini: "G",
  OpenCode: "O",
  Kilo: "K",
  Codex: "X",
  Shell: "$",
  Kanban: "B",
  CodeReview: "R",
  Api: "A",
};

// Kanban types
export interface KanbanCard {
  id: string;
  title: string;
  description: string;
  priority: string;
  assignee: string;
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
