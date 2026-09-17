// The ONE workspace switch path (sidebar row, switcher, palette, Alt+1…9,
// cycling) plus the two navigation actions built on it. Ordering logic is the
// pure `workspace-nav.ts`; this module only does the IPC and the error toast.

import { appState, WORKSPACE_MRU_KEY } from "../state";
import * as ipc from "../ipc";
import { settingsStore } from "../settings";
import { reportError, toast } from "./toast";
import { pickLastWorkspace, stepWorkspace, visibleWorkspaceIndices } from "../workspace-nav";

/** Switch to `idx` and hydrate the store with the detail the backend returns.
 *  Out-of-range is a no-op; switching to the active workspace is allowed (a
 *  click on the active row re-syncs it). */
export async function switchToWorkspace(idx: number): Promise<void> {
  if (idx < 0 || idx >= appState.workspaces.length) return;
  try {
    const detail = await ipc.switchWorkspace(idx);
    appState.setActiveWorkspace(idx, detail);
  } catch (err) {
    reportError("Workspace switch failed", err);
  }
}

/** Sidebar order for cycling. The rows are only the *order*, so when the
 *  backend call fails we still cycle — over the plain list order. Indices
 *  past the current list are dropped: the rows are fetched async and the list
 *  may have shrunk meanwhile. */
async function cycleOrder(): Promise<number[]> {
  let visible: number[] = [];
  try {
    visible = visibleWorkspaceIndices(await ipc.sidebarRows());
  } catch (err) {
    console.error("Failed to load sidebar rows for cycling:", err);
  }
  const count = appState.workspaces.length;
  visible = visible.filter((i) => i < count);
  return visible.length > 0 ? visible : Array.from({ length: count }, (_, i) => i);
}

/** Next (`1`) / previous (`-1`) workspace in sidebar order, wrapping, with the
 *  children of a collapsed worktree family skipped — TUI `prefix }` / `{`. */
export async function cycleWorkspace(delta: 1 | -1): Promise<void> {
  if (appState.workspaces.length < 2) return;
  const next = stepWorkspace(await cycleOrder(), appState.activeWorkspace, delta);
  if (next === null) return;
  await switchToWorkspace(next);
}

/** Back to the workspace visited before this one — TUI `` prefix ` ``. Reads
 *  the same MRU list the switcher ranks with, so it survives a deleted
 *  workspace and the list being rebuilt. */
export async function toggleLastWorkspace(): Promise<void> {
  const mru = settingsStore.get<string[]>(WORKSPACE_MRU_KEY) ?? [];
  const paths = appState.workspaces.map((w) => String(w.info.path));
  const idx = pickLastWorkspace(mru, paths, appState.activeWorkspace);
  if (idx === null) {
    toast("No previous workspace", "info");
    return;
  }
  await switchToWorkspace(idx);
}
