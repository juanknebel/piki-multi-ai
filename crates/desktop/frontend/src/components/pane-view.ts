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
  tearDownAndClosePane,
  restartPaneContent,
} from "./tab-bar";
import { contentLabel, getPaneProviderChoices, openProvider, TOOL_CHOICES } from "./open-content";
import { getShortcutKey } from "../shortcuts";
import { branchLabel } from "../labels";
import { icon } from "./icons";
import { openFuzzySearch } from "./fuzzy-search";
import { literalNextTab, onLiteralNextChange } from "../literal-next";

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
  // Agent status changes only affect the ws-tab bar's status dots — refresh
  // just that strip, never the whole pane tree (avoids terminal remount).
  // A terminal title (OSC 0/2) also lands here: pane titles are patched in
  // place for the same reason.
  appState.on("tab-shell-state-changed", () => {
    refreshWsTabBar();
    refreshPaneTitles();
  });
  appState.on("workspace-attention-changed", refreshWsTabBar);
  appState.on("agent-rows-changed", refreshWsTabBar);
  onLiteralNextChange(refreshLiteralHint);
}

function refreshWsTabBar() {
  const bar = rootEl?.querySelector<HTMLElement>(".ws-tab-bar");
  if (bar) renderWorkspaceTabBar(bar);
}

/** Patch every pane header's title text without rebuilding the tree. */
function refreshPaneTitles() {
  const wt = appState.activeTabTree;
  if (!wt || !rootEl) return;
  for (const leaf of allLeaves(wt.paneTree)) {
    const title = rootEl.querySelector<HTMLElement>(
      `.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-head > .pane-title`,
    );
    if (title) title.innerHTML = paneTitleHtml(leaf);
  }
}

/** Show the "next key → terminal" badge in the header of the pane whose
 *  terminal is armed (literal-next.ts); remove it everywhere else. */
function refreshLiteralHint() {
  if (!rootEl) return;
  rootEl.querySelectorAll(".pane-literal-hint").forEach((el) => el.remove());
  const tabId = literalNextTab();
  const wt = appState.activeTabTree;
  if (!tabId || !wt) return;
  const leaf = allLeaves(wt.paneTree).find((l) => l.contentId === tabId);
  if (!leaf) return;
  const head = rootEl.querySelector<HTMLElement>(`.pane[data-pane-id="${cssEscape(leaf.id)}"] > .pane-head`);
  const actions = head?.querySelector(".pane-actions");
  if (!head || !actions) return;
  const hint = document.createElement("span");
  hint.className = "pane-literal-hint";
  hint.textContent = "next key → terminal · Esc cancels";
  hint.title = `${getShortcutKey("literal-next")}: the next keystroke goes to the terminal, bypassing app shortcuts`;
  head.insertBefore(hint, actions);
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

  if (!ws) {
    renderWelcome(area);
    return;
  }
  if (!wt) {
    // A workspace with no tabs: its own empty state (name · branch + what
    // can open here), not the app-wide welcome.
    renderEmptyState(area, ws, (p) => void openProvider(p), () => openFuzzySearch());
    return;
  }

  area.appendChild(renderNode(wt.paneTree));
  syncMounts(wt.paneTree, ws.tabs);
  updateActivePaneHighlight();
  refreshLiteralHint();

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
  const exited = paneExited(leaf);
  if (exited) head.classList.add("pane-head--dead");
  head.innerHTML = `
    <span class="pane-title">${paneTitleHtml(leaf)}</span>
    <span class="pane-actions">
      ${exited ? '<button class="pane-btn pane-btn-restart" data-act="restart" title="Restart here">${icon("refresh")}<span class="icon-label">Restart</span></button>' : ""}
      <button class="pane-btn" data-act="right" title="Split right" aria-label="Split right">${icon("split-right")}</button>
      <button class="pane-btn" data-act="down" title="Split down" aria-label="Split down">${icon("split-down")}</button>
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
    void tearDownAndClosePane(leaf.id);
  });
  head.querySelector('[data-act="restart"]')?.addEventListener("click", (e) => {
    e.stopPropagation();
    appState.setActivePane(leaf.id);
    void restartPaneContent(leaf.id);
  });
  el.appendChild(head);

  const content = document.createElement("div");
  content.className = "pane-content";
  el.appendChild(content);
  return el;
}

/** True when the pane holds a PTY-backed content whose process has exited. */
function paneExited(leaf: LeafNode): boolean {
  const ws = appState.activeWs;
  if (!leaf.contentId || !ws) return false;
  const c = ws.tabs.find((t) => t.id === leaf.contentId);
  if (!c || c.alive) return false;
  return c.provider === "Shell" || (typeof c.provider === "object" && "Custom" in c.provider);
}

function paneTitle(leaf: LeafNode): string {
  const ws = appState.activeWs;
  if (!leaf.contentId || !ws) return "Empty";
  const c = ws.tabs.find((t) => t.id === leaf.contentId);
  if (c) return contentLabel(c);
  // Known to the layout but not (yet) to us: a Kanban / API board being
  // re-spawned after a restart.
  return "Restoring…";
}

/** Inner HTML of a pane header's `.pane-title` (label + exited mark). */
function paneTitleHtml(leaf: LeafNode): string {
  return `${escapeHtml(paneTitle(leaf))}${paneExited(leaf) ? '<span class="pane-title-dead"> · exited</span>' : ""}`;
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
  const ws = appState.activeWs;
  if (!ws) return;
  renderEmptyState(
    host,
    ws,
    (p) => {
      appState.setActivePane(paneId);
      void openProvider(p, { paneId });
    },
    () => {
      appState.setActivePane(paneId);
      openFuzzySearch({ paneId });
    },
    "Open in this pane",
  );
}

/** The per-workspace empty state, shared by a workspace with no tabs and a
 *  blank pane: "<workspace> · <branch>", then Shell / every configured
 *  provider (the list File ▸ New Tab preloads), then the tools — Web
 *  Preview / Kanban / API — and "Open file…". `open` / `openFile` say where
 *  a pick goes (new tab vs. this pane). */
function renderEmptyState(
  host: HTMLElement,
  ws: NonNullable<typeof appState.activeWs>,
  open: (p: AIProvider) => void,
  openFile: () => void,
  title = "Open here",
) {
  const box = document.createElement("div");
  box.className = "pane-chooser ui-empty";
  box.dataset.fill = "";
  box.innerHTML = `
    <div class="pane-empty-ws" title="${escapeHtml(ws.info.name)}${ws.branch ? ` · ${escapeHtml(ws.branch)}` : ""}">
      <span class="pane-empty-name">${escapeHtml(ws.info.name)}</span>
      <span class="pane-empty-sep">·</span>
      <span class="pane-empty-branch">${icon("branch")} ${escapeHtml(branchLabel(ws.branch))}</span>
    </div>
    <div class="pane-chooser-title">${escapeHtml(title)}</div>
    <div class="pane-chooser-list"><span class="pane-chooser-loading">…</span></div>
    <div class="pane-chooser-list pane-chooser-tools"></div>
    <div class="pane-empty-hint">${escapeHtml(getShortcutKey("command-palette"))} command palette · ${escapeHtml(getShortcutKey("workspace-switcher"))} switch workspace</div>`;
  host.appendChild(box);
  const list = box.querySelector<HTMLElement>(".pane-chooser-list")!;
  const tools = box.querySelector<HTMLElement>(".pane-chooser-tools")!;
  const item = (label: string, onClick: () => void, title?: string) => {
    const btn = document.createElement("button");
    btn.className = "pane-chooser-item ui-btn";
    btn.dataset.variant = "secondary";
    btn.textContent = label;
    if (title) btn.title = title;
    btn.addEventListener("click", onClick);
    return btn;
  };
  for (const p of TOOL_CHOICES) tools.appendChild(item(getProviderLabel(p), () => open(p)));
  tools.appendChild(item("Open file…", openFile, `Find a file in ${ws.info.name} (${getShortcutKey("fuzzy-search")})`));
  void getPaneProviderChoices().then((providers: AIProvider[]) => {
    list.innerHTML = "";
    for (const p of providers) list.appendChild(item(getProviderLabel(p), () => open(p)));
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
      <div class="shortcut-item"><span class="shortcut-key">${getShortcutKey("new-workspace")}</span><span class="shortcut-label">New workspace</span></div>
      <div class="shortcut-item"><span class="shortcut-key">${getShortcutKey("command-palette")}</span><span class="shortcut-label">Command palette</span></div>
      <div class="shortcut-item"><span class="shortcut-key">${getShortcutKey("workspace-switcher")}</span><span class="shortcut-label">Switch workspace</span></div>
      <div class="shortcut-item"><span class="shortcut-key">${getShortcutKey("help")}</span><span class="shortcut-label">All shortcuts</span></div>
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
