import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { getProviderLabel, getProviderIcon, getProviderKey } from "../types";
import type { AIProvider } from "../types";
import { destroyMarkdownEditorPanel } from "./markdown-editor-panel";
import { destroyCodeEditorPanel } from "./code-editor-panel";

export function renderTabBar(container: HTMLElement) {
  function render() {
    const ws = appState.activeWs;
    const tabs = ws?.tabs ?? [];
    const activeTab = ws?.activeTab ?? 0;

    container.innerHTML = "";

    tabs.forEach((tab, idx) => {
      const el = document.createElement("div");
      el.className = `tab${idx === activeTab ? " active" : ""}`;
      el.dataset.idx = String(idx);

      const icon = getProviderIcon(tab.provider);
      const label = getProviderLabel(tab.provider);

      el.innerHTML = `
        <span class="tab-icon">${icon}</span>
        <span class="tab-label">${escapeHtml(label)}</span>
        <button class="tab-close" title="Close">×</button>
      `;

      el.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest(".tab-close")) return;
        appState.setActiveTab(idx);
      });

      const closeBtn = el.querySelector(".tab-close");
      if (closeBtn) {
        closeBtn.addEventListener("click", async (e) => {
          e.stopPropagation();
          const tab = tabs[idx];
          // Frontend-only tabs (no backend PTY) — just remove from state
          if (tab.provider === "Markdown") {
            destroyMarkdownEditorPanel(tab.id);
            appState.removeTab(appState.activeWorkspace, idx);
            return;
          }
          if (tab.provider === "CodeEditor") {
            destroyCodeEditorPanel(tab.id);
            appState.removeTab(appState.activeWorkspace, idx);
            return;
          }
          try {
            await ipc.closeTab(appState.activeWorkspace, idx);
            appState.removeTab(appState.activeWorkspace, idx);
          } catch (err) {
            console.error("Failed to close tab:", err);
          }
        });
      }

      container.appendChild(el);
    });

    // Add tab button
    const addBtn = document.createElement("button");
    addBtn.className = "tab-add";
    addBtn.title = "New Tab";
    addBtn.textContent = "+";
    addBtn.addEventListener("click", () => showNewTabMenu(addBtn));
    container.appendChild(addBtn);
  }

  appState.on("tabs-changed", render);
  appState.on("active-tab-changed", render);
  appState.on("active-workspace-changed", render);
  render();
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
    configuredProviders = providerList.map((p): AIProvider => {
      // Map known names to built-in variants
      const builtinMap: Record<string, AIProvider> = {
        "Claude Code": "Claude",
        "Gemini": "Gemini",
        "OpenCode": "OpenCode",
        "Kilo": "Kilo",
        "Codex": "Codex",
      };
      return builtinMap[p.name] ?? { Custom: p.name };
    });
  } catch {
    // Fallback: just Claude
    configuredProviders = ["Claude"];
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
