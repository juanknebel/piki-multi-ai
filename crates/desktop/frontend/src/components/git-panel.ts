import { appState } from "../state";
import { reportError } from "./toast";
import * as ipc from "../ipc";
import { FILE_STATUS_LABELS, FILE_STATUS_CSS } from "../types";
import type { ChangedFile } from "../types";

export function renderGitPanel(container: HTMLElement) {
  let selectedIdx = -1;

  function render() {
    const ws = appState.activeWs;
    const files = ws?.changedFiles ?? [];
    const aheadBehind = ws?.aheadBehind;

    container.innerHTML = "";

    // Header
    const header = document.createElement("div");
    header.className = "sidebar-header";
    let headerText = "CHANGES";
    if (aheadBehind) {
      const [ahead, behind] = aheadBehind;
      const parts: string[] = [];
      if (ahead > 0) parts.push(`↑${ahead}`);
      if (behind > 0) parts.push(`↓${behind}`);
      if (parts.length > 0) headerText += ` ${parts.join(" ")}`;
    }
    if (files.length > 0) {
      headerText += ` (${files.length})`;
    }
    header.textContent = headerText;
    container.appendChild(header);

    if (files.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ui-empty";
      empty.textContent = "No changes";
      container.appendChild(empty);
      return;
    }

    // File items
    files.forEach((file, idx) => {
      const item = document.createElement("div");
      item.className = `file-item${idx === selectedIdx ? " selected" : ""}`;
      item.dataset.idx = String(idx);

      const statusLabel = FILE_STATUS_LABELS[file.status];
      const statusCss = FILE_STATUS_CSS[file.status];
      const fileName = file.path.split("/").pop() || file.path;
      const dirPath = file.path.includes("/")
        ? file.path.substring(0, file.path.lastIndexOf("/"))
        : "";

      item.innerHTML = `
        <span class="file-status ${statusCss}">${statusLabel}</span>
        <span class="file-path" title="${escapeAttr(file.path)}">
          ${escapeHtml(fileName)}${dirPath ? ` <span style="color: var(--text-muted)">${escapeHtml(dirPath)}</span>` : ""}
        </span>
        <span class="file-actions">
          ${createActionButton(file, "stage")}
          ${createActionButton(file, "unstage")}
        </span>
      `;

      item.addEventListener("click", () => {
        selectedIdx = idx;
        // Move the highlight in place — a full re-render would drop scroll
        // position and hover state just to switch a class.
        container.querySelectorAll(".file-item.selected").forEach((el) =>
          el.classList.remove("selected"),
        );
        item.classList.add("selected");
      });

      // Wire up action buttons
      item.querySelectorAll<HTMLButtonElement>(".file-action-btn").forEach((btn) => {
        btn.addEventListener("click", async (e) => {
          e.stopPropagation();
          const action = btn.dataset.action;
          const wsIdx = appState.activeWorkspace;
          try {
            if (action === "stage") {
              await ipc.gitStage(wsIdx, file.path);
            } else if (action === "unstage") {
              await ipc.gitUnstage(wsIdx, file.path);
            }
            const files = await ipc.getChangedFiles(wsIdx);
            appState.updateFiles(wsIdx, files, appState.activeWs?.aheadBehind ?? null);
          } catch (err) {
            reportError(`Failed to ${action}`, err);
          }
        });
      });

      container.appendChild(item);
    });
  }

  appState.on("files-changed", render);
  appState.on("active-workspace-changed", render);
  render();
}

function createActionButton(file: ChangedFile, action: string): string {
  const isStaged =
    file.status === "Staged" || file.status === "StagedModified" || file.status === "Added";

  if (action === "stage" && !isStaged) {
    return `<button data-variant="ghost" data-icon class="file-action-btn ui-btn" data-action="stage" title="Stage">+</button>`;
  }
  if (action === "unstage" && isStaged) {
    return `<button data-variant="ghost" data-icon class="file-action-btn ui-btn" data-action="unstage" title="Unstage">−</button>`;
  }
  return "";
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(text: string): string {
  return text.replace(/"/g, "&quot;").replace(/&/g, "&amp;");
}
