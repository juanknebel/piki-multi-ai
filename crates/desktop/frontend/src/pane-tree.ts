// Recursive pane tree for the desktop UI.
//
// Each top-level workspace TAB owns one PaneNode tree. A leaf holds exactly
// ONE content item (a terminal / agent / editor / …) by id, or null when the
// pane is blank (showing the content chooser). Splits hold two children plus
// an orientation/ratio. All updates are pure functions returning a new tree.

export type PaneId = string;

export interface LeafNode {
  kind: "leaf";
  id: PaneId;
  /** Content item id occupying this pane, or null = blank (chooser shown). */
  contentId: string | null;
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

export function newLeaf(contentId: string | null = null): LeafNode {
  return { kind: "leaf", id: genId(), contentId };
}

export function findPane(root: PaneNode, id: PaneId): PaneNode | null {
  if (root.id === id) return root;
  if (root.kind === "split") {
    return findPane(root.first, id) ?? findPane(root.second, id);
  }
  return null;
}

/** The leaf currently holding `contentId`, or null. */
export function findContentPane(root: PaneNode, contentId: string): LeafNode | null {
  if (root.kind === "leaf") {
    return root.contentId === contentId ? root : null;
  }
  return findContentPane(root.first, contentId) ?? findContentPane(root.second, contentId);
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

/**
 * Split `paneId` (must be a leaf). The existing content stays in the
 * source pane (left/top); a fresh BLANK pane appears on the new side
 * (right/bottom) for the user to fill via the chooser.
 */
export function splitPane(
  root: PaneNode,
  paneId: PaneId,
  dir: SplitDir,
): { root: PaneNode; newPaneId: PaneId } {
  const target = findPane(root, paneId);
  if (!target || target.kind !== "leaf") {
    return { root, newPaneId: paneId };
  }
  const newPane = newLeaf(null);
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
 * Returns `{ root: null }` only when the removed leaf was the root (caller
 * then closes the whole workspace tab).
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
  if (root.first.id === paneId) {
    return { root: root.second, promotedPaneId: firstLeafId(root.second) };
  }
  if (root.second.id === paneId) {
    return { root: root.first, promotedPaneId: firstLeafId(root.first) };
  }
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

/** Set (or clear, with null) the content of a leaf. */
export function setContent(
  root: PaneNode,
  paneId: PaneId,
  contentId: string | null,
): PaneNode {
  return mapPane(root, paneId, (p) =>
    p.kind === "leaf" ? { ...p, contentId } : p,
  );
}

/** Blank out whichever leaf holds `contentId` (pane stays, shows chooser). */
export function removeContent(root: PaneNode, contentId: string): PaneNode {
  if (root.kind === "leaf") {
    return root.contentId === contentId ? { ...root, contentId: null } : root;
  }
  return {
    ...root,
    first: removeContent(root.first, contentId),
    second: removeContent(root.second, contentId),
  };
}

export function setSplitRatio(root: PaneNode, splitId: PaneId, ratio: number): PaneNode {
  const clamped = Math.max(0.1, Math.min(0.9, ratio));
  return mapPane(root, splitId, (p) => (p.kind === "split" ? { ...p, ratio: clamped } : p));
}

/** Parent split of a node id, or null if it's the root. */
export function findParentSplit(root: PaneNode, id: PaneId): SplitNode | null {
  if (root.kind === "leaf") return null;
  if (root.first.id === id || root.second.id === id) return root;
  return findParentSplit(root.first, id) ?? findParentSplit(root.second, id);
}

/** Content ids referenced anywhere in the tree. */
export function treeContentIds(root: PaneNode): string[] {
  return allLeaves(root)
    .map((l) => l.contentId)
    .filter((c): c is string => c !== null);
}

// ── Serialization ────────────────────────────────────

export function serialize(root: PaneNode): unknown {
  return root;
}

export function deserialize(raw: unknown): PaneNode | null {
  if (!raw || typeof raw !== "object") return null;
  const node = raw as Record<string, unknown>;
  if (node.kind === "leaf") {
    if (typeof node.id !== "string") return null;
    const contentId = typeof node.contentId === "string" ? node.contentId : null;
    return { kind: "leaf", id: node.id, contentId };
  }
  if (node.kind === "split") {
    if (typeof node.id !== "string") return null;
    const first = deserialize(node.first);
    const second = deserialize(node.second);
    if (!first || !second) return null;
    const orientation = node.orientation === "vert" ? "vert" : "horiz";
    const ratio =
      typeof node.ratio === "number" && node.ratio > 0 && node.ratio < 1
        ? node.ratio
        : 0.5;
    return { kind: "split", id: node.id, orientation, ratio, first, second };
  }
  return null;
}

/** Null out leaf contentIds that aren't in `knownIds` (pane stays blank).
 *  Always returns a tree (a tab with blank panes is valid). */
export function reconcileWithContents(
  root: PaneNode,
  knownIds: Set<string>,
): PaneNode {
  if (root.kind === "leaf") {
    if (root.contentId && !knownIds.has(root.contentId)) {
      return { ...root, contentId: null };
    }
    return root;
  }
  return {
    ...root,
    first: reconcileWithContents(root.first, knownIds),
    second: reconcileWithContents(root.second, knownIds),
  };
}
