// Workspace-level (top) tab bar. Each top-level tab owns its own split
// layout (see pane-view.ts); a pane holds at most one content item.

import { appState } from "../state";
import { makeInteractive } from "./a11y";
import * as ipc from "../ipc";
import { toast, reportError } from "./toast";
import { showConfirm, escapeHtml as escapeConfirmHtml } from "./confirm";
import { destroyTerminal } from "./terminal-panel";
import { getProviderLabel, getTabLabel, cliAgentStatusView } from "../types";
import type { AIProvider, TabInfo, CliAgentStatus } from "../types";
import type { PaneId, PaneNode } from "../pane-tree";
import { allLeaves, findPane } from "../pane-tree";
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
  const base = getTabLabel(content);
  return others > 1 ? `${base} +${others - 1}` : base;
}

/** Highest-priority agent status across all content panes in a ws-tab, or
 *  undefined if none of them are Claude agent tabs. Priority favors states
 *  that need the user: permission > idle > done > running. */
function wsTabAgentStatus(tree: PaneNode): CliAgentStatus | undefined {
  const ws = appState.activeWs;
  if (!ws) return undefined;
  const rank: Record<CliAgentStatus, number> = {
    "waiting-permission": 3,
    idle: 2,
    done: 1,
    running: 0,
  };
  let best: CliAgentStatus | undefined;
  for (const leaf of allLeaves(tree)) {
    if (!leaf.contentId) continue;
    const st = appState.getTabShellState(leaf.contentId)?.agentStatus;
    if (st && (best === undefined || rank[st] > rank[best])) best = st;
  }
  return best;
}

export function renderWorkspaceTabBar(container: HTMLElement) {
  const ws = appState.activeWs;
  // An inline rename is in progress: agent status dots can wait for the
  // next refresh rather than wiping the input under the user's cursor.
  if (container.querySelector(".ws-tab-rename")) return;
  container.innerHTML = "";
  container.className = "ws-tab-bar";
  if (!ws) return;

  ws.wsTabs.forEach((wt, i) => {
    const el = document.createElement("div");
    const isActive = i === ws.activeWsTab;
    const exited = allLeaves(wt.paneTree).some((l) => {
      const c = l.contentId ? ws.tabs.find((t) => t.id === l.contentId) : undefined;
      return !!c && isPtyContent(c) && !c.alive;
    });
    el.className = `ws-tab${isActive ? " active" : ""}${exited ? " ws-tab--dead" : ""}`;
    el.title = wsTabTitle(wt.paneTree, wt.activePaneId) + (exited ? " — process exited" : "");
    const agent = wsTabAgentStatus(wt.paneTree);
    const dot = agent
      ? (() => {
          const v = cliAgentStatusView(agent);
          return `<span class="ws-tab-agent" style="color:${v.color}" title="${escapeHtml(v.label)}">●</span>`;
        })()
      : "";
    const deadMark = exited ? `<span class="ws-tab-dead" title="Process exited">○</span>` : "";
    el.innerHTML = `
      ${dot}${deadMark}<span class="ws-tab-label">${escapeHtml(wsTabTitle(wt.paneTree, wt.activePaneId))}</span>
      <button class="ws-tab-close" title="Close tab">×</button>
    `;
    el.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest(".ws-tab-close")) return;
      appState.setActiveWsTab(i);
    });
    el.querySelector(".ws-tab-close")!.addEventListener("click", (e) => {
      e.stopPropagation();
      void tearDownAndCloseWsTab(appState.activeWorkspace, i);
    });
    // Rename on double-click or right-click
    const doRename = () => beginInlineRename(el, wt);
    el.querySelector<HTMLElement>(".ws-tab-label")!.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      doRename();
    });
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      doRename();
    });
    makeInteractive(el, "tab");
    el.setAttribute("aria-selected", String(isActive));
    container.appendChild(el);
  });

  const addBtn = document.createElement("button");
  addBtn.className = "ws-tab-add";
  addBtn.title = "New tab";
  addBtn.textContent = "+";
  addBtn.addEventListener("click", () => appState.newBlankTab());
  container.appendChild(addBtn);

  // With many tabs the bar scrolls; keep the active tab visible after
  // switching via shortcut/palette.
  container
    .querySelector<HTMLElement>(".ws-tab.active")
    ?.scrollIntoView({ inline: "nearest", block: "nearest" });
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

type CloseMode = "kill" | "detach";

/** Human line for one live process, e.g. "claude — needs permission". */
function liveProcessLine(c: TabInfo): string {
  const label = getTabLabel(c);
  const st = appState.getTabShellState(c.id)?.agentStatus;
  if (!st) return label;
  const v = cliAgentStatusView(st);
  const sum = appState.getTabShellState(c.id)?.agentSummary;
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
 *  once if any process is still running (Close / Keep running / Cancel). */
export async function tearDownAndCloseWsTab(wsIdx: number, wsTabIdx: number) {
  const ws = appState.workspaces[wsIdx];
  if (!ws) return;
  const wt = ws.wsTabs[wsTabIdx];
  if (!wt) return;
  const contentIds = allLeaves(wt.paneTree)
    .map((l) => l.contentId)
    .filter((c): c is string => c !== null);
  const contents = contentIds
    .map((id) => ws.tabs.find((t) => t.id === id))
    .filter((t): t is TabInfo => !!t);

  const dirtyEditor = contents.find(
    (c) => c.provider === "CodeEditor" && isCodeEditorDirty(c.id),
  );
  if (dirtyEditor) {
    const proceed = await new Promise<boolean>((resolve) =>
      showUnsavedChangesPrompt(dirtyEditor.id, (action) => resolve(action !== "cancel")),
    );
    if (!proceed) return;
  }

  const mode = await confirmCloseLive(contents.filter((c) => isPtyContent(c) && c.alive));
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

/** Providers offerable in a blank pane chooser: Shell + configured agents. */
export async function getPaneProviderChoices(): Promise<AIProvider[]> {
  let custom: AIProvider[] = [];
  try {
    const list = await ipc.listProviders();
    custom = list.map((p): AIProvider => ({ Custom: p.name }));
  } catch {
    custom = [];
  }
  return [...custom, "Shell"];
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
    toast(`Failed to open: ${err}`, "error");
  }
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
