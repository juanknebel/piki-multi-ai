import { appState } from "../state";
import * as ipc from "../ipc";
import { showFileViewer } from "./file-viewer";
import { toast } from "./toast";
import { modCtrl } from "../shortcuts";
import type { SearchMatch } from "../ipc";

let searchEl: HTMLElement | null = null;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

export function openProjectSearch() {
  if (searchEl) {
    closeProjectSearch();
    return;
  }

  const wsIdx = appState.activeWorkspace;
  if (wsIdx < 0 || !appState.activeWs) return;

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";

  const palette = document.createElement("div");
  palette.className = "palette";
  palette.innerHTML = `
    <input class="palette-input" type="text" placeholder="Search in project (grep)..." autofocus />
    <div class="palette-results"></div>
  `;

  backdrop.appendChild(palette);
  document.body.appendChild(backdrop);
  searchEl = backdrop;

  const input = palette.querySelector<HTMLInputElement>(".palette-input")!;
  const results = palette.querySelector<HTMLElement>(".palette-results")!;
  let selectedIdx = 0;
  let matches: SearchMatch[] = [];

  function renderResults() {
    results.innerHTML = "";

    if (matches.length === 0 && input.value.trim()) {
      results.innerHTML = '<div class="palette-empty">No matches found</div>';
      return;
    }

    if (!input.value.trim()) {
      results.innerHTML = '<div class="palette-empty">Type to search file contents...</div>';
      return;
    }

    matches.forEach((m, i) => {
      const el = document.createElement("div");
      el.className = `palette-item${i === selectedIdx ? " selected" : ""}`;

      const fileName = m.path.split("/").pop() || m.path;
      const dirPath = m.path.includes("/") ? m.path.substring(0, m.path.lastIndexOf("/")) : "";

      el.innerHTML = `
        <span class="palette-category" style="min-width:32px;text-align:right">${m.line_num}</span>
        <span class="palette-label" style="flex:1;overflow:hidden">
          <span style="color:var(--text-bright)">${escapeHtml(fileName)}</span>
          ${dirPath ? `<span style="color:var(--text-muted);font-size:10px;margin-left:6px">${escapeHtml(dirPath)}</span>` : ""}
          <div style="color:var(--text-secondary);font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-top:2px;font-family:'JetBrainsMono NF Mono',monospace">${highlightMatch(m.text.trim(), input.value)}</div>
        </span>
      `;

      el.addEventListener("click", () => selectMatch(m));
      el.addEventListener("mouseenter", () => {
        if (selectedIdx === i) return;
        selectedIdx = i;
        updateSelection();
      });
      results.appendChild(el);
    });

    if (matches.length === 100) {
      const more = document.createElement("div");
      more.className = "palette-empty";
      more.textContent = "Showing first 100 results...";
      results.appendChild(more);
    }
  }

  function updateSelection() {
    results.querySelectorAll<HTMLElement>(".palette-item").forEach((el, i) => {
      el.classList.toggle("selected", i === selectedIdx);
    });
  }

  async function doSearch() {
    const q = input.value.trim();
    if (!q) {
      matches = [];
      selectedIdx = 0;
      renderResults();
      return;
    }

    try {
      matches = await ipc.projectSearch(wsIdx, q);
    } catch {
      matches = [];
    }
    selectedIdx = 0;
    renderResults();
  }

  function selectMatch(m: SearchMatch) {
    closeProjectSearch();
    showFileViewer(wsIdx, m.path);
  }

  async function editMatch(m: SearchMatch) {
    closeProjectSearch();
    try {
      const tabId = await ipc.spawnEditorTab(wsIdx, m.path);
      appState.addTab(wsIdx, { id: tabId, provider: "Shell", alive: true });
    } catch (err) {
      toast(`Failed to open editor: ${err}`, "error");
    }
  }

  input.addEventListener("input", () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(doSearch, 300);
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, matches.length - 1);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (matches[selectedIdx]) selectMatch(matches[selectedIdx]);
    } else if (e.key === "e" && modCtrl(e)) {
      e.preventDefault();
      if (matches[selectedIdx]) editMatch(matches[selectedIdx]);
    } else if (e.key === "Escape") {
      closeProjectSearch();
    }
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeProjectSearch();
  });

  renderResults();
  input.focus();
}

export function closeProjectSearch() {
  if (debounceTimer) clearTimeout(debounceTimer);
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
  return `${escapeHtml(before)}<strong style="color:var(--accent-primary)">${escapeHtml(match)}</strong>${escapeHtml(after)}`;
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
