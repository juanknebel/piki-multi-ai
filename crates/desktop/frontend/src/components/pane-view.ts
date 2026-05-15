// Recursive pane view.
//
// Mounts the workspace's pane tree under a single root container. Each leaf
// renders a mini tab bar plus a content host where the active tab's DOM is
// mounted via `tab-mount.ts`. Splits render as flex containers with a
// draggable resize handle between children.

import { appState } from "../state";
import type { PaneNode, LeafNode, SplitNode, PaneId } from "../pane-tree";
import { allLeaves, findParentSplit } from "../pane-tree";
import type { TabInfo } from "../types";
import { mountTab, unmountTab } from "../tab-mount";
import { renderPaneTabBar } from "./tab-bar";
import { formatShortcut } from "../shortcuts";

let rootEl: HTMLElement;

export function initPaneView(container: HTMLElement) {
  rootEl = container;
  rootEl.classList.add("pane-root");
  render();
  appState.on("pane-tree-changed", render);
  appState.on("active-pane-changed", updateActivePaneHighlight);
  appState.on("active-workspace-changed", render);
  appState.on("tabs-changed", render);
  // Tab switches within a pane don't change the tree structure — re-render
  // only the affected mini tab bars and remount the new active tab. Avoids
  // a full DOM rebuild that would reparent every terminal element and
  // run fit() before the new host's layout has settled.
  appState.on("active-tab-changed", syncActiveTabUpdate);
  // Re-render the per-pane tab bars when shell-integration state changes so
  // exit-code badges update without a full pane-tree rebuild.
  appState.on("tab-shell-state-changed", () => {
    const ws = appState.activeWs;
    if (!ws) return;
    for (const leaf of allLeaves(ws.paneTree)) {
      const pane = rootEl.querySelector<HTMLElement>(
        `.pane[data-pane-id="${CSS.escape(leaf.id)}"]`,
      );
      const bar = pane?.querySelector<HTMLElement>(".pane-tab-bar");
      if (bar) renderPaneTabBar(bar, leaf);
    }
  });
}

function render() {
  const ws = appState.activeWs;

  // Detach any panel elements currently inside the root so the rebuild doesn't
  // garbage them. Panel modules hold their own references — `appendChild` later
  // moves them back in.
  detachPanelElements(rootEl);
  rootEl.innerHTML = "";

  if (!ws) {
    renderWelcome(rootEl);
    return;
  }

  const tree = ws.paneTree;
  rootEl.appendChild(renderNode(tree));
  syncMounts(ws.paneTree, ws.tabs);
  updateActivePaneHighlight();

  // Leaves with no tabs get a welcome message inside their content host so
  // the user still sees the "+" button on the mini tab bar and the keyboard
  // hints — the most common case is a workspace that was just opened or
  // had all its tabs closed.
  for (const leaf of allLeaves(tree)) {
    if (leaf.tabIds.length > 0) continue;
    const host = rootEl.querySelector<HTMLElement>(
      `.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-content`,
    );
    if (host && host.children.length === 0) {
      renderWelcome(host);
    }
  }
}

function renderNode(node: PaneNode): HTMLElement {
  if (node.kind === "leaf") {
    return renderLeaf(node);
  }
  return renderSplit(node);
}

function renderLeaf(leaf: LeafNode): HTMLElement {
  const el = document.createElement("div");
  el.className = "pane";
  el.dataset.paneId = leaf.id;
  el.addEventListener("mousedown", () => {
    if (appState.activeWs?.activePaneId !== leaf.id) {
      appState.setActivePane(leaf.id);
    }
  }, true);

  const bar = document.createElement("nav");
  bar.className = "pane-tab-bar";
  renderPaneTabBar(bar, leaf);
  el.appendChild(bar);

  const content = document.createElement("div");
  content.className = "pane-content";
  el.appendChild(content);

  return el;
}

function renderSplit(split: SplitNode): HTMLElement {
  const el = document.createElement("div");
  el.className = `pane-split ${split.orientation === "horiz" ? "horiz" : "vert"}`;
  el.dataset.paneId = split.id;

  const first = renderNode(split.first);
  first.style.flex = `${split.ratio}`;
  el.appendChild(first);

  const handle = document.createElement("div");
  handle.className = "pane-resize-handle";
  wireResizeHandle(handle, split.id);
  el.appendChild(handle);

  const second = renderNode(split.second);
  second.style.flex = `${1 - split.ratio}`;
  el.appendChild(second);

  return el;
}

function syncMounts(tree: PaneNode, tabs: TabInfo[]) {
  const wsIdx = appState.activeWorkspace;
  const leaves = allLeaves(tree);
  const visibleTabIds = new Set<string>();
  for (const leaf of leaves) {
    if (leaf.activeTabId) visibleTabIds.add(leaf.activeTabId);
  }

  // Hide tabs that are no longer the active one in any leaf.
  for (const tab of tabs) {
    if (!visibleTabIds.has(tab.id)) {
      unmountTab(tab);
    }
  }

  // Mount the active tab in each leaf into the leaf's content host.
  for (const leaf of leaves) {
    if (!leaf.activeTabId) continue;
    const tab = tabs.find((t) => t.id === leaf.activeTabId);
    if (!tab) continue;
    const host = rootEl.querySelector<HTMLElement>(
      `.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-content`,
    );
    if (host) {
      mountTab(tab, host, wsIdx);
    }
  }
}

function syncActiveTabUpdate() {
  const ws = appState.activeWs;
  if (!ws || ws.tabs.length === 0) {
    // Falling back to a welcome / rebuild path here — defer to render().
    render();
    return;
  }
  // If the DOM structure doesn't reflect every leaf in the tree (e.g.
  // initial load before the first render), do a full rebuild.
  for (const leaf of allLeaves(ws.paneTree)) {
    if (!rootEl.querySelector(`.pane[data-pane-id="${cssEscape(leaf.id)}"]`)) {
      render();
      return;
    }
  }
  // Re-render each leaf's mini tab bar so the active highlight follows.
  for (const leaf of allLeaves(ws.paneTree)) {
    const bar = rootEl.querySelector<HTMLElement>(
      `.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-tab-bar`,
    );
    if (bar) renderPaneTabBar(bar, leaf);
  }
  syncMounts(ws.paneTree, ws.tabs);
}

function updateActivePaneHighlight() {
  const activeId = appState.activeWs?.activePaneId;
  rootEl.querySelectorAll<HTMLElement>(".pane").forEach((el) => {
    if (el.dataset.paneId === activeId) {
      el.classList.add("active");
    } else {
      el.classList.remove("active");
    }
  });
}

function detachPanelElements(container: HTMLElement) {
  // Panel elements live in `.pane-content`. Detach them so they're not removed
  // when we wipe the container — panel modules keep their references alive.
  container.querySelectorAll<HTMLElement>(".pane-content").forEach((host) => {
    const children = Array.from(host.children) as HTMLElement[];
    for (const child of children) {
      // Welcome divs are stateless throwaways — drop them so they don't
      // accumulate in the holding area on every rebuild.
      if (child.classList.contains("terminal-welcome")) {
        host.removeChild(child);
        continue;
      }
      // Hide and detach; mounted again later by syncMounts if still active.
      child.style.display = "none";
      host.removeChild(child);
      // Move to a hidden holding area under the root so the element stays in
      // the document (some libraries depend on document-attached state).
      getHolding().appendChild(child);
    }
  });
}

function getHolding(): HTMLElement {
  let holding = document.getElementById("pane-holding");
  if (!holding) {
    holding = document.createElement("div");
    holding.id = "pane-holding";
    holding.style.display = "none";
    document.body.appendChild(holding);
  }
  return holding;
}

function renderWelcome(container: HTMLElement) {
  const welcome = document.createElement("div");
  welcome.className = "terminal-welcome";
  welcome.innerHTML = `
    <div class="welcome-logo">PIKI</div>
    <div class="welcome-subtitle">Multi-Agent Workspace</div>
    <p>Select a workspace or create one to begin.</p>
    <div class="welcome-shortcuts">
      <div class="shortcut-item"><span class="shortcut-key">${formatShortcut("Ctrl+N")}</span><span class="shortcut-label">New workspace</span></div>
      <div class="shortcut-item"><span class="shortcut-key">${formatShortcut("Ctrl+P")}</span><span class="shortcut-label">Command palette</span></div>
      <div class="shortcut-item"><span class="shortcut-key">${formatShortcut("Ctrl+Space")}</span><span class="shortcut-label">Switch workspace</span></div>
      <div class="shortcut-item"><span class="shortcut-key">?</span><span class="shortcut-label">All shortcuts</span></div>
    </div>
  `;
  container.appendChild(welcome);
}

// ── Resize handle drag ────────────────────────────────

function wireResizeHandle(handle: HTMLElement, splitId: PaneId) {
  handle.addEventListener("mousedown", (e) => {
    e.preventDefault();
    const ws = appState.activeWs;
    if (!ws) return;
    const splitEl = handle.parentElement;
    if (!splitEl) return;
    const split = findSplitNode(ws.paneTree, splitId);
    if (!split) return;
    const rect = splitEl.getBoundingClientRect();
    const isHoriz = split.orientation === "horiz";
    const size = isHoriz ? rect.width : rect.height;
    const start = isHoriz ? rect.left : rect.top;
    if (size <= 0) return;

    handle.classList.add("dragging");
    document.body.style.cursor = isHoriz ? "ew-resize" : "ns-resize";

    const firstChild = splitEl.children[0] as HTMLElement | undefined;
    const secondChild = splitEl.children[2] as HTMLElement | undefined;

    let currentRatio = split.ratio;

    function onMove(ev: MouseEvent) {
      const pos = isHoriz ? ev.clientX : ev.clientY;
      const ratio = Math.max(0.1, Math.min(0.9, (pos - start) / size));
      currentRatio = ratio;
      if (firstChild) firstChild.style.flex = `${ratio}`;
      if (secondChild) secondChild.style.flex = `${1 - ratio}`;
    }

    function onUp() {
      handle.classList.remove("dragging");
      document.body.style.cursor = "";
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      appState.setSplitRatio(splitId, currentRatio);
    }

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  });
}

function findSplitNode(root: PaneNode, id: PaneId): SplitNode | null {
  if (root.kind === "split") {
    if (root.id === id) return root;
    return findSplitNode(root.first, id) ?? findSplitNode(root.second, id);
  }
  return null;
}

// Reference findParentSplit so its export stays in use (will be needed in
// follow-up phases for boundary checks).
void findParentSplit;

function cssEscape(s: string): string {
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return CSS.escape(s);
  }
  return s.replace(/[^a-zA-Z0-9_-]/g, "\\$&");
}
