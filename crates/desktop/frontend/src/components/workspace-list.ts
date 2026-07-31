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

/** Label for a row. Structure (families, the PR-review group, collapse
 *  state) is decided by the backend; only the text is decided here.
 *  Worktrees are named by their branch, a clone by its repo folder. */
function rowLabel(info: WorkspaceInfo, branch: string | null): string {
  if (info.workspace_type === "Worktree") return branch ?? info.name;
  const folder =
    info.source_repo.replace(/\/+$/, "").split("/").pop() ||
    info.source_repo_display ||
    info.name;
  return branch ? `${folder} (${branch})` : folder;
}

export function renderWorkspaceList(container: HTMLElement) {
  const collapsedGroups = new Set<string>();
  // Grouped rows straight from `core::workspace::sidebar_rows`, refreshed
  // whenever the workspace list or the collapse state changes. Cached so
  // render() stays synchronous for its many event-driven callers.
  let rows: ipc.SidebarRow[] = [];

  ipc
    .getCollapsedGroups()
    .then((groups) => {
      for (const g of groups) collapsedGroups.add(g);
      return refreshRows();
    })
    .catch(() => {});

  async function refreshRows() {
    try {
      rows = await ipc.sidebarRows();
    } catch (err) {
      console.error("Failed to load sidebar rows:", err);
      rows = [];
    }
    render();
  }

  function persistCollapsed() {
    ipc.setCollapsedGroups([...collapsedGroups]).catch(() => {});
    // The backend resolves collapse state, so re-fetch rather than
    // recompute here.
    void refreshRows();
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

    for (const row of rows) {
      if (row.type === "prReviewHeader") {
        const header = document.createElement("div");
        header.className = "group-header";
        header.innerHTML = `
          <svg class="group-chevron${row.collapsed ? " collapsed" : ""}" viewBox="0 0 16 16">
            <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
          </svg>
          <span class="group-label">PR Review</span>
        `;
        const key = row.family_key;
        header.addEventListener("click", () => {
          if (collapsedGroups.has(key)) collapsedGroups.delete(key);
          else collapsedGroups.add(key);
          persistCollapsed();
        });
        container.appendChild(header);
        continue;
      }

      const idx = row.index;
      // Rows are built from the same list this renders, but the fetch is
      // async — skip anything the list no longer has.
      if (idx >= workspaces.length) continue;
      const info = workspaces[idx].info;
      const item = document.createElement("div");
      item.className = `workspace-item${idx === activeIdx ? " active" : ""}${row.kind === "child" ? " grouped" : ""}`;
      item.dataset.idx = String(idx);

      const ws = workspaces[idx];
      const statusClass = getStatusClass(ws.status);

      const attentionDot = ws.needsAttention
        ? '<span class="workspace-attention" title="Needs attention">●</span>'
        : "";

      const chevron = row.kind === "parent"
        ? `<svg class="group-chevron${row.collapsed ? " collapsed" : ""}" viewBox="0 0 16 16">
             <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
           </svg>`
        : "";

      item.innerHTML = `
        ${chevron}
        ${idx === activeIdx ? '<span class="workspace-active-marker"></span>' : ""}
        <span class="workspace-name">${escapeHtml(rowLabel(info, ws.branch))}</span>
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
      if (row.kind === "parent" && row.family_key) {
        const key = row.family_key;
        item.querySelector(".group-chevron")!.addEventListener("click", (e) => {
          e.stopPropagation();
          if (collapsedGroups.has(key)) {
            collapsedGroups.delete(key);
          } else {
            collapsedGroups.add(key);
          }
          persistCollapsed();
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

  appState.on("workspaces-changed", () => void refreshRows());
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
