import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { getProviderLabel, getProviderIcon, getProviderKey } from "../types";
import type { AIProvider } from "../types";
import type { LeafNode } from "../pane-tree";
import { allLeaves } from "../pane-tree";
import { destroyMarkdownEditorPanel } from "./markdown-editor-panel";
import {
  destroyCodeEditorPanel,
  isCodeEditorDirty,
  showUnsavedChangesPrompt,
} from "./code-editor-panel";
import { destroyWebPreviewPanel } from "./web-preview-panel";

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
    const label = getProviderLabel(tab.provider);

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
      <button class="tab-menu" title="Tab Options">▾</button>
      <button class="tab-close" title="Close">×</button>
    `;

    el.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      if (target.closest(".tab-close") || target.closest(".tab-menu")) return;
      appState.setActiveTabInPane(leaf.id, tabId);
    });

    const menuBtn = el.querySelector<HTMLButtonElement>(".tab-menu");
    if (menuBtn) {
      menuBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        showTabActionMenu(menuBtn, tab, leaf);
      });
    }

    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      showTabActionMenu(el, tab, leaf);
    });

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
    showNewTabMenu(addBtn);
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

interface ActionMenuItem {
  label: string;
  onSelect?: () => void;
  disabled?: boolean;
  submenu?: ActionMenuItem[];
}

function showActionMenu(anchor: HTMLElement, items: ActionMenuItem[]) {
  document.querySelectorAll(".tab-action-menu").forEach((m) => m.remove());

  const rect = anchor.getBoundingClientRect();
  const menu = document.createElement("div");
  menu.className = "tab-action-menu";
  menu.style.cssText = `
    position: absolute;
    top: ${rect.bottom + 2}px;
    left: ${rect.left}px;
    background: var(--bg-dropdown);
    border: 1px solid var(--dialog-border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0,0,0,0.5), 0 0 1px rgba(255,255,255,0.05);
    padding: 4px 0;
    z-index: 50;
    min-width: 180px;
    animation: dialog-enter 0.12s cubic-bezier(0.16,1,0.3,1);
  `;

  for (const item of items) {
    const btn = document.createElement("button");
    btn.style.cssText = `
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      width: 100%;
      padding: 7px 14px;
      font-size: 12px;
      color: var(--text-primary);
      text-align: left;
      transition: background 0.1s, color 0.1s;
      border-radius: 0;
      ${item.disabled ? "opacity: 0.4; pointer-events: none;" : ""}
    `;
    btn.innerHTML = item.submenu
      ? `<span>${escapeHtml(item.label)}</span><span style="color:var(--text-muted)">›</span>`
      : `<span>${escapeHtml(item.label)}</span>`;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      if (item.submenu) {
        showActionMenu(btn, item.submenu);
        return;
      }
      menu.remove();
      document.removeEventListener("click", close);
      item.onSelect?.();
    });
    btn.addEventListener("mouseenter", () => {
      btn.style.background = "var(--bg-active)";
      btn.style.color = "var(--text-bright)";
    });
    btn.addEventListener("mouseleave", () => {
      btn.style.background = "";
      btn.style.color = "var(--text-primary)";
    });
    menu.appendChild(btn);
  }

  document.body.appendChild(menu);

  const close = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) {
      document.querySelectorAll(".tab-action-menu").forEach((m) => m.remove());
      document.removeEventListener("click", close);
    }
  };
  setTimeout(() => document.addEventListener("click", close), 0);
}

function showTabActionMenu(anchor: HTMLElement, tab: { id: string; provider: AIProvider }, leaf: LeafNode) {
  const ws = appState.activeWs;
  if (!ws) return;
  const leaves = allLeaves(ws.paneTree).filter((l) => l.id !== leaf.id);

  const items: ActionMenuItem[] = [
    {
      label: "Split Right",
      onSelect: () => appState.splitPane(leaf.id, "right", tab.id),
    },
    {
      label: "Split Down",
      onSelect: () => appState.splitPane(leaf.id, "down", tab.id),
    },
  ];

  if (leaves.length > 0) {
    items.push({
      label: "Move to Pane",
      submenu: leaves.map((l, idx) => ({
        label: `Pane ${idx + 2}`,
        onSelect: () => appState.moveTabToPane(tab.id, l.id),
      })),
    });
  }

  items.push({
    label: "Close",
    onSelect: () => { void closeTab(tab); },
  });

  showActionMenu(anchor, items);
}

// Built-in tool tabs always shown in the "+" menu
const TOOL_TABS: AIProvider[] = ["Shell", "Api"];

async function showNewTabMenu(anchor: HTMLElement) {
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
