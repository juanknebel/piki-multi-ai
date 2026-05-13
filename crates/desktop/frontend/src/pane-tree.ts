// Recursive pane tree for the desktop UI.
//
// Each workspace owns one PaneNode tree. Leaves hold a list of tabIds and an
// active tabId; splits hold two children plus an orientation/ratio. All updates
// are pure functions that return a new tree — caller swaps the root into state.

export type PaneId = string;

export interface LeafNode {
  kind: "leaf";
  id: PaneId;
  tabIds: string[];
  activeTabId: string | null;
}

export interface SplitNode {
  kind: "split";
  id: PaneId;
  orientation: "horiz" | "vert";
  ratio: number; // first child's share of the parent, in (0, 1)
  first: PaneNode;
  second: PaneNode;
}

export type PaneNode = LeafNode | SplitNode;

export type SplitDir = "right" | "down";

function genId(): PaneId {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `pane-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function newLeaf(tabIds: string[] = [], activeTabId?: string | null): LeafNode {
  const active = activeTabId !== undefined
    ? activeTabId
    : tabIds.length > 0 ? tabIds[0] : null;
  return { kind: "leaf", id: genId(), tabIds: [...tabIds], activeTabId: active };
}

export function findPane(root: PaneNode, id: PaneId): PaneNode | null {
  if (root.id === id) return root;
  if (root.kind === "split") {
    return findPane(root.first, id) ?? findPane(root.second, id);
  }
  return null;
}

export function findTabPane(root: PaneNode, tabId: string): LeafNode | null {
  if (root.kind === "leaf") {
    return root.tabIds.includes(tabId) ? root : null;
  }
  return findTabPane(root.first, tabId) ?? findTabPane(root.second, tabId);
}

export function allLeaves(root: PaneNode): LeafNode[] {
  if (root.kind === "leaf") return [root];
  return [...allLeaves(root.first), ...allLeaves(root.second)];
}

function mapPane(root: PaneNode, id: PaneId, fn: (p: PaneNode) => PaneNode): PaneNode {
  if (root.id === id) return fn(root);
  if (root.kind === "split") {
    return { ...root, first: mapPane(root.first, id, fn), second: mapPane(root.second, id, fn) };
  }
  return root;
}

export function splitPane(
  root: PaneNode,
  paneId: PaneId,
  dir: SplitDir,
): { root: PaneNode; newPaneId: PaneId } {
  const target = findPane(root, paneId);
  if (!target || target.kind !== "leaf") {
    return { root, newPaneId: paneId };
  }

  // Always: existing tabs stay in the source pane (left/top); a fresh empty
  // pane appears on the new side (right/bottom). Callers that want to also
  // move a specific tab into the new pane should follow up with
  // `moveTabToPane(newPaneId, tabId)`.
  const newPane = newLeaf([], null);
  const orientation: SplitNode["orientation"] = dir === "right" ? "horiz" : "vert";
  const split: SplitNode = {
    kind: "split",
    id: genId(),
    orientation,
    ratio: 0.5,
    first: target,
    second: newPane,
  };

  return { root: mapPane(root, paneId, () => split), newPaneId: newPane.id };
}

/**
 * Remove a leaf from the tree, collapsing its parent split into the sibling.
 * Returns `{ root: null }` only when the removed leaf was the root.
 */
export function closePane(
  root: PaneNode,
  paneId: PaneId,
): { root: PaneNode | null; promotedPaneId: PaneId | null } {
  if (root.id === paneId) {
    return { root: null, promotedPaneId: null };
  }
  if (root.kind === "leaf") {
    return { root, promotedPaneId: null };
  }

  // Direct child match → collapse this split into the sibling.
  if (root.first.id === paneId) {
    return { root: root.second, promotedPaneId: firstLeafId(root.second) };
  }
  if (root.second.id === paneId) {
    return { root: root.first, promotedPaneId: firstLeafId(root.first) };
  }

  // Recurse.
  const firstResult = closePane(root.first, paneId);
  if (firstResult.root !== root.first) {
    return {
      root: firstResult.root ? { ...root, first: firstResult.root } : root.second,
      promotedPaneId: firstResult.promotedPaneId,
    };
  }
  const secondResult = closePane(root.second, paneId);
  if (secondResult.root !== root.second) {
    return {
      root: secondResult.root ? { ...root, second: secondResult.root } : root.first,
      promotedPaneId: secondResult.promotedPaneId,
    };
  }
  return { root, promotedPaneId: null };
}

function firstLeafId(node: PaneNode): PaneId {
  return node.kind === "leaf" ? node.id : firstLeafId(node.first);
}

/**
 * Remove a tab from wherever it lives. If the owning leaf becomes empty, it is
 * collapsed and `collapsedPaneId` is set to the leaf that was removed.
 */
export function removeTab(
  root: PaneNode,
  tabId: string,
): { root: PaneNode | null; collapsedPaneId: PaneId | null } {
  if (root.kind === "leaf") {
    if (!root.tabIds.includes(tabId)) return { root, collapsedPaneId: null };
    const newTabs = root.tabIds.filter((t) => t !== tabId);
    const newActive = root.activeTabId === tabId
      ? (newTabs[0] ?? null)
      : root.activeTabId;
    // Don't collapse the root leaf — caller handles empty root.
    return {
      root: { ...root, tabIds: newTabs, activeTabId: newActive },
      collapsedPaneId: null,
    };
  }

  const firstHas = findTabPane(root.first, tabId);
  const child = firstHas ? "first" : "second";
  const result = removeTab(root[child], tabId);
  if (result.root === root[child]) return { root, collapsedPaneId: null };

  const updatedChild = result.root;
  // Collapse the child if it became an empty leaf.
  if (updatedChild && updatedChild.kind === "leaf" && updatedChild.tabIds.length === 0) {
    const sibling = child === "first" ? root.second : root.first;
    return { root: sibling, collapsedPaneId: updatedChild.id };
  }

  return {
    root: { ...root, [child]: updatedChild } as SplitNode,
    collapsedPaneId: result.collapsedPaneId,
  };
}

export function moveTabToPane(root: PaneNode, tabId: string, dstPaneId: PaneId): PaneNode {
  const source = findTabPane(root, tabId);
  const dst = findPane(root, dstPaneId);
  if (!source || !dst || dst.kind !== "leaf" || source.id === dstPaneId) {
    return root;
  }

  // Remove from source first (may collapse the leaf).
  const removed = removeTab(root, tabId);
  if (!removed.root) return root;

  // Then add to destination (the dst may have shifted if source was collapsed,
  // but its id stays the same — re-locate by id).
  return addTabToPane(removed.root, dstPaneId, tabId);
}

export function addTabToPane(root: PaneNode, paneId: PaneId, tabId: string): PaneNode {
  return mapPane(root, paneId, (p) => {
    if (p.kind !== "leaf") return p;
    if (p.tabIds.includes(tabId)) return { ...p, activeTabId: tabId };
    return { ...p, tabIds: [...p.tabIds, tabId], activeTabId: tabId };
  });
}

export function setActiveTabInPane(root: PaneNode, paneId: PaneId, tabId: string): PaneNode {
  return mapPane(root, paneId, (p) => {
    if (p.kind !== "leaf" || !p.tabIds.includes(tabId)) return p;
    return { ...p, activeTabId: tabId };
  });
}

export function setSplitRatio(root: PaneNode, splitId: PaneId, ratio: number): PaneNode {
  const clamped = Math.max(0.1, Math.min(0.9, ratio));
  return mapPane(root, splitId, (p) => p.kind === "split" ? { ...p, ratio: clamped } : p);
}

/** Find the parent split of a node id, or null if node is the root. */
export function findParentSplit(root: PaneNode, id: PaneId): SplitNode | null {
  if (root.kind === "leaf") return null;
  if (root.first.id === id || root.second.id === id) return root;
  return findParentSplit(root.first, id) ?? findParentSplit(root.second, id);
}

// ── Serialization ────────────────────────────────────

export function serialize(root: PaneNode): unknown {
  return root;
}

export function deserialize(raw: unknown): PaneNode | null {
  if (!raw || typeof raw !== "object") return null;
  const node = raw as Record<string, unknown>;
  if (node.kind === "leaf") {
    if (typeof node.id !== "string" || !Array.isArray(node.tabIds)) return null;
    const tabIds = node.tabIds.filter((t): t is string => typeof t === "string");
    const activeTabId = typeof node.activeTabId === "string" ? node.activeTabId : null;
    return { kind: "leaf", id: node.id, tabIds, activeTabId };
  }
  if (node.kind === "split") {
    if (typeof node.id !== "string") return null;
    const first = deserialize(node.first);
    const second = deserialize(node.second);
    if (!first || !second) return null;
    const orientation = node.orientation === "vert" ? "vert" : "horiz";
    const ratio = typeof node.ratio === "number" && node.ratio > 0 && node.ratio < 1
      ? node.ratio : 0.5;
    return { kind: "split", id: node.id, orientation, ratio, first, second };
  }
  return null;
}

/**
 * Validate a deserialized tree against the workspace's current tab list:
 * - drops tabIds that no longer exist
 * - returns null if the tree is empty or any leaf would have an unknown activeTabId
 *   with no remaining tabs (caller should fall back to a fresh root leaf with all tabs).
 */
export function reconcileWithTabs(root: PaneNode, knownTabIds: Set<string>): PaneNode | null {
  if (root.kind === "leaf") {
    const tabIds = root.tabIds.filter((t) => knownTabIds.has(t));
    if (tabIds.length === 0) return null;
    const activeTabId = root.activeTabId && tabIds.includes(root.activeTabId)
      ? root.activeTabId
      : tabIds[0];
    return { ...root, tabIds, activeTabId };
  }
  const first = reconcileWithTabs(root.first, knownTabIds);
  const second = reconcileWithTabs(root.second, knownTabIds);
  if (!first && !second) return null;
  if (!first) return second;
  if (!second) return first;
  return { ...root, first, second };
}
