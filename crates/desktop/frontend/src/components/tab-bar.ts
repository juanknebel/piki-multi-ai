import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { getProviderLabel, getProviderIcon, getProviderKey } from "../types";
import type { AIProvider } from "../types";
import type { LeafNode } from "../pane-tree";
import {
  destroyMarkdownEditorPanel,
  getMarkdownEditorFileName,
  getMarkdownEditorFilePath,
} from "./markdown-editor-panel";
import {
  destroyCodeEditorPanel,
  getCodeEditorFileName,
  getCodeEditorFilePath,
  isCodeEditorDirty,
  showUnsavedChangesPrompt,
} from "./code-editor-panel";
import { destroyWebPreviewPanel } from "./web-preview-panel";
import { revealInFileTree } from "./file-tree";

/**
 * Render a pane's mini tab bar into `container`. Each leaf calls this with its
 * own tab list and active tab id. The bar handles selection, closing tabs, and
 * adding new tabs into this pane.
 */
export function renderPaneTabBar(container: HTMLElement, leaf: LeafNode) {
  const ws = appState.activeWs;
  if (!ws) return;

  container.innerHTML = "";

  for (const tabId of leaf.tabIds) {
    const tab = ws.tabs.find((t) => t.id === tabId);
    if (!tab) continue;

    const el = document.createElement("div");
    const isActive = tabId === leaf.activeTabId;
    el.className = `tab${isActive ? " active" : ""}`;
    el.dataset.tabId = tabId;

    const icon = getProviderIcon(tab.provider);
    let label = getProviderLabel(tab.provider);
    if (tab.provider === "CodeEditor") {
      label = getCodeEditorFileName(tab.id) ?? label;
    } else if (tab.provider === "Markdown") {
      label = getMarkdownEditorFileName(tab.id) ?? label;
    }

    // Shell tabs with shell integration get a small exit-code dot:
    // green ✓ when the last command exited 0, red ✗ otherwise. Hidden
    // until a command has actually run.
    let exitBadge = "";
    if (tab.provider === "Shell") {
      const shellState = appState.getTabShellState(tab.id);
      if (shellState?.lastExitCode !== undefined) {
        const ok = shellState.lastExitCode === 0;
        exitBadge = `<span class="tab-exit-badge ${ok ? "ok" : "fail"}" title="Last command exit ${shellState.lastExitCode}">${ok ? "✓" : "✗"}</span>`;
      }
    }

    el.innerHTML = `
      <span class="tab-icon">${icon}</span>
      ${exitBadge}
      <span class="tab-label">${escapeHtml(label)}</span>
      <button class="tab-close" title="Close">×</button>
    `;

    el.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      if (target.closest(".tab-close")) return;
      appState.setActiveTabInPane(leaf.id, tabId);
    });

    if (tab.provider === "CodeEditor" || tab.provider === "Markdown") {
      el.addEventListener("contextmenu", (e) => {
        const path =
          tab.provider === "CodeEditor"
            ? getCodeEditorFilePath(tab.id)
            : getMarkdownEditorFilePath(tab.id);
        if (!path) return;
        e.preventDefault();
        revealInFileTree(path);
      });
    }

    const closeBtn = el.querySelector(".tab-close");
    if (closeBtn) {
      closeBtn.addEventListener("click", async (e) => {
        e.stopPropagation();
        await closeTab(tab);
      });
    }

    container.appendChild(el);
  }

  // "+" button: open the new-tab menu, anchored to this leaf so the new tab
  // lands here.
  const addBtn = document.createElement("button");
  addBtn.className = "tab-add";
  addBtn.title = "New Tab";
  addBtn.textContent = "+";
  addBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    appState.setActivePane(leaf.id);
    showNewTabMenu(addBtn, leaf);
  });
  container.appendChild(addBtn);
}

async function closeTab(tab: { id: string; provider: AIProvider }) {
  const ws = appState.activeWs;
  if (!ws) return;
  const idx = ws.tabs.findIndex((t) => t.id === tab.id);
  if (idx < 0) return;

  // Frontend-only tabs (no backend PTY) — just remove from state
  if (tab.provider === "Markdown") {
    destroyMarkdownEditorPanel(tab.id);
    appState.removeTab(appState.activeWorkspace, idx);
    return;
  }
  if (tab.provider === "WebPreview") {
    destroyWebPreviewPanel(tab.id);
    appState.removeTab(appState.activeWorkspace, idx);
    return;
  }
  if (tab.provider === "CodeEditor") {
    if (isCodeEditorDirty(tab.id)) {
      showUnsavedChangesPrompt(tab.id, (action) => {
        if (action === "cancel") return;
        // Save and discard both proceed with closing.
        destroyCodeEditorPanel(tab.id);
        const currentIdx = appState.activeWs?.tabs.findIndex((t) => t.id === tab.id) ?? -1;
        if (currentIdx >= 0) {
          appState.removeTab(appState.activeWorkspace, currentIdx);
        }
      });
      return;
    }
    destroyCodeEditorPanel(tab.id);
    appState.removeTab(appState.activeWorkspace, idx);
    return;
  }
  try {
    await ipc.closeTab(appState.activeWorkspace, idx);
    const currentIdx = appState.activeWs?.tabs.findIndex((t) => t.id === tab.id) ?? -1;
    if (currentIdx >= 0) {
      appState.removeTab(appState.activeWorkspace, currentIdx);
    }
  } catch (err) {
    console.error("Failed to close tab:", err);
  }
}

// Built-in tool tabs always shown in the "+" menu
const TOOL_TABS: AIProvider[] = ["Shell"];

async function showNewTabMenu(anchor: HTMLElement, leaf: LeafNode) {
  // Remove any existing menu
  document.querySelector(".tab-new-menu")?.remove();

  // Load providers dynamically from providers.toml
  let configuredProviders: AIProvider[] = [];
  try {
    const providerList = await ipc.listProviders();
    configuredProviders = providerList.map((p): AIProvider => ({ Custom: p.name }));
  } catch {
    configuredProviders = [];
  }

  // Combine: configured providers + tool tabs
  const allProviders: AIProvider[] = [...configuredProviders, ...TOOL_TABS];

  const splitActions: { label: string; icon: string; dir: "right" | "down" }[] = [
    { label: "Split Right", icon: "⇥", dir: "right" },
    { label: "Split Down", icon: "⤓", dir: "down" },
  ];

  const menu = document.createElement("div");
  menu.className = "tab-new-menu";
  menu.style.cssText = `
    position: absolute;
    top: ${anchor.getBoundingClientRect().bottom + 2}px;
    left: ${anchor.getBoundingClientRect().left}px;
    background: var(--bg-dropdown);
    border: 1px solid var(--dialog-border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0,0,0,0.5), 0 0 1px rgba(255,255,255,0.05);
    padding: 4px 0;
    z-index: 50;
    min-width: 180px;
    animation: dialog-enter 0.12s cubic-bezier(0.16,1,0.3,1);
  `;

  for (const provider of allProviders) {
    const item = document.createElement("button");
    item.style.cssText = `
      display: flex;
      align-items: center;
      gap: 10px;
      width: 100%;
      padding: 7px 14px;
      font-size: 12px;
      color: var(--text-primary);
      text-align: left;
      transition: background 0.1s, color 0.1s;
      border-radius: 0;
    `;
    item.innerHTML = `
      <span style="width:16px;text-align:center;color:var(--text-muted);font-size:11px">${getProviderIcon(provider)}</span>
      ${getProviderLabel(provider)}
    `;
    item.addEventListener("click", async () => {
      menu.remove();
      if (!appState.activeWs) {
        toast("Create a workspace first", "error");
        return;
      }
      try {
        const tabId = await ipc.spawnTab(appState.activeWorkspace, getProviderKey(provider));
        appState.addTab(appState.activeWorkspace, {
          id: tabId,
          provider,
          alive: true,
        });
      } catch (err) {
        toast(
          `Failed to open ${getProviderLabel(provider)}: ${err}`,
          "error",
        );
      }
    });
    item.addEventListener("mouseenter", () => {
      item.style.background = "var(--bg-active)";
      item.style.color = "var(--text-bright)";
    });
    item.addEventListener("mouseleave", () => {
      item.style.background = "";
      item.style.color = "var(--text-primary)";
    });
    menu.appendChild(item);
  }

  const divider = document.createElement("div");
  divider.style.cssText = "height:1px;background:var(--dialog-border);margin:4px 0;";
  menu.appendChild(divider);

  for (const action of splitActions) {
    const item = document.createElement("button");
    item.style.cssText = `
      display: flex;
      align-items: center;
      gap: 10px;
      width: 100%;
      padding: 7px 14px;
      font-size: 12px;
      color: var(--text-primary);
      text-align: left;
      transition: background 0.1s, color 0.1s;
      border-radius: 0;
    `;
    item.innerHTML = `
      <span style="width:16px;text-align:center;color:var(--text-muted);font-size:11px">${action.icon}</span>
      ${action.label}
    `;
    item.addEventListener("click", () => {
      menu.remove();
      appState.splitPane(leaf.id, action.dir);
    });
    item.addEventListener("mouseenter", () => {
      item.style.background = "var(--bg-active)";
      item.style.color = "var(--text-bright)";
    });
    item.addEventListener("mouseleave", () => {
      item.style.background = "";
      item.style.color = "var(--text-primary)";
    });
    menu.appendChild(item);
  }

  document.body.appendChild(menu);

  // Close on outside click
  const close = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) {
      menu.remove();
      document.removeEventListener("click", close);
    }
  };
  setTimeout(() => document.addEventListener("click", close), 0);
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
