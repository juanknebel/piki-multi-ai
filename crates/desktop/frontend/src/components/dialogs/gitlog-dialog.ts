import { appState } from "../../state";
import * as ipc from "../../ipc";
import { showCommitDiff } from "../diff-viewer";

export async function showGitLog() {
  document.querySelector(".dialog-backdrop")?.remove();

  const wsIdx = appState.activeWorkspace;
  let entries: ipc.GitLogEntry[];
  try {
    entries = await ipc.getGitLog(wsIdx);
  } catch (err) {
    console.error("Failed to load git log:", err);
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.style.paddingTop = "5vh";

  let selectedIdx = 0;

  function render() {
    const existing = backdrop.querySelector(".dialog");
    if (existing) existing.remove();

    const dialog = document.createElement("div");
    dialog.className = "dialog";
    dialog.style.maxWidth = "700px";
    dialog.style.maxHeight = "80vh";
    dialog.innerHTML = `
      <div class="dialog-header">
        <span class="dialog-title">Git Log</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="gitlog-content"></div>
    `;

    const content = dialog.querySelector<HTMLElement>(".gitlog-content")!;
    content.style.cssText = "overflow-y:auto;flex:1;font-family:'JetBrainsMono NF Mono',monospace;font-size:12px;";

    entries.forEach((entry, i) => {
      const el = document.createElement("div");
      el.className = `gitlog-entry${i === selectedIdx ? " selected" : ""}`;
      el.style.cssText = `
        padding: 2px 12px;
        cursor: ${entry.sha ? "pointer" : "default"};
        white-space: pre;
        color: var(--text-primary);
        ${i === selectedIdx ? "background: var(--sidebar-item-focus);" : ""}
      `;
      el.textContent = entry.line;

      if (entry.sha) {
        el.addEventListener("click", () => {
          close();
          showCommitDiff(wsIdx, entry.sha!);
        });
        el.addEventListener("mouseenter", () => {
          el.style.background = "var(--sidebar-item-hover)";
        });
        el.addEventListener("mouseleave", () => {
          el.style.background = i === selectedIdx ? "var(--sidebar-item-focus)" : "";
        });
      }

      content.appendChild(el);
    });

    dialog.querySelector(".dialog-close")!.addEventListener("click", close);
    backdrop.appendChild(dialog);
  }

  const close = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });

  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      close();
    } else if (e.key === "ArrowDown" || e.key === "j") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, entries.length - 1);
      render();
    } else if (e.key === "ArrowUp" || e.key === "k") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      render();
    } else if (e.key === "Enter") {
      const entry = entries[selectedIdx];
      if (entry?.sha) {
        close();
        showCommitDiff(wsIdx, entry.sha);
      }
    }
  });

  document.body.appendChild(backdrop);
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
  render();
}
