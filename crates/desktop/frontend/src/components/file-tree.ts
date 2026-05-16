import { appState } from "../state";
import * as ipc from "../ipc";
import type { DirEntry, EntryKind } from "../types";
import { registerCodeFile } from "./code-editor-panel";
import { registerMarkdownFile } from "./markdown-editor-panel";

type NodeState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "loaded"; entries: DirEntry[] }
  | { status: "error"; message: string };

interface Row {
  rel: string;
  name: string;
  kind: EntryKind;
  depth: number;
}

const MD_RE = /\.(md|markdown)$/i;

function joinRel(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

function parentRel(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i < 0 ? "" : rel.slice(0, i);
}

const FOLDER_SVG = `<svg class="ft-icon" viewBox="0 0 16 16"><path d="M1.5 3.5h4l1.5 2h7.5v8h-13z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
const FILE_SVG = `<svg class="ft-icon" viewBox="0 0 16 16"><path d="M3.5 1.5h6l3 3v10h-9z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/><path d="M9.5 1.5v3h3" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
const CHEVRON_SVG = `<svg class="ft-chevron" viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>`;

export function renderFileTree(container: HTMLElement) {
  const nodes = new Map<string, NodeState>();
  const expanded = new Set<string>();
  let selected: string | null = null;
  let showHidden = false;
  let rootPath: string | null = null;
  let wsIdx = -1;

  async function fetchChildren(rel: string) {
    nodes.set(rel, { status: "loading" });
    const reqWs = wsIdx;
    try {
      const entries = await ipc.fsReadDir(reqWs, rel, showHidden);
      // Drop the result if the workspace changed mid-flight.
      if (reqWs !== wsIdx) return;
      nodes.set(rel, { status: "loaded", entries });
    } catch (e) {
      if (reqWs !== wsIdx) return;
      nodes.set(rel, { status: "error", message: String(e) });
    }
    render();
  }

  function resetToActiveWorkspace() {
    const ws = appState.activeWs;
    wsIdx = appState.activeWorkspace;
    rootPath = ws?.info.path ?? null;
    nodes.clear();
    expanded.clear();
    selected = null;
    if (rootPath) void fetchChildren("");
    else render();
  }

  /** Re-list every currently-loaded directory (branch switch / commit). */
  function refreshLoaded() {
    if (!rootPath) return;
    for (const [rel, st] of [...nodes.entries()]) {
      if (st.status === "loaded") void fetchChildren(rel);
    }
  }

  function visibleRows(): Row[] {
    const out: Row[] = [];
    const walk = (relDir: string, depth: number) => {
      const node = nodes.get(relDir);
      if (!node || node.status !== "loaded") return;
      for (const e of node.entries) {
        const rel = joinRel(relDir, e.name);
        out.push({ rel, name: e.name, kind: e.kind, depth });
        if (e.kind === "Dir" && expanded.has(rel)) walk(rel, depth + 1);
      }
    };
    walk("", 0);
    return out;
  }

  function openFile(rel: string) {
    if (wsIdx < 0) return;
    const tabId = crypto.randomUUID();
    if (MD_RE.test(rel)) {
      registerMarkdownFile(tabId, rel);
      appState.addTab(wsIdx, { id: tabId, provider: "Markdown", alive: true });
    } else {
      registerCodeFile(tabId, rel, wsIdx);
      appState.addTab(wsIdx, { id: tabId, provider: "CodeEditor", alive: true });
    }
  }

  function toggleDir(rel: string) {
    if (expanded.has(rel)) {
      expanded.delete(rel);
    } else {
      expanded.add(rel);
      const st = nodes.get(rel);
      if (!st || st.status === "idle" || st.status === "error") {
        void fetchChildren(rel);
      }
    }
    render();
  }

  function onRowActivate(rel: string, isDir: boolean) {
    selected = rel;
    if (isDir) toggleDir(rel);
    else {
      openFile(rel);
      render();
    }
  }

  function handleKey(e: KeyboardEvent) {
    const rows = visibleRows();
    if (rows.length === 0) return;
    const idx = selected ? rows.findIndex((r) => r.rel === selected) : -1;
    const move = (next: number) => {
      const clamped = Math.max(0, Math.min(rows.length - 1, next));
      selected = rows[clamped].rel;
      render();
      const el = container.querySelector<HTMLElement>(
        `.ft-row[data-rel="${CSS.escape(selected)}"]`,
      );
      el?.scrollIntoView({ block: "nearest" });
    };
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(idx < 0 ? 0 : idx + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(idx < 0 ? rows.length - 1 : idx - 1);
        break;
      case "ArrowRight": {
        if (idx < 0) return;
        e.preventDefault();
        const row = rows[idx];
        if (row.kind === "Dir") {
          if (!expanded.has(row.rel)) toggleDir(row.rel);
          else move(idx + 1);
        }
        break;
      }
      case "ArrowLeft": {
        if (idx < 0) return;
        e.preventDefault();
        const row = rows[idx];
        if (row.kind === "Dir" && expanded.has(row.rel)) {
          toggleDir(row.rel);
        } else if (row.depth > 0) {
          selected = parentRel(row.rel);
          render();
        }
        break;
      }
      case "Enter":
        if (idx < 0) return;
        e.preventDefault();
        onRowActivate(rows[idx].rel, rows[idx].kind === "Dir");
        break;
    }
  }

  function render() {
    const list = container.querySelector<HTMLElement>(".ft-list");
    const prevScroll = list?.scrollTop ?? 0;
    container.innerHTML = "";

    const folderName = rootPath
      ? rootPath.split("/").filter(Boolean).pop() ?? rootPath
      : "";

    const header = document.createElement("div");
    header.className = "sidebar-header";
    header.innerHTML = `
      <span title="${escAttr(rootPath ?? "")}">${esc(folderName.toUpperCase() || "FILES")}</span>
      <span class="ft-header-actions">
        <button class="sc-header-btn ft-toggle-hidden${showHidden ? " active" : ""}" title="Show hidden files">.*</button>
        <button class="sc-header-btn ft-refresh" title="Refresh">⟳</button>
      </span>
    `;
    header
      .querySelector(".ft-toggle-hidden")!
      .addEventListener("click", (e) => {
        e.stopPropagation();
        showHidden = !showHidden;
        refreshLoaded();
        render();
      });
    header.querySelector(".ft-refresh")!.addEventListener("click", (e) => {
      e.stopPropagation();
      refreshLoaded();
    });
    container.appendChild(header);

    const listEl = document.createElement("div");
    listEl.className = "ft-list";
    listEl.tabIndex = 0;
    listEl.addEventListener("keydown", handleKey);

    if (!rootPath) {
      const empty = document.createElement("div");
      empty.className = "empty-message";
      empty.textContent = "No active workspace";
      listEl.appendChild(empty);
      container.appendChild(listEl);
      return;
    }

    const root = nodes.get("");
    if (root?.status === "loading" && visibleRows().length === 0) {
      const m = document.createElement("div");
      m.className = "empty-message";
      m.textContent = "Loading…";
      listEl.appendChild(m);
    } else if (root?.status === "error") {
      const m = document.createElement("div");
      m.className = "empty-message";
      m.textContent = `Failed to read directory: ${root.message}`;
      listEl.appendChild(m);
    } else if (root?.status === "loaded" && root.entries.length === 0) {
      const m = document.createElement("div");
      m.className = "empty-message";
      m.textContent = "Empty directory";
      listEl.appendChild(m);
    }

    for (const row of visibleRows()) {
      const isDir = row.kind === "Dir";
      const isOpen = isDir && expanded.has(row.rel);
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = `ft-row${row.rel === selected ? " selected" : ""}`;
      btn.dataset.rel = row.rel;
      btn.style.paddingLeft = `${6 + row.depth * 12}px`;
      btn.innerHTML = `
        <span class="ft-twisty${isOpen ? " open" : ""}">${isDir ? CHEVRON_SVG : ""}</span>
        ${isDir ? FOLDER_SVG : FILE_SVG}
        <span class="ft-name">${esc(row.name)}</span>
      `;
      btn.addEventListener("click", () => onRowActivate(row.rel, isDir));
      listEl.appendChild(btn);

      // Inline loading / error indicator for an expanded directory.
      if (isOpen) {
        const st = nodes.get(row.rel);
        if (st?.status === "loading" || st?.status === "error") {
          const note = document.createElement("div");
          note.className = "ft-note";
          note.style.paddingLeft = `${6 + (row.depth + 1) * 12}px`;
          note.textContent =
            st.status === "loading" ? "Loading…" : `Error: ${st.message}`;
          listEl.appendChild(note);
        }
      }
    }

    container.appendChild(listEl);
    listEl.scrollTop = prevScroll;
  }

  resetToActiveWorkspace();

  appState.on("active-workspace-changed", resetToActiveWorkspace);
  appState.on("files-changed", refreshLoaded);

  // Live filesystem updates from the core FileWatcher: refetch only the
  // parent directories of changed paths that we currently have loaded.
  void ipc.onFileChanged((evt) => {
    if (evt.workspace_idx !== wsIdx) return;
    const dirs = new Set<string>();
    for (const p of evt.paths) dirs.add(parentRel(p));
    let touched = false;
    for (const d of dirs) {
      const st = nodes.get(d);
      if (st && st.status === "loaded") {
        touched = true;
        void fetchChildren(d);
      }
    }
    if (touched) render();
  });
}

function esc(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escAttr(text: string): string {
  return text.replace(/"/g, "&quot;");
}
