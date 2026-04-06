import { appState } from "../../state";
import * as ipc from "../../ipc";
import { PROVIDER_LABELS } from "../../types";

export function showDashboard() {
  document.querySelector(".dialog-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.style.paddingTop = "5vh";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "800px";
  dialog.style.maxHeight = "80vh";

  const workspaces = appState.workspaces;
  const activeIdx = appState.activeWorkspace;

  // Group workspaces by group field
  type WsEntry = { idx: number; group: string; order: number };
  const entries: WsEntry[] = workspaces.map((ws, i) => ({
    idx: i,
    group: ws.info.group || "",
    order: ws.info.order,
  }));

  const groups = new Map<string, WsEntry[]>();
  for (const entry of entries) {
    if (!groups.has(entry.group)) groups.set(entry.group, []);
    groups.get(entry.group)!.push(entry);
  }
  const sortedGroups = [...groups.entries()]
    .sort(([a], [b]) => {
      if (a === "" && b !== "") return -1;
      if (a !== "" && b === "") return 1;
      return a.localeCompare(b);
    })
    .map(([group, items]) => ({
      group,
      items: items.sort((a, b) => a.order - b.order),
    }));

  let cardsHtml = "";
  for (const section of sortedGroups) {
    if (section.group) {
      cardsHtml += `<div class="dash-group-header">${esc(section.group)}</div>`;
    }
    for (const entry of section.items) {
      const ws = workspaces[entry.idx];
      const info = ws.info;
      const isActive = entry.idx === activeIdx;
      const statusLabel = typeof ws.status === "string" ? ws.status : "Error";
      const statusClass = statusLabel.toLowerCase();
      const fileCount = ws.changedFiles.length;
      const tabCount = ws.tabs.length;
      const tabLabels = ws.tabs.map(t => PROVIDER_LABELS[t.provider] || t.provider).join(", ");
      const ab = ws.aheadBehind;
      const syncInfo = ab ? `↑${ab[0]} ↓${ab[1]}` : "";

      cardsHtml += `
        <div class="dash-card${isActive ? " dash-active" : ""}" data-idx="${entry.idx}">
          <div class="dash-card-header">
            <span class="dash-card-name">${esc(info.name)}</span>
            <span class="dash-card-status ${statusClass}">${statusLabel}</span>
          </div>
          <div class="dash-card-branch">⎇ ${esc(info.branch)} ${syncInfo}</div>
          <div class="dash-card-meta">
            <span>${fileCount} change${fileCount !== 1 ? "s" : ""}</span>
            <span>${tabCount} tab${tabCount !== 1 ? "s" : ""}${tabLabels ? ": " + esc(tabLabels) : ""}</span>
          </div>
          ${info.description ? `<div class="dash-card-desc">${esc(info.description)}</div>` : ""}
        </div>
      `;
    }
  }

  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title">Dashboard — ${workspaces.length} workspace${workspaces.length !== 1 ? "s" : ""}</span>
      <button class="dialog-close">×</button>
    </div>
    <div class="dash-grid">${cardsHtml || '<div class="empty-message">No workspaces</div>'}</div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  // Click card to switch
  dialog.querySelectorAll<HTMLElement>(".dash-card").forEach((card) => {
    card.addEventListener("click", async () => {
      const idx = parseInt(card.dataset.idx!, 10);
      close();
      try {
        const detail = await ipc.switchWorkspace(idx);
        appState.setActiveWorkspace(idx, detail);
      } catch (err) {
        console.error("Switch failed:", err);
      }
    });
  });

  const close = () => backdrop.remove();
  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
