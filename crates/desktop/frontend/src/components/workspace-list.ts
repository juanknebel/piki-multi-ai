import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { createDropdown } from "./dropdown";
import {
  showCreateWorktreeDialog,
  showWorkspaceDialog,
  showWorkspaceInfo,
} from "./dialogs/workspace-dialog";
import { showAgentManager } from "./dialogs/agent-dialog";
import type { WorkspaceInfo } from "../types";

/** One visual sidebar row: always a real workspace (`idx`), decorated with
 *  family metadata. Mirrors the TUI's `App::sidebar_items()` — workspaces
 *  sharing `source_repo` form a family; the one member whose `workspace_type`
 *  isn't `"Worktree"` is the parent (repo folder name + its own branch),
 *  its `Worktree` siblings are children (branch name only, indented). A
 *  family with no parent loaded, or no family at all, renders flat. */
interface SidebarRow {
  idx: number;
  label: string;
  isParent: boolean;
  isChild: boolean;
  /** Present on parent rows: the family identifier + current collapse state. */
  familyKey?: string;
  collapsed?: boolean;
}

function familyKey(info: WorkspaceInfo): string {
  return info.source_repo;
}

function folderLabel(info: WorkspaceInfo): string {
  const folder =
    info.source_repo.replace(/\/+$/, "").split("/").pop() ||
    info.source_repo_display ||
    info.name;
  return info.branch ? `${folder} (${info.branch})` : folder;
}

function computeSidebarRows(
  workspaces: readonly { info: WorkspaceInfo }[],
  collapsedGroups: Set<string>,
): SidebarRow[] {
  const rows: SidebarRow[] = [];
  const consumed = new Array(workspaces.length).fill(false);

  for (let i = 0; i < workspaces.length; i++) {
    if (consumed[i]) continue;
    const sourceRepo = workspaces[i].info.source_repo;
    const siblings = workspaces
      .map((ws, j) => ({ ws, j }))
      .filter(({ ws, j }) => !consumed[j] && ws.info.source_repo === sourceRepo)
      .map(({ j }) => j);

    if (siblings.length <= 1) {
      rows.push({ idx: i, label: workspaces[i].info.name, isParent: false, isChild: false });
      consumed[i] = true;
      continue;
    }

    const parentPos = siblings.find((j) => workspaces[j].info.workspace_type !== "Worktree");

    if (parentPos !== undefined) {
      const key = familyKey(workspaces[parentPos].info);
      const collapsed = collapsedGroups.has(key);
      rows.push({
        idx: parentPos,
        label: folderLabel(workspaces[parentPos].info),
        isParent: true,
        isChild: false,
        familyKey: key,
        collapsed,
      });
      for (const j of siblings) {
        consumed[j] = true;
        if (j !== parentPos && !collapsed) {
          rows.push({ idx: j, label: workspaces[j].info.branch, isParent: false, isChild: true });
        }
      }
    } else {
      for (const j of siblings) {
        consumed[j] = true;
        rows.push({ idx: j, label: workspaces[j].info.branch, isParent: false, isChild: false });
      }
    }
  }

  return rows;
}

export function renderWorkspaceList(container: HTMLElement) {
  const collapsedGroups = new Set<string>();

  // Load persisted collapse state once, then re-render.
  ipc
    .getCollapsedGroups()
    .then((groups) => {
      for (const g of groups) collapsedGroups.add(g);
      render();
    })
    .catch(() => {});

  function persistCollapsed() {
    ipc.setCollapsedGroups([...collapsedGroups]).catch(() => {});
  }

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

    const rows = computeSidebarRows(workspaces, collapsedGroups);

    for (const row of rows) {
      const { idx, info } = { idx: row.idx, info: workspaces[row.idx].info };
      const item = document.createElement("div");
      item.className = `workspace-item${idx === activeIdx ? " active" : ""}${row.isChild ? " grouped" : ""}`;
      item.dataset.idx = String(idx);

      const ws = workspaces[idx];
      const statusClass = getStatusClass(ws.status);

      const attentionDot = ws.needsAttention
        ? '<span class="workspace-attention" title="Needs attention">●</span>'
        : "";

      const chevron = row.isParent
        ? `<svg class="group-chevron${row.collapsed ? " collapsed" : ""}" viewBox="0 0 16 16">
             <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
           </svg>`
        : "";

      item.innerHTML = `
        ${chevron}
        ${idx === activeIdx ? '<span class="workspace-active-marker"></span>' : ""}
        <span class="workspace-name">${escapeHtml(row.label)}</span>
        ${attentionDot}
        <span class="workspace-actions">
          <button class="ws-action-btn" data-action="agents" title="Manage Agents">⚙</button>
          <button class="ws-action-btn" data-action="info" title="Info">i</button>
          <button class="ws-action-btn" data-action="edit" title="Edit">✎</button>
          ${info.origin?.kind === "GitHub" ? `<button class="ws-action-btn" data-action="create-worktree" title="Create Worktree">⧉</button>` : ""}
          <button class="ws-action-btn ws-action-delete" data-action="delete" title="Delete">×</button>
        </span>
        <span class="workspace-status ${statusClass}">${getStatusIcon(ws.status)}</span>
      `;

      // Click the chevron to toggle collapse without switching workspace.
      if (row.isParent && row.familyKey) {
        const key = row.familyKey;
        item.querySelector(".group-chevron")!.addEventListener("click", (e) => {
          e.stopPropagation();
          if (collapsedGroups.has(key)) {
            collapsedGroups.delete(key);
          } else {
            collapsedGroups.add(key);
          }
          persistCollapsed();
          render();
        });
      }

      // Click to switch workspace
      item.addEventListener("click", async (e) => {
        if ((e.target as HTMLElement).closest(".ws-action-btn")) return;
        if ((e.target as HTMLElement).closest(".group-chevron")) return;
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
          if (action === "agents") {
            showAgentManager();
          } else if (action === "info") {
            showWorkspaceInfo(idx);
          } else if (action === "edit") {
            showWorkspaceDialog({ mode: "edit", editIndex: idx });
          } else if (action === "create-worktree") {
            showCreateWorktreeDialog(info);
          } else if (action === "delete") {
            showDeleteConfirm(idx, info.name);
          }
        });
      });

      container.appendChild(item);
    }
  }

  appState.on("workspaces-changed", render);
  appState.on("active-workspace-changed", render);
  appState.on("workspace-attention-changed", render);
  render();
}

async function showDeleteConfirm(idx: number, name: string) {
  document.querySelector(".ws-delete-confirm")?.remove();

  const ws = appState.workspaces[idx];
  const cardId = ws?.info.dispatch_card_id;
  const boardPath = ws?.info.dispatch_source_kanban;

  // If workspace was created via dispatch, load kanban columns for card move options
  let colDropdown: ReturnType<typeof createDropdown> | null = null;
  if (cardId && boardPath) {
    try {
      const board = await ipc.kanbanLoadBoardByPath(boardPath);
      const COL_LABELS: Record<string, string> = {
        todo: "To Do",
        in_progress: "In Progress",
        in_review: "In Review",
        done: "Done",
      };
      colDropdown = createDropdown(
        [
          { value: "", label: "(Leave where it is)" },
          ...board.columns.map((col) => ({ value: col.id, label: COL_LABELS[col.id] ?? col.id })),
        ],
        "",
      );
    } catch {
      // Board not available, skip card move
    }
  }

  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm";
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p>Delete <strong>${escapeHtml(name)}</strong>?</p>
      <p class="ws-delete-hint">This will remove the worktree and branch.</p>
      ${colDropdown ? '<div class="ws-delete-card-move"><label class="dialog-label">Move task card to:</label><span id="ws-delete-col-slot"></span></div>' : ""}
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-danger ws-confirm-yes">Delete</button>
        <button class="dialog-btn dialog-btn-secondary ws-confirm-no">Cancel</button>
      </div>
    </div>
  `;

  // Mount dropdown if present
  if (colDropdown) {
    const slot = overlay.querySelector("#ws-delete-col-slot");
    if (slot) slot.replaceWith(colDropdown.container);
  }

  overlay.querySelector(".ws-confirm-yes")!.addEventListener("click", async () => {
    // Move kanban card if user selected a column
    if (cardId && boardPath && colDropdown) {
      const targetCol = colDropdown.value;
      if (targetCol) {
        try {
          await ipc.kanbanMoveCardByPath(boardPath, cardId, targetCol);
        } catch {
          // Non-critical
        }
      }
    }

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
