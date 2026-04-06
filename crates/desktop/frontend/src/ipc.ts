import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  WorkspaceInfo,
  WorkspaceDetail,
  ChangedFile,
  PtyOutputEvent,
  PtyExitEvent,
  GitRefreshEvent,
  ToastEvent,
  KanbanBoard,
} from "./types";

// Workspace commands
export function listWorkspaces(): Promise<WorkspaceInfo[]> {
  return invoke("list_workspaces");
}

export function createWorkspace(
  name: string,
  description: string,
  prompt: string,
  dir: string,
  wsType: string,
  group: string | null,
  kanbanPath: string | null = null,
): Promise<WorkspaceInfo> {
  return invoke("create_workspace", {
    name,
    description,
    prompt,
    dir,
    wsType,
    group,
    kanbanPath,
  });
}

export function deleteWorkspace(index: number): Promise<void> {
  return invoke("delete_workspace", { index });
}

export function updateWorkspace(
  index: number,
  prompt?: string,
  group?: string,
  description?: string,
  kanbanPath?: string,
): Promise<void> {
  return invoke("update_workspace", { index, prompt, group, description, kanbanPath });
}

export function switchWorkspace(index: number): Promise<WorkspaceDetail> {
  return invoke("switch_workspace", { index });
}

// PTY commands
export function spawnTab(
  workspaceIdx: number,
  provider: string,
): Promise<string> {
  return invoke("spawn_tab", { workspaceIdx, provider });
}

export function spawnEditorTab(
  workspaceIdx: number,
  filePath: string,
): Promise<string> {
  return invoke("spawn_editor_tab", { workspaceIdx, filePath });
}

export function writePty(tabId: string, data: string): Promise<void> {
  return invoke("write_pty", { tabId, data });
}

export function resizePty(
  tabId: string,
  rows: number,
  cols: number,
): Promise<void> {
  return invoke("resize_pty", { tabId, rows, cols });
}

export function closeTab(
  workspaceIdx: number,
  tabIdx: number,
): Promise<void> {
  return invoke("close_tab", { workspaceIdx, tabIdx });
}

// Git commands
export function getChangedFiles(
  workspaceIdx: number,
): Promise<ChangedFile[]> {
  return invoke("get_changed_files", { workspaceIdx });
}

export function gitStage(
  workspaceIdx: number,
  filePath: string,
): Promise<void> {
  return invoke("git_stage", { workspaceIdx, filePath });
}

export function gitUnstage(
  workspaceIdx: number,
  filePath: string,
): Promise<void> {
  return invoke("git_unstage", { workspaceIdx, filePath });
}

export function gitCommit(
  workspaceIdx: number,
  message: string,
): Promise<void> {
  return invoke("git_commit", { workspaceIdx, message });
}

export function gitPush(workspaceIdx: number): Promise<void> {
  return invoke("git_push", { workspaceIdx });
}

export interface MergeResult {
  success: boolean;
  message: string;
  conflicts: string[];
}

export function gitMerge(
  workspaceIdx: number,
  strategy: "merge" | "rebase",
): Promise<MergeResult> {
  return invoke("git_merge", { workspaceIdx, strategy });
}

export function gitAbortMerge(workspaceIdx: number): Promise<void> {
  return invoke("git_abort_merge", { workspaceIdx });
}

export function gitResolveConflict(
  workspaceIdx: number,
  filePath: string,
  resolution: "ours" | "theirs" | "staged",
): Promise<void> {
  return invoke("git_resolve_conflict", { workspaceIdx, filePath, resolution });
}

export function gitContinueMerge(workspaceIdx: number): Promise<string> {
  return invoke("git_continue_merge", { workspaceIdx });
}

export function gitStageAll(workspaceIdx: number): Promise<void> {
  return invoke("git_stage_all", { workspaceIdx });
}

export function gitUnstageAll(workspaceIdx: number): Promise<void> {
  return invoke("git_unstage_all", { workspaceIdx });
}

// Diff commands
export interface DiffLine {
  content: string;
  line_type: "add" | "del" | "context" | "header" | "hunk";
}

export interface DiffSide {
  line_num: number;
  content: string;
}

export interface DiffPair {
  left: DiffSide | null;
  right: DiffSide | null;
  pair_type: "context" | "modified" | "added" | "deleted";
}

export interface DiffHunk {
  header: string;
  pairs: DiffPair[];
}

export interface SideBySideDiff {
  left_title: string;
  right_title: string;
  file_path: string;
  hunks: DiffHunk[];
  stats: { additions: number; deletions: number };
}

export interface ConflictRegion {
  region_type: "common" | "conflict";
  ours_lines: string[];
  theirs_lines: string[];
  base_lines: string[];
}

export interface ConflictDiff {
  file_path: string;
  ours_title: string;
  theirs_title: string;
  regions: ConflictRegion[];
}

export function getFileDiff(
  workspaceIdx: number,
  filePath: string,
  staged: boolean,
): Promise<DiffLine[]> {
  return invoke("get_file_diff", { workspaceIdx, filePath, staged });
}

export function getCommitDiff(
  workspaceIdx: number,
  sha: string,
): Promise<DiffLine[]> {
  return invoke("get_commit_diff", { workspaceIdx, sha });
}

export function getSideBySideDiff(
  workspaceIdx: number,
  filePath: string,
  staged: boolean,
): Promise<SideBySideDiff> {
  return invoke("get_side_by_side_diff", { workspaceIdx, filePath, staged });
}

export function getCommitSideBySideDiff(
  workspaceIdx: number,
  sha: string,
): Promise<SideBySideDiff[]> {
  return invoke("get_commit_side_by_side_diff", { workspaceIdx, sha });
}

export function getConflictDiff(
  workspaceIdx: number,
  filePath: string,
): Promise<ConflictDiff> {
  return invoke("get_conflict_diff", { workspaceIdx, filePath });
}

// Git log commands
export interface GitLogEntry {
  sha: string | null;
  line: string;
}

export function getGitLog(workspaceIdx: number): Promise<GitLogEntry[]> {
  return invoke("get_git_log", { workspaceIdx });
}

// Stash commands
export interface StashEntry {
  index: number;
  id: string;
  message: string;
}

export function gitStashList(workspaceIdx: number): Promise<StashEntry[]> {
  return invoke("git_stash_list", { workspaceIdx });
}

export function gitStashSave(workspaceIdx: number, message: string): Promise<string> {
  return invoke("git_stash_save", { workspaceIdx, message });
}

export function gitStashPop(workspaceIdx: number, stashIndex: number): Promise<string> {
  return invoke("git_stash_pop", { workspaceIdx, stashIndex });
}

export function gitStashApply(workspaceIdx: number, stashIndex: number): Promise<string> {
  return invoke("git_stash_apply", { workspaceIdx, stashIndex });
}

export function gitStashDrop(workspaceIdx: number, stashIndex: number): Promise<void> {
  return invoke("git_stash_drop", { workspaceIdx, stashIndex });
}

// Search commands
export function fuzzyFileList(workspaceIdx: number): Promise<string[]> {
  return invoke("fuzzy_file_list", { workspaceIdx });
}

export function readFileContent(workspaceIdx: number, path: string): Promise<string> {
  return invoke("read_file_content", { workspaceIdx, path });
}

export function writeFileContent(workspaceIdx: number, path: string, content: string): Promise<void> {
  return invoke("write_file_content", { workspaceIdx, path, content });
}

export interface SearchMatch {
  path: string;
  line_num: number;
  text: string;
}

export function projectSearch(workspaceIdx: number, query: string): Promise<SearchMatch[]> {
  return invoke("project_search", { workspaceIdx, query });
}

// API Explorer commands
export interface ApiResponseResult {
  status: number;
  elapsed_ms: number;
  body: string;
  headers: string;
  method: string;
  url: string;
}

export interface ApiHistoryEntryDto {
  id: number | null;
  created_at: string;
  request_text: string;
  method: string;
  url: string;
  status: number;
  elapsed_ms: number;
  response_body: string;
  response_headers: string;
}

export function sendApiRequest(workspaceIdx: number, requestText: string): Promise<ApiResponseResult[]> {
  return invoke("send_api_request", { workspaceIdx, requestText });
}

export function loadApiHistory(workspaceIdx: number, limit: number): Promise<ApiHistoryEntryDto[]> {
  return invoke("load_api_history", { workspaceIdx, limit });
}

export function searchApiHistory(workspaceIdx: number, query: string, limit: number): Promise<ApiHistoryEntryDto[]> {
  return invoke("search_api_history", { workspaceIdx, query, limit });
}

export function deleteApiHistoryEntry(entryId: number): Promise<void> {
  return invoke("delete_api_history_entry", { entryId });
}

export function jqFilter(input: string, filter: string): Promise<string> {
  return invoke("jq_filter", { input, filter });
}

// Code Review commands
export interface PrInfo {
  number: number;
  title: string;
  body: string;
  state: string;
  review_decision: string | null;
  url: string;
  head_ref_name: string;
  base_ref_name: string;
  additions: number;
  deletions: number;
}

export interface PrFile {
  path: string;
  additions: number;
  deletions: number;
}

export interface PrDetail {
  info: PrInfo;
  files: PrFile[];
}

export function getPrInfo(workspaceIdx: number): Promise<PrDetail | null> {
  return invoke("get_pr_info", { workspaceIdx });
}

export function getPrFileDiff(
  workspaceIdx: number,
  file: string,
  baseRef: string,
): Promise<{ path: string; lines: { line_type: string; content: string; old_line: number | null; new_line: number | null }[] }> {
  return invoke("get_pr_file_diff", { workspaceIdx, file, baseRef });
}

export function submitPrReview(
  workspaceIdx: number,
  prNumber: number,
  verdict: string,
  body: string,
  comments: { path: string; line: number; side: string; body: string }[],
): Promise<string> {
  return invoke("submit_pr_review", { workspaceIdx, prNumber, verdict, body, comments });
}

// Markdown commands
export function readMarkdownFile(workspaceIdx: number, filePath: string): Promise<string> {
  return invoke("read_markdown_file", { workspaceIdx, filePath });
}

// Agent commands
export interface AgentInfo {
  id: number | null;
  name: string;
  provider: string;
  role: string;
  version: number;
  last_synced_at: string | null;
}

export interface ScannedAgent {
  name: string;
  provider: string;
  role: string;
  exists: boolean;
}

export function listAgents(workspaceIdx: number): Promise<AgentInfo[]> {
  return invoke("list_agents", { workspaceIdx });
}

export function saveAgent(
  workspaceIdx: number,
  name: string,
  provider: string,
  role: string,
  id?: number | null,
): Promise<void> {
  return invoke("save_agent", { workspaceIdx, name, provider, role, id });
}

export function deleteAgent(agentId: number): Promise<void> {
  return invoke("delete_agent", { agentId });
}

export function scanRepoAgents(workspaceIdx: number): Promise<ScannedAgent[]> {
  return invoke("scan_repo_agents", { workspaceIdx });
}

export function importAgents(
  workspaceIdx: number,
  agents: { name: string; provider: string; role: string }[],
): Promise<number> {
  return invoke("import_agents", { workspaceIdx, agents });
}

export function dispatchAgent(
  workspaceIdx: number,
  provider: string,
  prompt: string,
  createWorktree: boolean,
  wsName?: string,
  group?: string,
  dispatchCardId?: string,
  dispatchSourceKanban?: string,
  dispatchAgentName?: string,
  dispatchCardTitle?: string,
): Promise<string> {
  return invoke("dispatch_agent", { workspaceIdx, provider, prompt, createWorktree, wsName, group, dispatchCardId, dispatchSourceKanban, dispatchAgentName, dispatchCardTitle });
}

export function kanbanLoadBoardByPath(boardPath: string): Promise<KanbanBoard> {
  return invoke("kanban_load_board_by_path", { boardPath });
}

export function kanbanMoveCardByPath(boardPath: string, cardId: string, toColumnId: string): Promise<void> {
  return invoke("kanban_move_card_by_path", { boardPath, cardId, toColumnId });
}

// Log commands
export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}

export function getLogs(levelFilter?: number): Promise<LogEntry[]> {
  return invoke("get_logs", { levelFilter: levelFilter ?? null });
}

export function clearLogs(): Promise<void> {
  return invoke("clear_logs");
}

// Theme commands
export function getTheme(): Promise<[string | null, string | null]> {
  return invoke("get_theme");
}

export function setTheme(preset: string, overrides: string): Promise<void> {
  return invoke("set_theme", { preset, overrides });
}

// System commands
export function getSysinfo(): Promise<string> {
  return invoke("get_sysinfo");
}

// Kanban commands
export function kanbanLoadBoard(workspaceIdx: number): Promise<KanbanBoard> {
  return invoke("kanban_load_board", { workspaceIdx });
}

export function kanbanCreateCard(workspaceIdx: number, columnId: string): Promise<string> {
  return invoke("kanban_create_card", { workspaceIdx, columnId });
}

export function kanbanUpdateCard(
  workspaceIdx: number,
  cardId: string,
  title: string,
  description: string,
  priority: string,
  assignee: string,
): Promise<void> {
  return invoke("kanban_update_card", { workspaceIdx, cardId, title, description, priority, assignee });
}

export function kanbanMoveCard(workspaceIdx: number, cardId: string, toColumnId: string): Promise<void> {
  return invoke("kanban_move_card", { workspaceIdx, cardId, toColumnId });
}

export function kanbanDeleteCard(workspaceIdx: number, cardId: string): Promise<void> {
  return invoke("kanban_delete_card", { workspaceIdx, cardId });
}

// Settings commands
export function getSettings(): Promise<string | null> {
  return invoke("get_settings");
}

export function setSettings(value: string): Promise<void> {
  return invoke("set_settings", { value });
}

// Event listeners
export function onPtyOutput(
  callback: (event: PtyOutputEvent) => void,
): Promise<UnlistenFn> {
  return listen<PtyOutputEvent>("pty-output", (e) => callback(e.payload));
}

export function onPtyExit(
  callback: (event: PtyExitEvent) => void,
): Promise<UnlistenFn> {
  return listen<PtyExitEvent>("pty-exit", (e) => callback(e.payload));
}

export function onGitRefresh(
  callback: (event: GitRefreshEvent) => void,
): Promise<UnlistenFn> {
  return listen<GitRefreshEvent>("git-refresh", (e) => callback(e.payload));
}

export function onSysinfoUpdate(
  callback: (formatted: string) => void,
): Promise<UnlistenFn> {
  return listen<{ formatted: string }>("sysinfo-update", (e) =>
    callback(e.payload.formatted),
  );
}

export function onToast(
  callback: (event: ToastEvent) => void,
): Promise<UnlistenFn> {
  return listen<ToastEvent>("toast", (e) => callback(e.payload));
}
