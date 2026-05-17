// Workspace view: a top-level tab bar plus the active tab's pane tree.
//
// Each top-level tab owns one PaneNode tree. Every leaf is a pane holding at
// most ONE content item (terminal/agent/editor) — no per-pane mini tab bars.
// A blank pane shows a content chooser. Splits render as flex containers with
// a draggable resize handle.

import { appState } from "../state";
import type { PaneNode, LeafNode, SplitNode, PaneId } from "../pane-tree";
import { allLeaves } from "../pane-tree";
import type { TabInfo, AIProvider } from "../types";
import { getProviderLabel } from "../types";
import { mountTab, unmountTab } from "../tab-mount";
import {
  renderWorkspaceTabBar,
  getPaneProviderChoices,
  spawnIntoPane,
} from "./tab-bar";
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
  appState.on("active-tab-changed", render);
}

function render() {
  detachPanelElements(rootEl);
  rootEl.innerHTML = "";

  const ws = appState.activeWs;
  const wt = appState.activeTabTree;

  // Top-level tab bar (always present when a workspace is active).
  if (ws) {
    const bar = document.createElement("nav");
    rootEl.appendChild(bar);
    renderWorkspaceTabBar(bar);
  }

  const area = document.createElement("div");
  area.className = "pane-area";
  rootEl.appendChild(area);

  if (!ws || !wt) {
    renderWelcome(area);
    return;
  }

  area.appendChild(renderNode(wt.paneTree));
  syncMounts(wt.paneTree, ws.tabs);
  updateActivePaneHighlight();

  // Blank panes get a content chooser.
  for (const leaf of allLeaves(wt.paneTree)) {
    if (leaf.contentId) continue;
    const host = area.querySelector<HTMLElement>(
      `.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-content`,
    );
    if (host && host.children.length === 0) renderChooser(host, leaf.id);
  }
}

function renderNode(node: PaneNode): HTMLElement {
  return node.kind === "leaf" ? renderLeaf(node) : renderSplit(node);
}

function renderLeaf(leaf: LeafNode): HTMLElement {
  const el = document.createElement("div");
  el.className = "pane";
  el.dataset.paneId = leaf.id;
  el.addEventListener(
    "mousedown",
    () => {
      if (appState.activeTabTree?.activePaneId !== leaf.id) {
        appState.setActivePane(leaf.id);
      }
    },
    true,
  );

  const head = document.createElement("div");
  head.className = "pane-head";
  const title = paneTitle(leaf);
  head.innerHTML = `
    <span class="pane-title">${escapeHtml(title)}</span>
    <span class="pane-actions">
      <button class="pane-btn" data-act="right" title="Split right">⇥</button>
      <button class="pane-btn" data-act="down" title="Split down">⤓</button>
      <button class="pane-btn pane-btn-close" data-act="close" title="Close pane">×</button>
    </span>
  `;
  head.querySelector('[data-act="right"]')!.addEventListener("click", (e) => {
    e.stopPropagation();
    appState.setActivePane(leaf.id);
    appState.splitPane(leaf.id, "right");
  });
  head.querySelector('[data-act="down"]')!.addEventListener("click", (e) => {
    e.stopPropagation();
    appState.setActivePane(leaf.id);
    appState.splitPane(leaf.id, "down");
  });
  head.querySelector('[data-act="close"]')!.addEventListener("click", (e) => {
    e.stopPropagation();
    appState.closePane(leaf.id);
  });
  el.appendChild(head);

  const content = document.createElement("div");
  content.className = "pane-content";
  el.appendChild(content);
  return el;
}

function paneTitle(leaf: LeafNode): string {
  const ws = appState.activeWs;
  if (!leaf.contentId || !ws) return "Empty";
  const c = ws.tabs.find((t) => t.id === leaf.contentId);
  return c ? getProviderLabel(c.provider) : "Empty";
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

function syncMounts(tree: PaneNode, contents: TabInfo[]) {
  const wsIdx = appState.activeWorkspace;
  const leaves = allLeaves(tree);
  const visible = new Set<string>();
  for (const leaf of leaves) {
    if (leaf.contentId) visible.add(leaf.contentId);
  }
  // Hide every content not visible in the active tab's tree.
  for (const c of contents) {
    if (!visible.has(c.id)) unmountTab(c);
  }
  // Mount each pane's content into its host.
  for (const leaf of leaves) {
    if (!leaf.contentId) continue;
    const c = contents.find((t) => t.id === leaf.contentId);
    if (!c) continue;
    const host = rootEl.querySelector<HTMLElement>(
      `.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-content`,
    );
    if (host) mountTab(c, host, wsIdx);
  }
}

function renderChooser(host: HTMLElement, paneId: PaneId) {
  const box = document.createElement("div");
  box.className = "pane-chooser";
  box.innerHTML = `<div class="pane-chooser-title">Open in this pane</div>
    <div class="pane-chooser-list"><span class="pane-chooser-loading">…</span></div>`;
  host.appendChild(box);
  const list = box.querySelector<HTMLElement>(".pane-chooser-list")!;
  void getPaneProviderChoices().then((providers: AIProvider[]) => {
    list.innerHTML = "";
    for (const p of providers) {
      const btn = document.createElement("button");
      btn.className = "pane-chooser-item";
      btn.textContent = getProviderLabel(p);
      btn.addEventListener("click", () => {
        appState.setActivePane(paneId);
        void spawnIntoPane(paneId, p);
      });
      list.appendChild(btn);
    }
  });
}

function updateActivePaneHighlight() {
  const activeId = appState.activeTabTree?.activePaneId;
  rootEl.querySelectorAll<HTMLElement>(".pane").forEach((el) => {
    el.classList.toggle("active", el.dataset.paneId === activeId);
  });
}

function detachPanelElements(container: HTMLElement) {
  container.querySelectorAll<HTMLElement>(".pane-content").forEach((host) => {
    for (const child of Array.from(host.children) as HTMLElement[]) {
      if (
        child.classList.contains("terminal-welcome") ||
        child.classList.contains("pane-chooser")
      ) {
        host.removeChild(child);
        continue;
      }
      child.style.display = "none";
      host.removeChild(child);
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
    <p>Select a workspace or open a tab to begin.</p>
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
    const wt = appState.activeTabTree;
    if (!wt) return;
    const splitEl = handle.parentElement;
    if (!splitEl) return;
    const split = findSplitNode(wt.paneTree, splitId);
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

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function cssEscape(s: string): string {
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return CSS.escape(s);
  }
  return s.replace(/[^a-zA-Z0-9_-]/g, "\\$&");
}
