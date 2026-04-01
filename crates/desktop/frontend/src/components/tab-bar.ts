import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { PROVIDER_LABELS, PROVIDER_ICONS } from "../types";
import type { AIProvider } from "../types";

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

      const icon = PROVIDER_ICONS[tab.provider] || "?";
      const label = PROVIDER_LABELS[tab.provider] || tab.provider;

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

const NEW_TAB_PROVIDERS: AIProvider[] = [
  "Claude",
  "Gemini",
  "OpenCode",
  "Kilo",
  "Codex",
  "Shell",
];

function showNewTabMenu(anchor: HTMLElement) {
  // Remove any existing menu
  document.querySelector(".tab-new-menu")?.remove();

  const menu = document.createElement("div");
  menu.className = "tab-new-menu";
  menu.style.cssText = `
    position: absolute;
    top: ${anchor.getBoundingClientRect().bottom}px;
    left: ${anchor.getBoundingClientRect().left}px;
    background: var(--bg-dropdown);
    border: 1px solid var(--border-primary);
    border-radius: 4px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    padding: 4px 0;
    z-index: 50;
    min-width: 160px;
  `;

  for (const provider of NEW_TAB_PROVIDERS) {
    const item = document.createElement("button");
    item.style.cssText = `
      display: flex;
      align-items: center;
      gap: 8px;
      width: 100%;
      padding: 6px 12px;
      font-size: 13px;
      color: var(--text-primary);
      text-align: left;
    `;
    item.innerHTML = `
      <span style="width:16px;text-align:center;color:var(--text-muted)">${PROVIDER_ICONS[provider]}</span>
      ${PROVIDER_LABELS[provider]}
    `;
    item.addEventListener("click", async () => {
      menu.remove();
      try {
        const tabId = await ipc.spawnTab(appState.activeWorkspace, provider);
        appState.addTab(appState.activeWorkspace, {
          id: tabId,
          provider,
          alive: true,
        });
      } catch (err) {
        toast(
          `Failed to open ${PROVIDER_LABELS[provider]}: ${err}`,
          "error",
        );
      }
    });
    item.addEventListener("mouseenter", () => {
      item.style.background = "var(--sidebar-item-hover)";
    });
    item.addEventListener("mouseleave", () => {
      item.style.background = "";
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
