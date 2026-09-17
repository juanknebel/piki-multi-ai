// Workspace navigation order as pure list operations — the impure half
// (IPC + switching) lives in `components/workspace-actions.ts`. Cycling walks
// the VISUAL order: the rows `core::workspace::sidebar_rows` produces, which
// is what the sidebar renders, so a collapsed worktree family is skipped
// because its children are not rows at all. Same rule as the TUI's
// `App::next_workspace` / `prev_workspace`. No DOM, no IPC; covered by
// workspace-nav.test.ts.

import type { SidebarRow } from "./ipc";

type WorkspaceRow = Extract<SidebarRow, { type: "workspace" }>;

/** Workspace indices in sidebar order — group headers dropped, children of a
 *  collapsed family already absent from `rows`. */
export function visibleWorkspaceIndices(rows: readonly SidebarRow[]): number[] {
  return rows.filter((r): r is WorkspaceRow => r.type === "workspace").map((r) => r.index);
}

/** The workspace `delta` steps from `active` along `visible`, wrapping at both
 *  ends. `null` when there is nowhere to go: no visible rows, or the step
 *  lands back on the active one (a single row). An `active` that isn't
 *  visible itself (its family got collapsed under it) starts the walk at the
 *  first row, mirroring the TUI's `position(...).unwrap_or(0)`. */
export function stepWorkspace(
  visible: readonly number[],
  active: number,
  delta: number,
): number | null {
  if (visible.length === 0) return null;
  const at = visible.indexOf(active);
  const pos = at === -1 ? 0 : at;
  const len = visible.length;
  const next = visible[(((pos + delta) % len) + len) % len];
  return next === active ? null : next;
}

/** The "last workspace" target for the toggle: the most recently used
 *  workspace that is not the active one, resolved through the MRU list of
 *  paths `appState.setActiveWorkspace` maintains. Walking the whole list (not
 *  just `mru[1]`) makes it survive a deleted or unloaded workspace, and using
 *  paths instead of indices makes it survive the list being rebuilt.
 *  `null` when nothing in the MRU is loaded any more. */
export function pickLastWorkspace(
  mru: readonly string[],
  paths: readonly string[],
  activeIdx: number,
): number | null {
  for (const key of mru) {
    const idx = paths.indexOf(key);
    if (idx >= 0 && idx !== activeIdx) return idx;
  }
  return null;
}
