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
): Promise<WorkspaceInfo> {
  return invoke("create_workspace", {
    name,
    description,
    prompt,
    dir,
    wsType,
    group,
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
): Promise<void> {
  return invoke("update_workspace", { index, prompt, group, description });
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

// System commands
export function getSysinfo(): Promise<string> {
  return invoke("get_sysinfo");
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
