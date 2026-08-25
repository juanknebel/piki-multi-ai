// The ONE "delete workspace" confirmation — sidebar row menu and command
// palette both call `confirmDeleteWorkspace(idx)`. It tells the truth per
// workspace type (a Simple workspace has no worktree or branch to remove),
// counts the uncommitted changes, and lists the agents still running there
// (from `appState.agentRows`, backend-sourced so never-visited workspaces
// count too). Dispatch-created workspaces also offer to move their card.

import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast, reportError } from "../toast";
import { showConfirm, escapeHtml } from "../confirm";
import { createDropdown } from "../dropdown";
import { cliAgentStatusView, type WorkspaceType } from "../../types";

/** What deletion removes, by workspace type (mirrors
 *  `WorkspaceManager::remove`): worktree + branch, a project root's
 *  registration, or just the entry for a plain directory. */
function removalHint(type: WorkspaceType): string {
  switch (type) {
    case "Worktree":
      return "This removes the worktree and its branch.";
    case "Project":
      return "This removes the project workspace from the list — the directory stays on disk.";
    case "Simple":
    default:
      return "This removes the workspace from the list — the directory stays on disk.";
  }
}

export async function confirmDeleteWorkspace(idx: number) {
  const ws = appState.workspaces[idx];
  if (!ws) return;
  const info = ws.info;
  const name = info.name;
  const cardId = info.dispatch_card_id;
  const boardPath = info.dispatch_source_kanban;

  // Dispatch-created workspace: offer to move its kanban card.
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
    } catch (err) {
      // The card move is optional; deletion still proceeds without it.
      console.error("Kanban board unavailable for card move:", err);
    }
  }

  const changes = ws.changedFiles.length;
  const agents = appState.agentRows.filter((r) => r.workspace_idx === idx && r.alive);
  const warnings: string[] = [];
  if (changes > 0) {
    warnings.push(
      `<p class="ws-delete-warn">${changes} uncommitted change${changes === 1 ? "" : "s"} will be lost.</p>`,
    );
  }
  if (agents.length > 0) {
    const lines = agents
      .map((r) => {
        const st = r.status ? ` — ${cliAgentStatusView(r.status, r.attention).label}` : "";
        return `<li>${escapeHtml(r.label)}${escapeHtml(st)}</li>`;
      })
      .join("");
    warnings.push(
      `<p class="ws-delete-warn">${agents.length} running agent${agents.length === 1 ? "" : "s"} will be terminated:</p><ul class="ws-delete-list">${lines}</ul>`,
    );
  }

  const { overlay } = showConfirm({
    bodyHtml: `
      <p>Delete <strong>${escapeHtml(name)}</strong>?</p>
      <p class="ws-delete-hint">${removalHint(info.workspace_type)}</p>
      ${warnings.join("")}
      ${colDropdown ? '<div class="ws-delete-card-move"><label class="dialog-label">Move task card to:</label><span id="ws-delete-col-slot"></span></div>' : ""}
    `,
    actions: [
      {
        label: "Delete",
        kind: "danger",
        isDefault: true,
        onSelect: async () => {
          if (cardId && boardPath && colDropdown?.value) {
            try {
              await ipc.kanbanMoveCardByPath(boardPath, cardId, colDropdown.value);
            } catch (err) {
              reportError("Move task card failed", err);
            }
          }
          try {
            await ipc.deleteWorkspace(idx);
            appState.removeWorkspace(idx);
            toast(`Deleted "${name}"`, "info");
          } catch (err) {
            reportError(`Delete "${name}" failed`, err);
          }
        },
      },
      { label: "Cancel", kind: "secondary" },
    ],
  });

  if (colDropdown) {
    overlay.querySelector("#ws-delete-col-slot")?.replaceWith(colDropdown.container);
  }
}
