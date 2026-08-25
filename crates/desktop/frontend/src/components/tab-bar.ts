// Workspace-level (top) tab bar. Each top-level tab owns its own split
// layout (see pane-view.ts); a pane holds at most one content item.
//
// Mouse: click switches, middle-click closes (through the same safe path as
// the × — a running process still gets its confirm), right-click opens the
// tab menu, double-click renames inline. The `+` and the `⋯` "all tabs"
// button sit outside the scrolling strip so they never scroll away.

import { appState } from "../state";
import { makeInteractive } from "./a11y";
import * as ipc from "../ipc";
import { toast, reportError } from "./toast";
import { showConfirm, escapeHtml as escapeConfirmHtml } from "./confirm";
import { createDropdown } from "./dropdown";
import { openContextMenu, type CtxItem } from "./context-menu";
import { destroyTerminal } from "./terminal-panel";
import { getProviderLabel, getTabLabel, cliAgentStatusView, agentStatusSeverity } from "../types";
import type { AIProvider, TabInfo, CliAgentStatus } from "../types";
import type { PaneId, PaneNode } from "../pane-tree";
import { allLeaves, findPane } from "../pane-tree";
import { branchLabel } from "../labels";
import { getCachedProviderTabs, preloadProviderTabs } from "./provider-cache";
import {
  destroyMarkdownEditorPanel,
  getMarkdownEditorFileName,
} from "./markdown-editor-panel";
import {
  destroyCodeEditorPanel,
  getCodeEditorFileName,
  isCodeEditorDirty,
  showUnsavedChangesPrompt,
} from "./code-editor-panel";
import { destroyWebPreviewPanel } from "./web-preview-panel";
import { icon } from "./icons";

const FRONTEND_ONLY: AIProvider[] = ["Markdown", "CodeEditor", "WebPreview"];

function isFrontendOnly(p: AIProvider): boolean {
  return typeof p === "string" && (FRONTEND_ONLY as string[]).includes(p);
}

/** Content backed by a PTY process (shell or AI agent) — the only kind that
 *  can be "running", exit, be restarted or be kept alive in the daemon. */
function isPtyContent(t: TabInfo): boolean {
  return t.provider === "Shell" || (typeof t.provider === "object" && "Custom" in t.provider);
}

function providerKey(p: AIProvider): string {
  return typeof p === "string" ? p : p.Custom;
}

/** Content id of a ws-tab's active pane (falling back to the first pane
 *  with content) — what the title, rename and status chip refer to. */
function activeContentId(wt: import("../state").WorkspaceTab): string | null {
  const leaves = allLeaves(wt.paneTree);
  const active = leaves.find((l) => l.id === wt.activePaneId);
  return active?.contentId ?? leaves.find((l) => l.contentId)?.contentId ?? null;
}

/** Title for a top-level tab: the label of its active pane's content. */
function wsTabTitle(tree: PaneNode, activePaneId: PaneId): string {
  const ws = appState.activeWs;
  if (!ws) return "Tab";
  const active = allLeaves(tree).find((l) => l.id === activePaneId) ?? allLeaves(tree)[0];
  const cid = active?.contentId ?? null;
  if (!cid) return "New Tab";
  const content = ws.tabs.find((t) => t.id === cid);
  if (!content) return "New Tab";
  if (content.provider === "CodeEditor") return getCodeEditorFileName(cid) ?? "Editor";
  if (content.provider === "Markdown") return getMarkdownEditorFileName(cid) ?? "Markdown";
  const others = allLeaves(tree).filter((l) => l.contentId).length;
  const base = getTabLabel(content, appState.getTabShellState(cid)?.title);
  return others > 1 ? `${base} +${others - 1}` : base;
}

/** The terminal behind `contentId` rang its bell: flash the chip of the
 *  top-level tab holding it (no-op when that tab isn't on screen). The class
 *  is removed on `animationend` so a second bell restarts the wash. */
export function flashTabChip(contentId: string) {
  const ws = appState.activeWs;
  if (!ws) return;
  const wt = ws.wsTabs.find((t) => allLeaves(t.paneTree).some((l) => l.contentId === contentId));
  if (!wt) return;
  const el = document.querySelector<HTMLElement>(`.ws-tab[data-ws-tab-id="${CSS.escape(wt.id)}"]`);
  if (!el) return;
  el.classList.remove("ws-tab--bell");
  void el.offsetWidth; // restart the animation when a bell repeats
  el.classList.add("ws-tab--bell");
  el.addEventListener("animationend", () => el.classList.remove("ws-tab--bell"), { once: true });
}

/** Worst agent (status, attention) across all content panes in a ws-tab, or
 *  undefined if none of them are Claude agent tabs. Severity is the shared
 *  `agentStatusSeverity` (permission > unseen news > running); quiet states
 *  tie-break idle > done so a waiting prompt still wins over a finished one. */
function wsTabAgentStatus(tree: PaneNode): { status: CliAgentStatus; attention: boolean } | undefined {
  const ws = appState.activeWs;
  if (!ws) return undefined;
  const quiet: Record<CliAgentStatus, number> = {
    "waiting-permission": 3,
    idle: 2,
    done: 1,
    running: 0,
  };
  let best: { status: CliAgentStatus; attention: boolean } | undefined;
  let bestKey = -1;
  for (const leaf of allLeaves(tree)) {
    if (!leaf.contentId) continue;
    const st = appState.getTabShellState(leaf.contentId);
    if (!st?.agentStatus) continue;
    const attention = st.attention ?? false;
    const key = agentStatusSeverity(st.agentStatus, attention) * 10 + quiet[st.agentStatus];
    if (key > bestKey) {
      bestKey = key;
      best = { status: st.agentStatus, attention };
    }
  }
  return best;
}

/** Contents (from `ws.tabs`) held by a ws-tab's pane tree. */
function wsTabContents(ws: { tabs: TabInfo[] }, wt: import("../state").WorkspaceTab): TabInfo[] {
  return allLeaves(wt.paneTree)
    .map((l) => l.contentId)
    .filter((c): c is string => c !== null)
    .map((id) => ws.tabs.find((t) => t.id === id))
    .filter((t): t is TabInfo => !!t);
}

export function renderWorkspaceTabBar(container: HTMLElement) {
  const ws = appState.activeWs;
  // An inline rename is in progress: agent status dots can wait for the
  // next refresh rather than wiping the input under the user's cursor.
  if (container.querySelector(".ws-tab-rename")) return;
  const prevStrip = container.querySelector<HTMLElement>(".ws-tab-strip");
  const prevScroll = prevStrip?.scrollLeft ?? 0;
  container.innerHTML = "";
  container.className = "ws-tab-bar";
  if (!ws) return;

  // Scrolling strip of chips; `+` and `⋯` live outside it, always visible.
  const strip = document.createElement("div");
  strip.className = "ws-tab-strip";
  container.appendChild(strip);

  ws.wsTabs.forEach((wt, i) => {
    const el = document.createElement("div");
    const isActive = i === ws.activeWsTab;
    const exited = wsTabContents(ws, wt).some((c) => isPtyContent(c) && !c.alive);
    el.className = `ws-tab${isActive ? " active" : ""}${exited ? " ws-tab--dead" : ""}`;
    el.dataset.wsTabId = wt.id;
    el.title = wsTabTitle(wt.paneTree, wt.activePaneId) + (exited ? " — process exited" : "");
    const agent = wsTabAgentStatus(wt.paneTree);
    const dot = agent
      ? (() => {
          const v = cliAgentStatusView(agent.status, agent.attention);
          return `<span class="ws-tab-agent" style="color:${v.color}" title="${escapeHtml(v.label)}">${icon("dot")}</span>`;
        })()
      : "";
    const deadMark = exited ? `<span class="ws-tab-dead" title="Process exited">${icon("circle")}</span>` : "";
    el.innerHTML = `
      ${dot}${deadMark}<span class="ws-tab-label">${escapeHtml(wsTabTitle(wt.paneTree, wt.activePaneId))}</span>
      <button class="ws-tab-close" title="Close tab (middle-click also closes)">×</button>
    `;
    el.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest(".ws-tab-close")) return;
      appState.setActiveWsTab(i);
    });
    el.querySelector(".ws-tab-close")!.addEventListener("click", (e) => {
      e.stopPropagation();
      void tearDownAndCloseWsTab(appState.activeWorkspace, i);
    });
    // Middle-click closes through the same safe path as ×. The mousedown
    // guard stops WebKit's autoscroll-on-middle-button.
    el.addEventListener("mousedown", (e) => {
      if (e.button === 1) e.preventDefault();
    });
    el.addEventListener("auxclick", (e) => {
      if (e.button !== 1) return;
      e.preventDefault();
      e.stopPropagation();
      void tearDownAndCloseWsTab(appState.activeWorkspace, i);
    });
    // Rename on double-click; right-click opens the tab menu.
    el.querySelector<HTMLElement>(".ws-tab-label")!.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      beginInlineRename(el, wt);
    });
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      openContextMenu(e.clientX, e.clientY, wsTabMenuItems(i, () => beginInlineRename(el, wt)));
    });
    makeInteractive(el, "tab");
    el.setAttribute("aria-selected", String(isActive));
    strip.appendChild(el);
  });

  const addBtn = document.createElement("button");
  addBtn.className = "ws-tab-add";
  addBtn.title = "New tab";
  addBtn.textContent = "+";
  addBtn.addEventListener("click", () => appState.newBlankTab());
  container.appendChild(addBtn);

  // "All tabs" list — the way to find a tab by name once the strip overflows.
  if (ws.wsTabs.length > 0) {
    const moreBtn = document.createElement("button");
    moreBtn.className = "ws-tab-more";
    moreBtn.title = `All tabs (${ws.wsTabs.length})`;
    moreBtn.setAttribute("aria-label", "List all tabs");
    moreBtn.innerHTML = icon("more");
    moreBtn.addEventListener("click", () => {
      const r = moreBtn.getBoundingClientRect();
      openContextMenu(r.left, r.bottom + 2, allTabsMenuItems());
    });
    container.appendChild(moreBtn);
  }

  // With many tabs the strip scrolls; keep the active tab visible after
  // switching via shortcut/palette, otherwise keep the user's scroll.
  strip.scrollLeft = prevScroll;
  strip
    .querySelector<HTMLElement>(".ws-tab.active")
    ?.scrollIntoView({ inline: "nearest", block: "nearest" });
}

/** One entry per top-level tab of the active workspace: title, agent
 *  status, the current one marked. Picking one switches to it. */
function allTabsMenuItems(): CtxItem[] {
  const ws = appState.activeWs;
  if (!ws) return [];
  return ws.wsTabs.map((wt, i): CtxItem => {
    const agent = wsTabAgentStatus(wt.paneTree);
    const status = agent ? ` — ${cliAgentStatusView(agent.status, agent.attention).label}` : "";
    const exited = wsTabContents(ws, wt).some((c) => isPtyContent(c) && !c.alive) ? " — exited" : "";
    const current = i === ws.activeWsTab ? " (current)" : "";
    return {
      label: `${i + 1}. ${wsTabTitle(wt.paneTree, wt.activePaneId)}${status || exited}${current}`,
      action: () => appState.setActiveWsTab(i),
    };
  });
}

/** Right-click menu of a top-level tab. "Close, keep running" only when a
 *  session daemon can hold the processes; "Move to workspace…" only when
 *  every content is a plain process (editors/boards are bound to their
 *  workspace's files). */
function wsTabMenuItems(wsTabIdx: number, rename: () => void): CtxItem[] {
  const ws = appState.activeWs;
  const wsIdx = appState.activeWorkspace;
  if (!ws) return [];
  const wt = ws.wsTabs[wsTabIdx];
  if (!wt) return [];
  const contents = wsTabContents(ws, wt);
  const live = contents.filter((c) => isPtyContent(c) && c.alive);
  const movable = contents.length > 0 && contents.every((c) => isPtyContent(c));
  const split = (dir: "right" | "down") => {
    appState.setActiveWsTab(wsTabIdx);
    appState.splitActivePane(dir);
  };
  return [
    { label: "Rename", action: rename, disabled: !activeContentId(wt) },
    { separator: true },
    { label: "Split right", action: () => split("right") },
    { label: "Split down", action: () => split("down") },
    { separator: true },
    {
      label: "Move to workspace…",
      disabled: !movable || appState.workspaces.length < 2,
      action: () => void moveWsTabToWorkspace(wsIdx, wsTabIdx),
    },
    { separator: true },
    { label: "Close", danger: true, action: () => void tearDownAndCloseWsTab(wsIdx, wsTabIdx) },
    ...(appState.sessionsAvailable && live.length > 0
      ? [{ label: "Close, keep running", action: () => void tearDownAndCloseWsTab(wsIdx, wsTabIdx, { mode: "detach" }) }]
      : []),
  ];
}

/** Swap the tab's label for an inline input; Enter commits, Esc cancels,
 *  blur commits. Empty clears the custom title. Targets the active pane's
 *  content, i.e. what the label shows. */
function beginInlineRename(tabEl: HTMLElement, wt: import("../state").WorkspaceTab) {
  const ws = appState.activeWs;
  if (!ws) return;
  if (tabEl.querySelector(".ws-tab-rename")) return;
  const contentId = activeContentId(wt);
  if (!contentId) return;
  const content = ws.tabs.find((t) => t.id === contentId);
  if (!content) return;
  const labelEl = tabEl.querySelector<HTMLElement>(".ws-tab-label")!;
  const wsIdx = appState.activeWorkspace;

  const input = document.createElement("input");
  input.className = "ws-tab-rename";
  input.type = "text";
  input.maxLength = 40;
  input.value = content.custom_title ?? labelEl.textContent ?? "";
  input.placeholder = getProviderLabel(content.provider);
  input.setAttribute("aria-label", "Rename tab");
  labelEl.replaceWith(input);

  let done = false;
  const finish = (commit: boolean) => {
    if (done) return;
    done = true;
    const trimmed = input.value.trim().slice(0, 40);
    input.replaceWith(labelEl);
    if (!commit) return;
    const finalTitle = trimmed.length > 0 ? trimmed : null;
    if ((content.custom_title ?? null) === finalTitle) return;
    appState.renameTab(wsIdx, contentId, finalTitle);
    ipc.renameTab(wsIdx, contentId, finalTitle).catch((err) => reportError("Rename tab failed", err));
  };
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { e.preventDefault(); finish(true); }
    else if (e.key === "Escape") { e.preventDefault(); finish(false); }
  });
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("click", (e) => e.stopPropagation());
  input.addEventListener("dblclick", (e) => e.stopPropagation());
  input.addEventListener("mousedown", (e) => e.stopPropagation());
  input.focus();
  input.select();
}

/** Close the active top-level tab (used by the menu bar / shortcut). */
export function closeActiveWsTab() {
  const ws = appState.activeWs;
  if (!ws || ws.wsTabs.length === 0) return;
  void tearDownAndCloseWsTab(appState.activeWorkspace, ws.activeWsTab);
}

/** "Move Tab to Workspace…" for the active top-level tab (menu bar /
 *  palette); the tab's context menu reaches the same chooser. */
export function moveActiveWsTabToWorkspace() {
  const ws = appState.activeWs;
  if (!ws || ws.wsTabs.length === 0) return;
  void moveWsTabToWorkspace(appState.activeWorkspace, ws.activeWsTab);
}

/** Chooser + move: picks a target workspace (`createDropdown` inside the
 *  shared confirm — never a native select), moves every content of the tab
 *  backend-side with its process alive, re-homes the pane tree, then
 *  switches to the target so the tab is in front of the user. Editors and
 *  singleton boards are refused (they belong to their workspace's files). */
export async function moveWsTabToWorkspace(wsIdx: number, wsTabIdx: number) {
  const ws = appState.workspaces[wsIdx];
  const wt = ws?.wsTabs[wsTabIdx];
  if (!ws || !wt) return;
  const contents = wsTabContents(ws, wt);
  if (contents.length === 0 || !contents.every((c) => isPtyContent(c))) {
    toast("Only terminal and agent tabs can move between workspaces", "info");
    return;
  }
  const targets = appState.workspaces
    .map((w, i) => ({ w, i }))
    .filter(({ i }) => i !== wsIdx);
  if (targets.length === 0) {
    toast("No other workspace to move to", "info");
    return;
  }
  const dropdown = createDropdown(
    targets.map(({ w, i }) => ({
      value: String(i),
      label: `${w.info.name} · ${branchLabel(w.branch)}`,
    })),
    String(targets[0].i),
  );
  const title = wsTabTitle(wt.paneTree, wt.activePaneId);
  const toIdx = await new Promise<number | null>((resolve) => {
    const { overlay } = showConfirm({
      bodyHtml: `
        <p>Move <strong>${escapeConfirmHtml(title)}</strong> to:</p>
        <div class="ws-delete-card-move"><span id="ws-move-target-slot"></span></div>
        <p class="ws-delete-hint">The process keeps running; only the tab changes workspace.</p>
      `,
      actions: [
        { label: "Move", kind: "primary", isDefault: true, autofocus: true, onSelect: () => resolve(Number(dropdown.value)) },
        { label: "Cancel", kind: "secondary", onSelect: () => resolve(null) },
      ],
      onDismiss: () => resolve(null),
    });
    overlay.querySelector("#ws-move-target-slot")?.replaceWith(dropdown.container);
  });
  if (toIdx === null || !appState.workspaces[toIdx]) return;

  // Backend first: a content that fails to move stays (and the tab stays).
  const moved: TabInfo[] = [];
  for (const c of contents) {
    try {
      await ipc.moveTab(wsIdx, c.id, toIdx);
      moved.push(c);
    } catch (err) {
      reportError(`Move ${getTabLabel(c)} failed`, err);
    }
  }
  if (moved.length !== contents.length) {
    // Partial move: put the moved ones back so the two sides agree.
    for (const c of moved) {
      try {
        await ipc.moveTab(toIdx, c.id, wsIdx);
      } catch (err) {
        reportError(`Undo move of ${getTabLabel(c)} failed`, err);
      }
    }
    return;
  }
  const idx = ws.wsTabs.indexOf(wt);
  if (idx < 0) return;
  appState.moveWsTab(wsIdx, idx, toIdx);
  const target = appState.workspaces[toIdx];
  try {
    const detail = await ipc.switchWorkspace(toIdx);
    appState.setActiveWorkspace(toIdx, detail);
    toast(`Moved "${title}" to ${target.info.name}`, "info");
  } catch (err) {
    reportError("Workspace switch failed", err);
  }
}

type CloseMode = "kill" | "detach";

/** Human line for one live process, e.g. "claude — needs permission". */
function liveProcessLine(c: TabInfo): string {
  const label = getTabLabel(c);
  const state = appState.getTabShellState(c.id);
  const st = state?.agentStatus;
  if (!st) return label;
  const v = cliAgentStatusView(st, state?.attention ?? false);
  const sum = state?.agentSummary;
  return `${label} — ${v.label}${sum ? `: ${sum.slice(0, 80)}` : ""}`;
}

/** Ask before ending live processes. Resolves with how to proceed, or null
 *  when cancelled. Offers "Keep running" only when a session daemon backs
 *  the tabs (otherwise there is nothing to keep). */
function confirmCloseLive(live: TabInfo[]): Promise<CloseMode | null> {
  if (live.length === 0) return Promise.resolve("kill");
  return new Promise((resolve) => {
    const lines = live.map((c) => `<li>${escapeConfirmHtml(liveProcessLine(c))}</li>`).join("");
    const head = live.length === 1 ? "A process is still running:" : `${live.length} processes are still running:`;
    const keep = appState.sessionsAvailable;
    showConfirm({
      bodyHtml: `
        <p>${head}</p>
        <ul class="ws-delete-list">${lines}</ul>
        <p class="ws-delete-hint">${keep
          ? "Close ends them. Keep running leaves them in the session daemon — find them again in Sessions."
          : "Closing ends them."}</p>
      `,
      actions: [
        { label: "Close", kind: "danger", isDefault: true, onSelect: () => resolve("kill") },
        ...(keep ? [{ label: "Keep running", kind: "secondary" as const, onSelect: () => resolve("detach") }] : []),
        { label: "Cancel", kind: "secondary", autofocus: true, onSelect: () => resolve(null) },
      ],
      onDismiss: () => resolve(null),
    });
  });
}

/** Release the backend side of PTY contents, highest index first so the
 *  indices stay valid as the backend list shrinks. `detach` keeps daemon
 *  sessions alive; a tab that can't be detached (in-process) is reported and
 *  left open. Returns the ids actually released. */
async function releasePtyContents(wsIdx: number, contents: TabInfo[], mode: CloseMode): Promise<Set<string>> {
  const ws = appState.workspaces[wsIdx];
  const released = new Set<string>();
  if (!ws) return released;
  const ordered = contents
    .filter((c) => isPtyContent(c))
    .map((c) => ({ c, idx: ws.tabs.findIndex((t) => t.id === c.id) }))
    .filter(({ idx }) => idx >= 0)
    .sort((a, b) => b.idx - a.idx);
  for (const { c, idx } of ordered) {
    try {
      if (mode === "detach" && c.alive) await ipc.detachTab(wsIdx, idx);
      else await ipc.closeTab(wsIdx, idx);
      released.add(c.id);
    } catch (err) {
      reportError(mode === "detach" ? "Keep running failed" : "Close tab failed", err);
    }
  }
  return released;
}

function destroyFrontendPanel(c: TabInfo) {
  if (c.provider === "Markdown") destroyMarkdownEditorPanel(c.id);
  else if (c.provider === "CodeEditor") destroyCodeEditorPanel(c.id);
  else if (c.provider === "WebPreview") destroyWebPreviewPanel(c.id);
}

/** Tear down every content in a top-level tab (PTYs + frontend panels),
 *  then drop the tab from state. Prompts once if a CodeEditor is dirty and
 *  once if any process is still running (Close / Keep running / Cancel) —
 *  unless `opts.mode` already says what to do with live processes (the tab
 *  menu's "Close, keep running"). */
export async function tearDownAndCloseWsTab(
  wsIdx: number,
  wsTabIdx: number,
  opts: { mode?: CloseMode } = {},
) {
  const ws = appState.workspaces[wsIdx];
  if (!ws) return;
  const wt = ws.wsTabs[wsTabIdx];
  if (!wt) return;
  const contents = wsTabContents(ws, wt);

  const dirtyEditor = contents.find(
    (c) => c.provider === "CodeEditor" && isCodeEditorDirty(c.id),
  );
  if (dirtyEditor) {
    const proceed = await new Promise<boolean>((resolve) =>
      showUnsavedChangesPrompt(dirtyEditor.id, (action) => resolve(action !== "cancel")),
    );
    if (!proceed) return;
  }

  const mode = opts.mode ?? (await confirmCloseLive(contents.filter((c) => isPtyContent(c) && c.alive)));
  if (!mode) return;

  const released = await releasePtyContents(wsIdx, contents, mode);
  const kept = contents.filter((c) => isPtyContent(c) && !released.has(c.id));
  for (const c of contents) destroyFrontendPanel(c);
  for (const id of released) destroyTerminal(id);
  if (kept.length === 0) {
    // Locate the tab again: state may have shifted while we awaited.
    const idx = ws.wsTabs.indexOf(wt);
    if (idx >= 0) appState.closeWsTab(wsIdx, idx);
    return;
  }
  // Some PTYs could not be released: drop the others, keep the tab open.
  for (const id of released) {
    const i = ws.tabs.findIndex((t) => t.id === id);
    if (i >= 0) appState.removeTab(wsIdx, i);
  }
}

/** Tear down a single pane's content (dirty prompt + running-process prompt +
 *  PTY/panel teardown), then remove the pane from the tree. Mirrors
 *  `tearDownAndCloseWsTab` so a pane close never orphans a running session or
 *  drops unsaved edits. */
export async function tearDownAndClosePane(paneId: PaneId) {
  const wsIdx = appState.activeWorkspace;
  const ws = appState.activeWs;
  if (!ws) return;
  const wt = ws.wsTabs[ws.activeWsTab];
  if (!wt) return;
  const pane = findPane(wt.paneTree, paneId);
  const contentId = pane && pane.kind === "leaf" ? pane.contentId : null;
  const content = contentId ? ws.tabs.find((t) => t.id === contentId) : undefined;

  if (content?.provider === "CodeEditor" && isCodeEditorDirty(content.id)) {
    const proceed = await new Promise<boolean>((resolve) =>
      showUnsavedChangesPrompt(content.id, (action) => resolve(action !== "cancel")),
    );
    if (!proceed) return;
  }

  if (content && isPtyContent(content)) {
    const mode = await confirmCloseLive(content.alive ? [content] : []);
    if (!mode) return;
    const released = await releasePtyContents(wsIdx, [content], mode);
    if (!released.has(content.id)) return;
    destroyTerminal(content.id);
  } else if (content) {
    destroyFrontendPanel(content);
  }
  if (findPane(wt.paneTree, paneId)) appState.closePane(paneId);
}

/** Respawn the (exited) process of a pane in place: same provider, same
 *  pane, custom title carried over. Powers the pane-head Restart button. */
export async function restartPaneContent(paneId: PaneId) {
  const wsIdx = appState.activeWorkspace;
  const ws = appState.activeWs;
  if (!ws) return;
  const wt = ws.wsTabs[ws.activeWsTab];
  if (!wt) return;
  const pane = findPane(wt.paneTree, paneId);
  const contentId = pane && pane.kind === "leaf" ? pane.contentId : null;
  const content = contentId ? ws.tabs.find((t) => t.id === contentId) : undefined;
  if (!content || !isPtyContent(content) || content.alive) return;

  const oldIdx = ws.tabs.findIndex((t) => t.id === content.id);
  try {
    if (oldIdx >= 0) await ipc.closeTab(wsIdx, oldIdx);
  } catch (err) {
    reportError("Discard exited tab failed", err);
  }
  destroyTerminal(content.id);
  try {
    const tabId = await ipc.spawnTab(wsIdx, providerKey(content.provider));
    appState.replacePaneContent(paneId, content.id, {
      id: tabId,
      provider: content.provider,
      alive: true,
      custom_title: content.custom_title ?? null,
    });
    if (content.custom_title) {
      ipc.renameTab(wsIdx, tabId, content.custom_title).catch((err) => reportError("Rename tab failed", err));
    }
  } catch (err) {
    reportError(`Restart ${getProviderLabel(content.provider)} failed`, err);
    appState.replacePaneContent(paneId, content.id, null);
  }
}

/** Providers offerable in a blank pane / empty workspace: Shell first, then
 *  the configured agents (from the shared provider cache). */
export async function getPaneProviderChoices(): Promise<AIProvider[]> {
  await preloadProviderTabs();
  return ["Shell", ...getCachedProviderTabs()];
}

/** Spawn `provider` and place it into the (blank) pane `paneId` of the
 *  active workspace tab. */
export async function spawnIntoPane(paneId: PaneId, provider: AIProvider) {
  const wsIdx = appState.activeWorkspace;
  if (appState.isSingletonProvider(provider) && appState.focusSingletonTab(provider)) {
    return;
  }
  try {
    const key = typeof provider === "string" ? provider : provider.Custom;
    const tabId = await ipc.spawnTab(wsIdx, key);
    appState.setPaneContent(paneId, { id: tabId, provider, alive: true });
  } catch (err) {
    reportError(`Open ${getProviderLabel(provider)} failed`, err);
  }
}

/** Spawn `provider` as a new top-level tab of the active workspace (the
 *  empty-workspace state's buttons; same semantics as File ▸ New Tab). */
export async function spawnNewTab(provider: AIProvider) {
  if (appState.focusSingletonTab(provider)) return;
  const wsIdx = appState.activeWorkspace;
  try {
    const tabId = await ipc.spawnTab(wsIdx, providerKey(provider));
    appState.addTab(wsIdx, { id: tabId, provider, alive: true });
  } catch (err) {
    reportError(`Open ${getProviderLabel(provider)} failed`, err);
  }
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
