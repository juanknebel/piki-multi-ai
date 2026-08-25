import { appState } from "../../state";
import { reportError } from "../toast";
import * as ipc from "../../ipc";
import { getTabLabel } from "../../types";

export function showDashboard() {
  document.querySelector(".dashboard-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop dashboard-backdrop";
  backdrop.style.paddingTop = "5vh";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "800px";
  dialog.style.maxHeight = "80vh";

  const workspaces = appState.workspaces;
  const activeIdx = appState.activeWorkspace;

  // Only workspaces with something open (tabs), ordered parent -> children
  // per worktree family (same rule as TUI `dashboard_indices` and sidebar).
  const filteredIdx = workspaces
    .map((_, i) => i)
    .filter((i) => workspaces[i].tabs.length > 0);

  // Group filtered workspaces by source_repo to enforce parent-first order.
  const bySourceRepo = new Map<string, number[]>();
  for (const idx of filteredIdx) {
    const repo = workspaces[idx].info.source_repo;
    if (!bySourceRepo.has(repo)) bySourceRepo.set(repo, []);
    bySourceRepo.get(repo)!.push(idx);
  }

  // Build ordered list: families in order of first appearance, parent first.
  const orderedIdx: number[] = [];
  const seenRepo = new Set<string>();
  for (const idx of filteredIdx) {
    const repo = workspaces[idx].info.source_repo;
    if (seenRepo.has(repo)) continue;
    seenRepo.add(repo);
    const members = bySourceRepo.get(repo)!;
    if (members.length <= 1) {
      orderedIdx.push(members[0]);
    } else {
      const parent = members.find((m) => workspaces[m].info.workspace_type !== "Worktree");
      if (parent !== undefined) {
        orderedIdx.push(parent);
        const children = members
          .filter((m) => m !== parent)
          .sort((a, b) => workspaces[a].info.order - workspaces[b].info.order);
        orderedIdx.push(...children);
      } else {
        members.sort((a, b) => workspaces[a].info.order - workspaces[b].info.order);
        orderedIdx.push(...members);
      }
    }
  }

  // Optional: group header by folder name for families with >1 member,
  // mirroring previous grouping but now on the ordered filtered set.
  function folderName(sourceRepo: string): string {
    return sourceRepo.replace(/\/+$/, "").split("/").pop() || sourceRepo;
  }

  let cardsHtml = "";
  let lastRepo: string | null = null;
  for (const idx of orderedIdx) {
    const ws = workspaces[idx];
    const repo = ws.info.source_repo;
    const familySize = bySourceRepo.get(repo)!.length;
    // Show a group header when entering a new family with >1 member
    if (familySize > 1 && repo !== lastRepo) {
      cardsHtml += `<div class="dash-group-header">${esc(folderName(repo))}</div>`;
    }
    lastRepo = repo;
    {
      const info = ws.info;
      const isActive = idx === activeIdx;
      const statusLabel = typeof ws.status === "string" ? ws.status : "Error";
      const statusClass = statusLabel.toLowerCase();
      const fileCount = ws.changedFiles.length;
      const tabCount = ws.tabs.length;
      const tabLabels = ws.tabs.map(t => getTabLabel(t)).join(", ");
      const ab = ws.aheadBehind;
      const syncInfo = ab ? `↑${ab[0]} ↓${ab[1]}` : "";
      const branch = ws.branch;

      cardsHtml += `
        <div class="dash-card${isActive ? " dash-active" : ""}" data-idx="${idx}">
          <div class="dash-card-header">
            <span class="dash-card-name">${esc(info.name)}</span>
            <span class="dash-card-status ${statusClass}">${statusLabel}</span>
          </div>
          ${branch || syncInfo ? `<div class="dash-card-branch">${branch ? "⎇ " + esc(branch) : ""}${syncInfo ? (branch ? " " : "") + syncInfo : ""}</div>` : ""}
          <div class="dash-card-meta">
            <span>${fileCount} change${fileCount !== 1 ? "s" : ""}</span>
            <span>${tabCount} tab${tabCount !== 1 ? "s" : ""}${tabLabels ? ": " + esc(tabLabels) : ""}</span>
          </div>
          ${info.description ? `<div class="dash-card-desc">${esc(info.description)}</div>` : ""}
        </div>
      `;
    }
  }

  const totalShown = orderedIdx.length;
  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title">Dashboard — ${totalShown} workspace${totalShown !== 1 ? "s" : ""}</span>
      <button class="dialog-close" title="Close" aria-label="Close">×</button>
    </div>
    <div class="dash-grid">${cardsHtml || '<div class="empty-message">No open workspaces</div>'}</div>
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
        reportError("Workspace switch failed", err);
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
