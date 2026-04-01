import * as ipc from "../ipc";
import { DiffPanel } from "./diff-panel";

let overlayEl: HTMLElement | null = null;

export function showFileDiff(workspaceIdx: number, filePath: string, staged: boolean) {
  closeDiffViewer();
  ipc.getSideBySideDiff(workspaceIdx, filePath, staged).then((diff) => {
    openOverlay(filePath, (container) => {
      const panel = new DiffPanel(container, { mode: "side-by-side" });
      panel.renderSideBySide(diff);
    });
  }).catch((err) => {
    console.error("Failed to load diff:", err);
  });
}

export function showCommitDiff(workspaceIdx: number, sha: string) {
  closeDiffViewer();
  ipc.getCommitSideBySideDiff(workspaceIdx, sha).then((diffs) => {
    const title = `commit ${sha.slice(0, 8)}`;
    openOverlay(title, (container) => {
      const panel = new DiffPanel(container, { mode: "side-by-side" });
      panel.renderMultiFile(diffs);
    });
  }).catch((err) => {
    console.error("Failed to load commit diff:", err);
  });
}

export function showConflictDiff(
  workspaceIdx: number,
  filePath: string,
  onAcceptOurs: (regionIdx: number) => void,
  onAcceptTheirs: (regionIdx: number) => void,
) {
  closeDiffViewer();
  ipc.getConflictDiff(workspaceIdx, filePath).then((conflict) => {
    openOverlay(`CONFLICT: ${filePath}`, (container) => {
      const panel = new DiffPanel(container, {
        mode: "three-way",
        onAcceptOurs,
        onAcceptTheirs,
        onAcceptBoth: (regionIdx) => {
          // Accept both = ours + theirs concatenated
          onAcceptOurs(regionIdx);
        },
      });
      panel.renderConflict(conflict);
    });
  }).catch((err) => {
    console.error("Failed to load conflict diff:", err);
  });
}

function openOverlay(title: string, renderFn: (container: HTMLElement) => void) {
  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.style.paddingTop = "3vh";
  backdrop.style.justifyContent = "center";

  const viewer = document.createElement("div");
  viewer.className = "diff-viewer";

  const header = document.createElement("div");
  header.className = "diff-header";
  header.innerHTML = `
    <span class="diff-title">${escapeHtml(title)}</span>
    <button class="dialog-close diff-close">×</button>
  `;
  viewer.appendChild(header);

  const content = document.createElement("div");
  content.className = "diff-content";
  viewer.appendChild(content);

  backdrop.appendChild(viewer);
  document.body.appendChild(backdrop);
  overlayEl = backdrop;

  renderFn(content);

  header.querySelector(".diff-close")!.addEventListener("click", closeDiffViewer);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeDiffViewer();
  });
  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeDiffViewer();
  });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

export function closeDiffViewer() {
  overlayEl?.remove();
  overlayEl = null;
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
