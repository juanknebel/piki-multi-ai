// Pure helpers for the per-workspace layout snapshot (`wsTabsV2` in the
// settings document). Besides the pane trees, the snapshot carries a
// descriptor per NON-PTY content so editors, web previews, the kanban board
// and the API explorer come back after a restart: PTY contents are
// re-attached by the session daemon, everything else is re-created from
// here (`appState._hydrateLayout` + the `ContentRestorer` in
// components/open-content.ts). No DOM, no IPC — vitest-covered.

import type { PaneNode } from "./pane-tree";
import { allLeaves, treeContentIds } from "./pane-tree";
import type { AIProvider, TabInfo } from "./types";

/** Non-PTY content kinds the snapshot can bring back. */
export type SavedContentKind = "CodeEditor" | "Markdown" | "WebPreview" | "Kanban" | "Api";

export interface SavedContent {
  id: string;
  kind: SavedContentKind;
  /** Workspace-relative file path (editors). */
  path?: string;
  /** Last loaded URL (web preview); empty = nothing loaded yet. */
  url?: string;
  title?: string | null;
}

export interface SavedWsTab {
  tree: unknown;
  activePaneId: string;
}

export interface SavedWsLayout {
  tabs: SavedWsTab[];
  activeWsTab: number;
  /** Absent in snapshots written before phase 10. */
  contents?: SavedContent[];
}

const SAVED_KINDS: readonly string[] = ["CodeEditor", "Markdown", "WebPreview", "Kanban", "Api"];

export function isSavedContentKind(p: unknown): p is SavedContentKind {
  return typeof p === "string" && SAVED_KINDS.includes(p);
}

/** Kinds that live only in the frontend: restored synchronously with the
 *  SAME id (their panel state survives an in-session re-hydration). */
export function isFrontendOnlyKind(kind: SavedContentKind): boolean {
  return kind === "CodeEditor" || kind === "Markdown" || kind === "WebPreview";
}

/** Kinds backed by a backend `DesktopTab` without a PTY: gone after a
 *  restart, re-spawned with a NEW id (`ipc.spawnTab`) and remapped. */
export function isRespawnKind(kind: SavedContentKind): boolean {
  return kind === "Kanban" || kind === "Api";
}

/** Descriptor for a content, or null when the snapshot cannot bring it back
 *  (PTY contents; an editor whose path is unknown). `pathOf`/`urlOf` read
 *  the panel registries. */
export function describeContent(
  tab: TabInfo,
  pathOf: (id: string) => string | null,
  urlOf: (id: string) => string | null,
): SavedContent | null {
  const p: AIProvider = tab.provider;
  if (!isSavedContentKind(p)) return null;
  const base: SavedContent = { id: tab.id, kind: p };
  if (tab.custom_title) base.title = tab.custom_title;
  if (p === "CodeEditor" || p === "Markdown") {
    const path = pathOf(tab.id);
    if (!path) return null;
    return { ...base, path };
  }
  if (p === "WebPreview") return { ...base, url: urlOf(tab.id) ?? "" };
  return base;
}

/** Validate a raw settings value into a layout map; anything malformed is
 *  dropped rather than thrown. */
export function parseSavedLayouts(raw: unknown): Record<string, SavedWsLayout> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, SavedWsLayout> = {};
  for (const [path, entry] of Object.entries(raw as Record<string, unknown>)) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    if (!Array.isArray(e.tabs)) continue;
    const tabs: SavedWsTab[] = e.tabs
      .filter((t): t is Record<string, unknown> => !!t && typeof t === "object")
      .map((t) => ({ tree: t.tree, activePaneId: typeof t.activePaneId === "string" ? t.activePaneId : "" }));
    const contents = Array.isArray(e.contents) ? e.contents.filter(isSavedContent) : undefined;
    out[path] = {
      tabs,
      activeWsTab: typeof e.activeWsTab === "number" ? e.activeWsTab : 0,
      ...(contents ? { contents } : {}),
    };
  }
  return out;
}

function isSavedContent(c: unknown): c is SavedContent {
  if (!c || typeof c !== "object") return false;
  const o = c as Record<string, unknown>;
  if (typeof o.id !== "string" || !isSavedContentKind(o.kind)) return false;
  if (o.path !== undefined && typeof o.path !== "string") return false;
  if (o.url !== undefined && typeof o.url !== "string") return false;
  return true;
}

/** Saved contents referenced by the saved trees (in tree order) that are not
 *  among `knownIds` — what must be re-created before hydration. */
export function missingSavedContents(
  entry: SavedWsLayout,
  trees: PaneNode[],
  knownIds: Set<string>,
): SavedContent[] {
  const byId = new Map((entry.contents ?? []).map((c) => [c.id, c]));
  const out: SavedContent[] = [];
  const seen = new Set<string>();
  for (const tree of trees) {
    for (const id of treeContentIds(tree)) {
      if (knownIds.has(id) || seen.has(id)) continue;
      const c = byId.get(id);
      if (!c) continue;
      seen.add(id);
      out.push(c);
    }
  }
  return out;
}

/** Rewrite leaf content ids through `map` (ids not in the map are kept). */
export function remapContentIds(root: PaneNode, map: Map<string, string>): PaneNode {
  if (root.kind === "leaf") {
    const next = root.contentId ? map.get(root.contentId) : undefined;
    return next ? { ...root, contentId: next } : root;
  }
  return { ...root, first: remapContentIds(root.first, map), second: remapContentIds(root.second, map) };
}

/** Descriptors for every content of `tabs` that the snapshot can restore,
 *  in list order (the tab list order is what hydration appends in). */
export function snapshotContents(
  tabs: TabInfo[],
  describe: (tab: TabInfo) => SavedContent | null,
): SavedContent[] {
  const out: SavedContent[] = [];
  for (const t of tabs) {
    const d = describe(t);
    if (d) out.push(d);
  }
  return out;
}

/** True when every leaf of `root` is blank. */
export function isAllBlank(root: PaneNode): boolean {
  return allLeaves(root).every((l) => l.contentId === null);
}
