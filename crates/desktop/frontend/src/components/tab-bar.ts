// Workspace-level (top) tab bar. Each top-level tab owns its own split
// layout (see pane-view.ts); a pane holds at most one content item.

import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { getProviderLabel, cliAgentStatusView } from "../types";
import type { AIProvider, TabInfo, CliAgentStatus } from "../types";
import type { PaneId, PaneNode } from "../pane-tree";
import { allLeaves } from "../pane-tree";
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
  const base = getProviderLabel(content.provider);
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
  container.innerHTML = "";
  container.className = "ws-tab-bar";
  if (!ws) return;

  ws.wsTabs.forEach((wt, i) => {
    const el = document.createElement("div");
    const isActive = i === ws.activeWsTab;
    el.className = `ws-tab${isActive ? " active" : ""}`;
    el.title = wsTabTitle(wt.paneTree, wt.activePaneId);
    const agent = wsTabAgentStatus(wt.paneTree);
    const dot = agent
      ? (() => {
          const v = cliAgentStatusView(agent);
          return `<span class="ws-tab-agent" style="color:${v.color}" title="${escapeHtml(v.label)}">●</span>`;
        })()
      : "";
    el.innerHTML = `
      ${dot}<span class="ws-tab-label">${escapeHtml(el.title)}</span>
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
    container.appendChild(el);
  });

  const addBtn = document.createElement("button");
  addBtn.className = "ws-tab-add";
  addBtn.title = "New tab";
  addBtn.textContent = "+";
  addBtn.addEventListener("click", () => appState.newBlankTab());
  container.appendChild(addBtn);
}

/** Close the active top-level tab (used by the menu bar / shortcut). */
export function closeActiveWsTab() {
  const ws = appState.activeWs;
  if (!ws || ws.wsTabs.length === 0) return;
  void tearDownAndCloseWsTab(appState.activeWorkspace, ws.activeWsTab);
}

/** Tear down every content in a top-level tab (PTYs + frontend panels),
 *  then drop the tab from state. Prompts once if a CodeEditor is dirty. */
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
  const finish = () => {
    // Backend PTYs: close by current index, descending so indices stay valid.
    const ptyIdx = contents
      .filter((c) => !isFrontendOnly(c.provider))
      .map((c) => ws.tabs.findIndex((t) => t.id === c.id))
      .filter((idx) => idx >= 0)
      .sort((a, b) => b - a);
    for (const idx of ptyIdx) ipc.closeTab(wsIdx, idx).catch(() => {});
    for (const c of contents) {
      if (c.provider === "Markdown") destroyMarkdownEditorPanel(c.id);
      else if (c.provider === "CodeEditor") destroyCodeEditorPanel(c.id);
      else if (c.provider === "WebPreview") destroyWebPreviewPanel(c.id);
    }
    appState.closeWsTab(wsIdx, wsTabIdx);
  };

  if (dirtyEditor) {
    showUnsavedChangesPrompt(dirtyEditor.id, (action) => {
      if (action === "cancel") return;
      finish();
    });
    return;
  }
  finish();
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
