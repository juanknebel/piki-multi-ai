import { appState } from "../state";
import type { PaneId } from "../pane-tree";
import { fuzzyScorePath, mruBump, mruRank } from "./fuzzy";
import * as ipc from "../ipc";
import { showFileViewer } from "./file-viewer";
import { showMarkdown } from "./markdown-viewer";
import { toast, reportError } from "./toast";
import { modAlt, modCtrl, formatShortcut } from "../shortcuts";
import { fileGlyph } from "./file-icons";
import { openFileInEditor, openFileInExternalEditor } from "./open-content";
import { isMarkdownPath, looksBinary } from "../file-kind";

const SHOWN_LIMIT = 50;

let searchEl: HTMLElement | null = null;
/** Bumped on every open so a slow IPC from a previous palette is ignored. */
let generation = 0;
/** Last index seen per workspace path — rendered instantly on the next open
 *  while the backend (which memoises too) confirms or refreshes it. */
const lastIndex = new Map<string, string[]>();

/** `paneId`: open the picked file INTO that blank pane (the blank-pane
 *  chooser's "Open file…") instead of a new top-level tab. */
export function openFuzzySearch(opts: { paneId?: PaneId } = {}) {
  if (searchEl) {
    closeFuzzySearch();
    return;
  }

  const ws = appState.activeWs;
  if (!ws) {
    toast("Open a workspace to find files", "info");
    return;
  }
  const wsIdx = appState.activeWorkspace;
  const wsPath = ws.info.path;
  const target = { paneId: opts.paneId };
  const gen = ++generation;

  // Render first, index second: the input must be usable immediately and
  // whatever is typed before the list lands is applied when it does.
  let allFiles: string[] = lastIndex.get(wsPath) ?? [];
  let indexing = true;
  let truncated = false;

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";

  const palette = document.createElement("div");
  palette.className = "palette ui-surface";
  palette.innerHTML = `
    <input class="palette-input" type="text" placeholder="${opts.paneId ? "Open a file in this pane…" : "Search files…"}" autofocus />
    <div class="palette-results"></div>
    <div class="palette-footer">
      <span class="palette-footer-hint"><span class="palette-key">Enter</span> Edit</span>
      <span class="palette-footer-hint"><span class="palette-key">${formatShortcut("Alt+Enter")}</span> View</span>
      <span class="palette-footer-hint"><span class="palette-key">${formatShortcut("Ctrl+E")}</span> $EDITOR</span>
      <span class="palette-footer-status"></span>
    </div>
  `;

  backdrop.appendChild(palette);
  document.body.appendChild(backdrop);
  searchEl = backdrop;

  const input = palette.querySelector<HTMLInputElement>(".palette-input")!;
  const results = palette.querySelector<HTMLElement>(".palette-results")!;
  const status = palette.querySelector<HTMLElement>(".palette-footer-status")!;
  let selectedIdx = 0;
  const byRecency = (a: string, b: string) => mruRank("files", a) - mruRank("files", b);
  let filtered: string[] = [...allFiles].sort(byRecency);

  function renderStatus() {
    status.classList.toggle("indexing", indexing);
    if (indexing) {
      status.textContent = allFiles.length ? `Indexing… (${allFiles.length} files so far)` : "Indexing…";
      return;
    }
    status.textContent = truncated
      ? `First ${allFiles.length} files only — index capped`
      : `${allFiles.length} files`;
  }

  function renderResults() {
    results.innerHTML = "";
    const shown = filtered.slice(0, SHOWN_LIMIT);
    shown.forEach((file, i) => {
      const el = document.createElement("div");
      el.className = `palette-item${i === selectedIdx ? " selected" : ""}`;

      const fileName = file.split("/").pop() || file;
      const dirPath = file.includes("/") ? file.substring(0, file.lastIndexOf("/")) : "";
      const fi = fileGlyph(fileName);

      el.innerHTML = `
        <span class="${fi.cls}">${fi.glyph}</span>
        <span class="palette-label">
          ${highlightMatch(fileName, input.value)}
          ${dirPath ? ` <span class="palette-sub">${escapeHtml(dirPath)}</span>` : ""}
        </span>
      `;

      el.addEventListener("click", () => openInEditor(file));
      el.addEventListener("mouseenter", () => {
        if (selectedIdx === i) return;
        selectedIdx = i;
        updateSelection();
      });
      results.appendChild(el);
    });

    if (filtered.length > SHOWN_LIMIT) {
      const more = document.createElement("div");
      more.className = "ui-empty";
      more.textContent = `${filtered.length - SHOWN_LIMIT} more files…`;
      results.appendChild(more);
    }

    if (filtered.length === 0) {
      results.innerHTML = `<div class="ui-empty">${indexing && !allFiles.length ? "Indexing…" : "No matching files"}</div>`;
    }
  }

  function updateSelection() {
    results.querySelectorAll<HTMLElement>(".palette-item").forEach((el, i) => {
      el.classList.toggle("selected", i === selectedIdx);
    });
  }

  function filter() {
    const q = input.value.trim();
    if (!q) {
      filtered = [...allFiles].sort(byRecency);
    } else {
      filtered = allFiles
        .map((f) => ({ f, score: fuzzyScorePath(q, f) }))
        .filter((e): e is { f: string; score: number } => e.score !== null)
        .sort((a, b) => b.score - a.score || byRecency(a.f, b.f))
        .map((e) => e.f);
    }
    selectedIdx = 0;
    renderResults();
  }

  /** Enter / click: an editor tab, exactly what the file tree opens on
   *  click. Files that are not text stay in the read-only viewer. */
  function openInEditor(file: string) {
    if (looksBinary(file)) {
      openInViewer(file);
      return;
    }
    mruBump("files", file);
    closeFuzzySearch();
    openFileInEditor(wsIdx, file, target);
  }

  /** Alt+Enter: the read-only overlay (rendered markdown for .md). */
  function openInViewer(file: string) {
    mruBump("files", file);
    closeFuzzySearch();
    if (isMarkdownPath(file)) {
      showMarkdown(file);
    } else {
      showFileViewer(wsIdx, file);
    }
  }

  /** Ctrl+E: a terminal tab running $EDITOR on the file. */
  async function openInExternalEditor(file: string) {
    mruBump("files", file);
    closeFuzzySearch();
    await openFileInExternalEditor(wsIdx, file, target);
  }

  function selected(): string | undefined {
    return filtered.slice(0, SHOWN_LIMIT)[selectedIdx];
  }

  input.addEventListener("input", filter);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, Math.min(filtered.length, SHOWN_LIMIT) - 1);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      const file = selected();
      if (!file) return;
      if (modAlt(e)) openInViewer(file);
      else openInEditor(file);
    } else if (e.key === "e" && modCtrl(e)) {
      e.preventDefault();
      const file = selected();
      if (file) void openInExternalEditor(file);
    } else if (e.key === "Escape") {
      closeFuzzySearch();
    }
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeFuzzySearch();
  });

  renderResults();
  renderStatus();
  input.focus();

  ipc
    .fuzzyFileList(wsIdx)
    .then((index) => {
      lastIndex.set(wsPath, index.files);
      if (gen !== generation || searchEl !== backdrop) return;
      allFiles = index.files;
      truncated = index.truncated;
      indexing = false;
      filter();
      renderStatus();
    })
    .catch((err) => {
      if (gen !== generation || searchEl !== backdrop) return;
      indexing = false;
      status.classList.remove("indexing");
      status.textContent = "Could not index files";
      if (!allFiles.length) results.innerHTML = '<div class="ui-empty">Could not index files</div>';
      reportError("Failed to list files", err);
    });
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
