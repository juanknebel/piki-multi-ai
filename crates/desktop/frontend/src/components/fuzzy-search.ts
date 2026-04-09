import { appState } from "../state";
import * as ipc from "../ipc";
import { showFileViewer } from "./file-viewer";
import { showMarkdown } from "./markdown-viewer";
import { toast } from "./toast";
import { modCtrl } from "../shortcuts";

let searchEl: HTMLElement | null = null;

export async function openFuzzySearch() {
  if (searchEl) {
    closeFuzzySearch();
    return;
  }

  const wsIdx = appState.activeWorkspace;
  let allFiles: string[];
  try {
    allFiles = await ipc.fuzzyFileList(wsIdx);
  } catch (err) {
    console.error("Failed to list files:", err);
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";

  const palette = document.createElement("div");
  palette.className = "palette";
  palette.innerHTML = `
    <input class="palette-input" type="text" placeholder="Search files... (${allFiles.length} files)" autofocus />
    <div class="palette-results"></div>
  `;

  backdrop.appendChild(palette);
  document.body.appendChild(backdrop);
  searchEl = backdrop;

  const input = palette.querySelector<HTMLInputElement>(".palette-input")!;
  const results = palette.querySelector<HTMLElement>(".palette-results")!;
  let selectedIdx = 0;
  let filtered: string[] = allFiles.slice(0, 50);

  function renderResults() {
    results.innerHTML = "";
    const shown = filtered.slice(0, 50);
    shown.forEach((file, i) => {
      const el = document.createElement("div");
      el.className = `palette-item${i === selectedIdx ? " selected" : ""}`;

      const fileName = file.split("/").pop() || file;
      const dirPath = file.includes("/") ? file.substring(0, file.lastIndexOf("/")) : "";

      el.innerHTML = `
        <span class="palette-label">
          ${highlightMatch(fileName, input.value)}
          ${dirPath ? ` <span style="color:var(--text-muted);font-size:11px">${escapeHtml(dirPath)}</span>` : ""}
        </span>
      `;

      el.addEventListener("click", () => selectFile(file));
      el.addEventListener("mouseenter", () => {
        selectedIdx = i;
        renderResults();
      });
      results.appendChild(el);
    });

    if (filtered.length > 50) {
      const more = document.createElement("div");
      more.className = "palette-empty";
      more.textContent = `${filtered.length - 50} more files...`;
      results.appendChild(more);
    }

    if (filtered.length === 0) {
      results.innerHTML = '<div class="palette-empty">No matching files</div>';
    }
  }

  function filter() {
    const q = input.value.toLowerCase();
    if (!q) {
      filtered = allFiles.slice(0, 200);
    } else {
      filtered = allFiles.filter((f) => f.toLowerCase().includes(q));
    }
    selectedIdx = 0;
    renderResults();
  }

  function selectFile(file: string) {
    closeFuzzySearch();
    if (file.endsWith(".md") || file.endsWith(".markdown")) {
      showMarkdown(file);
    } else {
      showFileViewer(wsIdx, file);
    }
  }

  async function editFile(file: string) {
    closeFuzzySearch();
    try {
      const tabId = await ipc.spawnEditorTab(wsIdx, file);
      appState.addTab(wsIdx, { id: tabId, provider: "Shell", alive: true });
    } catch (err) {
      toast(`Failed to open editor: ${err}`, "error");
    }
  }

  input.addEventListener("input", filter);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, Math.min(filtered.length, 50) - 1);
      renderResults();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      renderResults();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      const shown = filtered.slice(0, 50);
      if (shown[selectedIdx]) selectFile(shown[selectedIdx]);
    } else if (e.key === "e" && modCtrl(e)) {
      e.preventDefault();
      const shown = filtered.slice(0, 50);
      if (shown[selectedIdx]) editFile(shown[selectedIdx]);
    } else if (e.key === "Escape") {
      closeFuzzySearch();
    }
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeFuzzySearch();
  });

  renderResults();
  input.focus();
}

export function closeFuzzySearch() {
  searchEl?.remove();
  searchEl = null;
}

function highlightMatch(text: string, query: string): string {
  if (!query) return escapeHtml(text);
  const lower = text.toLowerCase();
  const idx = lower.indexOf(query.toLowerCase());
  if (idx === -1) return escapeHtml(text);
  const before = text.slice(0, idx);
  const match = text.slice(idx, idx + query.length);
  const after = text.slice(idx + query.length);
  return `${escapeHtml(before)}<strong>${escapeHtml(match)}</strong>${escapeHtml(after)}`;
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
