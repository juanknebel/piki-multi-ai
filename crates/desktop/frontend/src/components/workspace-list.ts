import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import {
  showWorkspaceDialog,
  showWorkspaceInfo,
} from "./dialogs/workspace-dialog";
import type { WorkspaceInfo } from "../types";

export function renderWorkspaceList(container: HTMLElement) {
  function render() {
    const workspaces = appState.workspaces;
    const activeIdx = appState.activeWorkspace;

    container.innerHTML = "";

    // Header with create button
    const header = document.createElement("div");
    header.className = "sidebar-header";
    header.innerHTML = `
      <span>WORKSPACES</span>
      <button class="sc-header-btn" id="ws-create-btn" title="Create Workspace">+</button>
    `;
    header.querySelector("#ws-create-btn")!.addEventListener("click", (e) => {
      e.stopPropagation();
      showWorkspaceDialog({ mode: "create" });
    });
    container.appendChild(header);

    if (workspaces.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-message";
      empty.textContent = "No workspaces yet";
      container.appendChild(empty);
      return;
    }

    // Group workspaces
    const groups = new Map<string, { idx: number; info: WorkspaceInfo }[]>();
    workspaces.forEach((ws, idx) => {
      const group = ws.info.group || "";
      if (!groups.has(group)) groups.set(group, []);
      groups.get(group)!.push({ idx, info: ws.info });
    });

    const sortedGroups = [...groups.entries()].sort(([a], [b]) => {
      if (a === "" && b !== "") return -1;
      if (a !== "" && b === "") return 1;
      return a.localeCompare(b);
    });

    let badge = 1;
    for (const [groupName, items] of sortedGroups) {
      if (groupName) {
        const groupHeader = document.createElement("div");
        groupHeader.className = "group-header";
        groupHeader.innerHTML = `
          <svg class="group-chevron" viewBox="0 0 16 16">
            <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
          </svg>
          ${escapeHtml(groupName)} (${items.length})
        `;
        container.appendChild(groupHeader);
      }

      for (const { idx, info } of items) {
        const item = document.createElement("div");
        item.className = `workspace-item${idx === activeIdx ? " active" : ""}`;
        item.dataset.idx = String(idx);

        const ws = workspaces[idx];
        const statusClass = getStatusClass(ws.status);

        item.innerHTML = `
          <span class="workspace-badge">${badge <= 9 ? badge : ""}</span>
          ${idx === activeIdx ? '<span class="workspace-active-marker"></span>' : ""}
          <span class="workspace-name">${escapeHtml(info.name)}</span>
          <span class="workspace-actions">
            <button class="ws-action-btn" data-action="info" title="Info">i</button>
            <button class="ws-action-btn" data-action="edit" title="Edit">✎</button>
            <button class="ws-action-btn" data-action="clone" title="Clone">⧉</button>
            <button class="ws-action-btn ws-action-delete" data-action="delete" title="Delete">×</button>
          </span>
          <span class="workspace-status ${statusClass}">${getStatusIcon(ws.status)}</span>
        `;

        // Click to switch workspace
        item.addEventListener("click", async (e) => {
          if ((e.target as HTMLElement).closest(".ws-action-btn")) return;
          try {
            const detail = await ipc.switchWorkspace(idx);
            appState.setActiveWorkspace(idx, detail);
          } catch (err) {
            console.error("Failed to switch workspace:", err);
          }
        });

        // Action buttons
        item.querySelectorAll<HTMLButtonElement>(".ws-action-btn").forEach((btn) => {
          btn.addEventListener("click", (e) => {
            e.stopPropagation();
            const action = btn.dataset.action;
            if (action === "info") {
              showWorkspaceInfo(idx);
            } else if (action === "edit") {
              showWorkspaceDialog({ mode: "edit", editIndex: idx });
            } else if (action === "clone") {
              showWorkspaceDialog({ mode: "clone", cloneFrom: info });
            } else if (action === "delete") {
              showDeleteConfirm(idx, info.name);
            }
          });
        });

        container.appendChild(item);
        badge++;
      }
    }
  }

  appState.on("workspaces-changed", render);
  appState.on("active-workspace-changed", render);
  render();
}

function showDeleteConfirm(idx: number, name: string) {
  document.querySelector(".ws-delete-confirm")?.remove();

  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm";
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p>Delete <strong>${escapeHtml(name)}</strong>?</p>
      <p class="ws-delete-hint">This will remove the worktree and branch.</p>
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-danger ws-confirm-yes">Delete</button>
        <button class="dialog-btn dialog-btn-secondary ws-confirm-no">Cancel</button>
      </div>
    </div>
  `;

  overlay.querySelector(".ws-confirm-yes")!.addEventListener("click", async () => {
    overlay.remove();
    try {
      await ipc.deleteWorkspace(idx);
      appState.removeWorkspace(idx);
      toast(`Deleted "${name}"`, "info");
    } catch (err) {
      toast(`Failed to delete: ${err}`, "error");
    }
  });

  overlay.querySelector(".ws-confirm-no")!.addEventListener("click", () => {
    overlay.remove();
  });

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) overlay.remove();
  });

  document.body.appendChild(overlay);
}

function getStatusClass(status: import("../types").WorkspaceStatus): string {
  if (typeof status === "string") return status.toLowerCase();
  return "error";
}

function getStatusIcon(status: import("../types").WorkspaceStatus): string {
  if (status === "Busy") return "●";
  if (status === "Done") return "✓";
  if (typeof status === "object" && "Error" in status) return "✕";
  return "";
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
