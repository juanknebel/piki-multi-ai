import { appState } from "../state";
import * as ipc from "../ipc";
import { showFileDiff } from "./diff-viewer";
import { FILE_STATUS_LABELS, FILE_STATUS_CSS } from "../types";
import { modCtrl } from "../shortcuts";
import type { ChangedFile, FileStatus } from "../types";

const STAGED_STATUSES: FileStatus[] = [
  "Staged",
  "Added",
  "Renamed",
  "StagedModified",
];
const UNSTAGED_STATUSES: FileStatus[] = [
  "Modified",
  "Deleted",
  "Untracked",
  "StagedModified",
];

let stagedCollapsed = false;
let changesCollapsed = false;
let savedCommitMessage = "";

export function renderSourceControl(container: HTMLElement) {
  function render() {
    const ws = appState.activeWs;
    const files = ws?.changedFiles ?? [];
    const aheadBehind = ws?.aheadBehind;

    const staged = files.filter((f) => STAGED_STATUSES.includes(f.status));
    const unstaged = files.filter((f) => UNSTAGED_STATUSES.includes(f.status));

    // Preserve commit message before clearing the DOM
    const existingTextarea = container.querySelector<HTMLTextAreaElement>(".sc-commit-input");
    if (existingTextarea) {
      savedCommitMessage = existingTextarea.value;
    }

    container.innerHTML = "";

    // Header
    const header = document.createElement("div");
    header.className = "sidebar-header sc-header";
    header.innerHTML = `
      <span>SOURCE CONTROL</span>
      <span class="sc-header-actions">
        ${aheadBehind && aheadBehind[0] > 0 ? `<button class="sc-header-btn" data-action="push" title="Push (↑${aheadBehind[0]})">↑${aheadBehind[0]}</button>` : ""}
        <button class="sc-header-btn" data-action="refresh" title="Refresh">↻</button>
      </span>
    `;
    container.appendChild(header);

    // Wire header actions
    header.querySelectorAll<HTMLButtonElement>(".sc-header-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const action = btn.dataset.action;
        const wsIdx = appState.activeWorkspace;
        try {
          if (action === "push") {
            await ipc.gitPush(wsIdx);
            const status = await ipc.getWorkspaceGitStatus(wsIdx);
            appState.updateFiles(wsIdx, status.files, status.ahead_behind);
          } else if (action === "refresh") {
            const status = await ipc.getWorkspaceGitStatus(wsIdx);
            appState.updateFiles(wsIdx, status.files, status.ahead_behind);
          }
        } catch (err) {
          console.error(`Source control ${action} error:`, err);
        }
      });
    });

    // Commit input area
    const commitArea = document.createElement("div");
    commitArea.className = "sc-commit-area";
    commitArea.innerHTML = `
      <textarea class="sc-commit-input" placeholder="Message (press Ctrl+Enter to commit)" rows="3"></textarea>
      <button class="sc-commit-btn" disabled>
        <span class="sc-commit-icon">✓</span> Commit
      </button>
    `;
    container.appendChild(commitArea);

    const textarea = commitArea.querySelector<HTMLTextAreaElement>(".sc-commit-input")!;
    const commitBtn = commitArea.querySelector<HTMLButtonElement>(".sc-commit-btn")!;

    // Restore saved commit message
    if (savedCommitMessage) {
      textarea.value = savedCommitMessage;
    }

    textarea.addEventListener("input", () => {
      commitBtn.disabled = textarea.value.trim().length === 0 || staged.length === 0;
    });

    textarea.addEventListener("keydown", (e) => {
      if (modCtrl(e) && e.key === "Enter") {
        e.preventDefault();
        if (!commitBtn.disabled) commitBtn.click();
      }
    });

    commitBtn.disabled = textarea.value.trim().length === 0 || staged.length === 0;

    commitBtn.addEventListener("click", async () => {
      const msg = textarea.value.trim();
      if (!msg || staged.length === 0) return;
      commitBtn.disabled = true;
      commitBtn.textContent = "Committing...";
      try {
        const wsIdx = appState.activeWorkspace;
        await ipc.gitCommit(wsIdx, msg);
        textarea.value = "";
        savedCommitMessage = "";
        const status = await ipc.getWorkspaceGitStatus(wsIdx);
        appState.updateFiles(wsIdx, status.files, status.ahead_behind);
      } catch (err) {
        console.error("Commit error:", err);
        commitBtn.textContent = "✓ Commit";
        commitBtn.disabled = false;
      }
    });

    // Staged Changes section
    renderSection(
      container,
      "Staged Changes",
      staged,
      stagedCollapsed,
      (collapsed) => {
        stagedCollapsed = collapsed;
        render();
      },
      "unstage",
      async () => {
        const paths = staged.map(f => f.path);
        await ipc.gitUnstageAll(appState.activeWorkspace);
        appState.pushUndo({ action: "unstage", files: paths });
        await refreshFiles();
      },
    );

    // Changes section
    renderSection(
      container,
      "Changes",
      unstaged,
      changesCollapsed,
      (collapsed) => {
        changesCollapsed = collapsed;
        render();
      },
      "stage",
      async () => {
        const paths = unstaged.map(f => f.path);
        await ipc.gitStageAll(appState.activeWorkspace);
        appState.pushUndo({ action: "stage", files: paths });
        await refreshFiles();
      },
    );

    // Empty state
    if (files.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-message";
      empty.style.padding = "16px 20px";
      empty.textContent = "No changes in this workspace.";
      container.appendChild(empty);
    }
  }

  appState.on("files-changed", render);
  appState.on("active-workspace-changed", render);
  render();
}

function renderSection(
  container: HTMLElement,
  title: string,
  files: ChangedFile[],
  collapsed: boolean,
  onToggle: (collapsed: boolean) => void,
  action: "stage" | "unstage",
  onBulkAction: () => Promise<void>,
) {
  if (files.length === 0) return;

  const selected = new Set<string>();

  const section = document.createElement("div");
  section.className = "sc-section";

  // Section header
  const header = document.createElement("div");
  header.className = "sc-section-header";
  header.innerHTML = `
    <span class="sc-section-toggle">
      <svg class="group-chevron${collapsed ? " collapsed" : ""}" viewBox="0 0 16 16">
        <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
      </svg>
      ${escapeHtml(title)} (${files.length})
    </span>
    <span class="sc-section-actions">
      <button class="sc-section-action sc-selected-action" style="display:none" title="${action === "stage" ? "Stage Selected" : "Unstage Selected"}">
        ${action === "stage" ? "+" : "−"}<span class="sc-selected-count"></span>
      </button>
      <button class="sc-section-action" title="${action === "stage" ? "Stage All" : "Unstage All"}">
        ${action === "stage" ? "++" : "−−"}
      </button>
    </span>
  `;

  header.querySelector(".sc-section-toggle")!.addEventListener("click", () => {
    onToggle(!collapsed);
  });

  // Bulk all action
  header.querySelectorAll<HTMLButtonElement>(".sc-section-action")[1].addEventListener("click", async (e) => {
    e.stopPropagation();
    try {
      await onBulkAction();
    } catch (err) {
      console.error(`Bulk ${action} error:`, err);
    }
  });

  // Bulk selected action
  const selectedBtn = header.querySelector<HTMLButtonElement>(".sc-selected-action")!;
  const selectedCount = selectedBtn.querySelector<HTMLSpanElement>(".sc-selected-count")!;

  function updateSelectedBtn() {
    if (selected.size > 0) {
      selectedBtn.style.display = "";
      selectedCount.textContent = `${selected.size}`;
    } else {
      selectedBtn.style.display = "none";
    }
  }

  selectedBtn.addEventListener("click", async (e) => {
    e.stopPropagation();
    if (selected.size === 0) return;
    const paths = [...selected];
    try {
      const wsIdx = appState.activeWorkspace;
      for (const p of paths) {
        if (action === "stage") await ipc.gitStage(wsIdx, p);
        else await ipc.gitUnstage(wsIdx, p);
      }
      appState.pushUndo({ action, files: paths });
      await refreshFiles();
    } catch (err) {
      console.error(`Bulk selected ${action} error:`, err);
    }
  });

  section.appendChild(header);

  // File list
  if (!collapsed) {
    const list = document.createElement("div");
    list.className = "sc-file-list";

    for (const file of files) {
      const item = document.createElement("div");
      item.className = "file-item";

      const statusLabel = FILE_STATUS_LABELS[file.status];
      const statusCss = FILE_STATUS_CSS[file.status];
      const fileName = file.path.split("/").pop() || file.path;
      const dirPath = file.path.includes("/")
        ? file.path.substring(0, file.path.lastIndexOf("/"))
        : "";

      item.innerHTML = `
        <input type="checkbox" class="file-check" title="Select" />
        <span class="file-status ${statusCss}">${statusLabel}</span>
        <span class="file-path" title="${escapeAttr(file.path)}">
          ${escapeHtml(fileName)}${dirPath ? ` <span style="color:var(--text-muted)">${escapeHtml(dirPath)}</span>` : ""}
        </span>
        <span class="file-actions">
          <button class="file-action-btn" data-action="${action}" title="${action === "stage" ? "Stage" : "Unstage"}">
            ${action === "stage" ? "+" : "−"}
          </button>
        </span>
      `;

      // Checkbox toggle
      const checkbox = item.querySelector<HTMLInputElement>(".file-check")!;
      checkbox.addEventListener("click", (e) => e.stopPropagation());
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) {
          selected.add(file.path);
          item.classList.add("selected");
        } else {
          selected.delete(file.path);
          item.classList.remove("selected");
        }
        updateSelectedBtn();
      });

      // Click file to show diff
      item.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest(".file-action-btn")) return;
        if ((e.target as HTMLElement).closest(".file-check")) return;
        const isStaged = action === "unstage";
        showFileDiff(appState.activeWorkspace, file.path, isStaged);
      });

      // Show actions on hover (handled by CSS visibility)

      // Wire action button
      item.querySelector<HTMLButtonElement>(".file-action-btn")!.addEventListener(
        "click",
        async (e) => {
          e.stopPropagation();
          try {
            const wsIdx = appState.activeWorkspace;
            if (action === "stage") {
              await ipc.gitStage(wsIdx, file.path);
              appState.pushUndo({ action: "stage", files: [file.path] });
            } else {
              await ipc.gitUnstage(wsIdx, file.path);
              appState.pushUndo({ action: "unstage", files: [file.path] });
            }
            await refreshFiles();
          } catch (err) {
            console.error(`${action} error:`, err);
          }
        },
      );

      list.appendChild(item);
    }

    section.appendChild(list);
  }

  container.appendChild(section);
}

async function refreshFiles() {
  const wsIdx = appState.activeWorkspace;
  try {
    const files = await ipc.getChangedFiles(wsIdx);
    appState.updateFiles(wsIdx, files, appState.activeWs?.aheadBehind ?? null);
  } catch (err) {
    console.error("Failed to refresh files:", err);
  }
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}
