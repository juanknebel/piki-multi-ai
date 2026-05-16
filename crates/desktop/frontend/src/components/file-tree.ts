import { appState } from "../state";
import * as ipc from "../ipc";
import type { DirEntry, EntryKind } from "../types";
import { registerCodeFile } from "./code-editor-panel";
import { registerMarkdownFile } from "./markdown-editor-panel";
import { showMarkdown } from "./markdown-viewer";
import { toast } from "./toast";

type NodeState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "loaded"; entries: DirEntry[] }
  | { status: "error"; message: string };

interface EntryRow {
  type: "entry";
  rel: string;
  name: string;
  kind: EntryKind;
  depth: number;
}
interface InputRow {
  type: "input";
  parentRel: string;
  createKind: "file" | "dir";
  depth: number;
}
type RenderRow = EntryRow | InputRow;

const MD_RE = /\.(md|markdown)$/i;

function joinRel(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}
function parentRel(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i < 0 ? "" : rel.slice(0, i);
}
function baseName(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i < 0 ? rel : rel.slice(i + 1);
}

const FOLDER_SVG = `<svg class="ft-icon" viewBox="0 0 16 16"><path d="M1.5 3.5h4l1.5 2h7.5v8h-13z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
const FILE_SVG = `<svg class="ft-icon" viewBox="0 0 16 16"><path d="M3.5 1.5h6l3 3v10h-9z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/><path d="M9.5 1.5v3h3" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
const CHEVRON_SVG = `<svg class="ft-chevron" viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>`;

interface CtxItem {
  label?: string;
  action?: () => void;
  danger?: boolean;
  separator?: boolean;
}

function openContextMenu(x: number, y: number, items: CtxItem[]) {
  document.querySelector(".ft-ctx")?.remove();
  const menu = document.createElement("div");
  menu.className = "ft-ctx";
  for (const it of items) {
    if (it.separator) {
      const s = document.createElement("div");
      s.className = "ft-ctx-sep";
      menu.appendChild(s);
      continue;
    }
    const b = document.createElement("button");
    b.type = "button";
    b.className = `ft-ctx-item${it.danger ? " danger" : ""}`;
    b.textContent = it.label ?? "";
    b.addEventListener("click", () => {
      close();
      it.action?.();
    });
    menu.appendChild(b);
  }
  const close = () => {
    menu.remove();
    document.removeEventListener("mousedown", onDown, true);
    document.removeEventListener("keydown", onKey, true);
    window.removeEventListener("blur", close);
  };
  const onDown = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) close();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };
  document.body.appendChild(menu);
  // Clamp to viewport.
  const r = menu.getBoundingClientRect();
  const left = Math.min(x, window.innerWidth - r.width - 4);
  const top = Math.min(y, window.innerHeight - r.height - 4);
  menu.style.left = `${Math.max(4, left)}px`;
  menu.style.top = `${Math.max(4, top)}px`;
  document.addEventListener("mousedown", onDown, true);
  document.addEventListener("keydown", onKey, true);
  window.addEventListener("blur", close);
}

export function renderFileTree(container: HTMLElement) {
  const nodes = new Map<string, NodeState>();
  const expanded = new Set<string>();
  let selected: string | null = null;
  let showHidden = false;
  let rootPath: string | null = null;
  let wsIdx = -1;
  let pendingCreate: { parentRel: string; kind: "file" | "dir" } | null = null;
  let renaming: string | null = null;

  async function fetchChildren(rel: string) {
    nodes.set(rel, { status: "loading" });
    const reqWs = wsIdx;
    try {
      const entries = await ipc.fsReadDir(reqWs, rel, showHidden);
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
    pendingCreate = null;
    renaming = null;
    if (rootPath) void fetchChildren("");
    else render();
  }

  function refreshLoaded() {
    if (!rootPath) return;
    for (const [rel, st] of [...nodes.entries()]) {
      if (st.status === "loaded") void fetchChildren(rel);
    }
  }

  /** Drop cached node + expanded state for `rel` and everything under it. */
  function forgetSubtree(rel: string) {
    const prefix = `${rel}/`;
    for (const k of [...nodes.keys()]) {
      if (k === rel || k.startsWith(prefix)) nodes.delete(k);
    }
    for (const k of [...expanded]) {
      if (k === rel || k.startsWith(prefix)) expanded.delete(k);
    }
  }

  function visibleRows(): RenderRow[] {
    const out: RenderRow[] = [];
    if (pendingCreate?.parentRel === "") {
      out.push({ type: "input", parentRel: "", createKind: pendingCreate.kind, depth: 0 });
    }
    const walk = (relDir: string, depth: number) => {
      const node = nodes.get(relDir);
      if (!node || node.status !== "loaded") return;
      for (const e of node.entries) {
        const rel = joinRel(relDir, e.name);
        out.push({ type: "entry", rel, name: e.name, kind: e.kind, depth });
        if (e.kind === "Dir" && expanded.has(rel)) {
          if (pendingCreate?.parentRel === rel) {
            out.push({
              type: "input",
              parentRel: rel,
              createKind: pendingCreate.kind,
              depth: depth + 1,
            });
          }
          walk(rel, depth + 1);
        }
      }
    };
    walk("", 0);
    return out;
  }

  function openFile(rel: string, forceCode = false) {
    if (wsIdx < 0) return;
    const tabId = crypto.randomUUID();
    if (!forceCode && MD_RE.test(rel)) {
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

  // ── mutations ──────────────────────────────────────

  function beginCreate(parentRelPath: string, kind: "file" | "dir") {
    renaming = null;
    pendingCreate = { parentRel: parentRelPath, kind };
    if (parentRelPath) {
      expanded.add(parentRelPath);
      const st = nodes.get(parentRelPath);
      if (!st || st.status !== "loaded") void fetchChildren(parentRelPath);
    }
    render();
  }

  async function commitCreate(name: string) {
    const pc = pendingCreate;
    pendingCreate = null;
    if (!pc) return render();
    const trimmed = name.trim();
    if (!trimmed) return render();
    const rel = joinRel(pc.parentRel, trimmed);
    try {
      if (pc.kind === "dir") await ipc.fsCreateDir(wsIdx, rel);
      else await ipc.fsCreateFile(wsIdx, rel);
      selected = rel;
      await fetchChildren(pc.parentRel);
      if (pc.kind === "file") openFile(rel);
    } catch (e) {
      toast(`Create failed: ${e}`, "error");
      render();
    }
  }

  function beginRename(rel: string) {
    pendingCreate = null;
    renaming = rel;
    render();
  }

  async function commitRename(newName: string) {
    const from = renaming;
    renaming = null;
    if (!from) return render();
    const trimmed = newName.trim();
    if (!trimmed || trimmed === baseName(from)) return render();
    const to = joinRel(parentRel(from), trimmed);
    try {
      await ipc.fsRename(wsIdx, from, to);
      forgetSubtree(from);
      if (selected === from) selected = to;
      await fetchChildren(parentRel(from));
    } catch (e) {
      toast(`Rename failed: ${e}`, "error");
      render();
    }
  }

  function confirmDelete(rel: string) {
    document.querySelector(".ws-delete-confirm")?.remove();
    const overlay = document.createElement("div");
    overlay.className = "ws-delete-confirm";
    overlay.innerHTML = `
      <div class="ws-delete-dialog">
        <p>Delete <strong>${esc(baseName(rel))}</strong>?</p>
        <p class="ws-delete-hint">This permanently removes it from disk.</p>
        <div class="ws-delete-buttons">
          <button class="dialog-btn dialog-btn-danger ws-confirm-yes">Delete</button>
          <button class="dialog-btn dialog-btn-secondary ws-confirm-no">Cancel</button>
        </div>
      </div>`;
    const closeOverlay = () => overlay.remove();
    overlay.querySelector(".ws-confirm-yes")!.addEventListener("click", async () => {
      closeOverlay();
      try {
        await ipc.fsDelete(wsIdx, rel);
        forgetSubtree(rel);
        if (selected === rel) selected = null;
        await fetchChildren(parentRel(rel));
      } catch (e) {
        toast(`Delete failed: ${e}`, "error");
      }
    });
    overlay.querySelector(".ws-confirm-no")!.addEventListener("click", closeOverlay);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) closeOverlay();
    });
    document.body.appendChild(overlay);
  }

  function copyAbs(rel: string) {
    const abs = rootPath ? `${rootPath.replace(/\/$/, "")}/${rel}` : rel;
    ipc.clipboardCopy(abs).catch(() => {});
  }

  function showRowMenu(ev: MouseEvent, row: EntryRow) {
    ev.preventDefault();
    selected = row.rel;
    render();
    const isDir = row.kind === "Dir";
    const isMd = !isDir && MD_RE.test(row.rel);
    const createTarget = isDir ? row.rel : parentRel(row.rel);
    const items: CtxItem[] = [];
    if (!isDir) {
      if (isMd) {
        items.push({ label: "Open (Rendered)", action: () => openFile(row.rel) });
        items.push({ label: "Preview", action: () => void showMarkdown(row.rel) });
        items.push({ label: "Open as Text", action: () => openFile(row.rel, true) });
      } else {
        items.push({ label: "Open", action: () => openFile(row.rel) });
      }
      items.push({ separator: true });
    }
    items.push({ label: "New File", action: () => beginCreate(createTarget, "file") });
    items.push({ label: "New Folder", action: () => beginCreate(createTarget, "dir") });
    items.push({ separator: true });
    items.push({ label: "Rename", action: () => beginRename(row.rel) });
    items.push({ label: "Delete", danger: true, action: () => confirmDelete(row.rel) });
    items.push({ separator: true });
    items.push({ label: "Copy Path", action: () => copyAbs(row.rel) });
    items.push({ label: "Copy Relative Path", action: () => ipc.clipboardCopy(row.rel).catch(() => {}) });
    openContextMenu(ev.clientX, ev.clientY, items);
  }

  function showRootMenu(ev: MouseEvent) {
    ev.preventDefault();
    openContextMenu(ev.clientX, ev.clientY, [
      { label: "New File", action: () => beginCreate("", "file") },
      { label: "New Folder", action: () => beginCreate("", "dir") },
      { separator: true },
      { label: "Refresh", action: () => refreshLoaded() },
    ]);
  }

  // ── keyboard ───────────────────────────────────────

  function handleKey(e: KeyboardEvent) {
    if (pendingCreate || renaming) return;
    if ((e.target as HTMLElement)?.tagName === "INPUT") return;
    const rows = visibleRows().filter((r): r is EntryRow => r.type === "entry");
    if (rows.length === 0) return;
    const idx = selected ? rows.findIndex((r) => r.rel === selected) : -1;
    const move = (next: number) => {
      const clamped = Math.max(0, Math.min(rows.length - 1, next));
      selected = rows[clamped].rel;
      render();
      container
        .querySelector<HTMLElement>(`.ft-row[data-rel="${CSS.escape(selected)}"]`)
        ?.scrollIntoView({ block: "nearest" });
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
        if (row.kind === "Dir" && expanded.has(row.rel)) toggleDir(row.rel);
        else if (row.depth > 0) {
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
      case "F2":
        if (idx < 0) return;
        e.preventDefault();
        beginRename(rows[idx].rel);
        break;
      case "Delete":
        if (idx < 0) return;
        e.preventDefault();
        confirmDelete(rows[idx].rel);
        break;
    }
  }

  // ── render ─────────────────────────────────────────

  function makeInputRow(row: InputRow): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "ft-row ft-input-row";
    wrap.style.paddingLeft = `${6 + row.depth * 12}px`;
    wrap.innerHTML = `<span class="ft-twisty"></span>${
      row.createKind === "dir" ? FOLDER_SVG : FILE_SVG
    }`;
    const input = document.createElement("input");
    input.className = "ft-input";
    input.spellcheck = false;
    input.placeholder = row.createKind === "dir" ? "folder name" : "file name";
    let done = false;
    const finish = (commit: boolean) => {
      if (done) return;
      done = true;
      if (commit) void commitCreate(input.value);
      else {
        pendingCreate = null;
        render();
      }
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        finish(true);
      } else if (e.key === "Escape") {
        e.preventDefault();
        finish(false);
      }
    });
    input.addEventListener("blur", () => finish(input.value.trim().length > 0));
    wrap.appendChild(input);
    queueMicrotask(() => input.focus());
    return wrap;
  }

  function makeRenameRow(row: EntryRow): HTMLElement {
    const isDir = row.kind === "Dir";
    const wrap = document.createElement("div");
    wrap.className = "ft-row ft-input-row";
    wrap.style.paddingLeft = `${6 + row.depth * 12}px`;
    wrap.innerHTML = `<span class="ft-twisty">${isDir ? CHEVRON_SVG : ""}</span>${
      isDir ? FOLDER_SVG : FILE_SVG
    }`;
    const input = document.createElement("input");
    input.className = "ft-input";
    input.spellcheck = false;
    input.value = row.name;
    let done = false;
    const finish = (commit: boolean) => {
      if (done) return;
      done = true;
      if (commit) void commitRename(input.value);
      else {
        renaming = null;
        render();
      }
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        finish(true);
      } else if (e.key === "Escape") {
        e.preventDefault();
        finish(false);
      }
    });
    input.addEventListener("blur", () => finish(true));
    wrap.appendChild(input);
    queueMicrotask(() => {
      input.focus();
      const dot = row.name.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : row.name.length);
    });
    return wrap;
  }

  function render() {
    const listPrev = container.querySelector<HTMLElement>(".ft-list");
    const prevScroll = listPrev?.scrollTop ?? 0;
    container.innerHTML = "";

    const folderName = rootPath
      ? rootPath.split("/").filter(Boolean).pop() ?? rootPath
      : "";

    const header = document.createElement("div");
    header.className = "sidebar-header";
    header.innerHTML = `
      <span title="${escAttr(rootPath ?? "")}">${esc(folderName.toUpperCase() || "FILES")}</span>
      <span class="ft-header-actions">
        <button class="sc-header-btn ft-new-file" title="New File">+</button>
        <button class="sc-header-btn ft-toggle-hidden${showHidden ? " active" : ""}" title="Show hidden files">.*</button>
        <button class="sc-header-btn ft-refresh" title="Refresh">⟳</button>
      </span>`;
    header.querySelector(".ft-new-file")!.addEventListener("click", (e) => {
      e.stopPropagation();
      beginCreate("", "file");
    });
    header.querySelector(".ft-toggle-hidden")!.addEventListener("click", (e) => {
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
    listEl.addEventListener("contextmenu", (e) => {
      if (e.target === listEl) showRootMenu(e);
    });

    if (!rootPath) {
      const empty = document.createElement("div");
      empty.className = "empty-message";
      empty.textContent = "No active workspace";
      listEl.appendChild(empty);
      container.appendChild(listEl);
      return;
    }

    const root = nodes.get("");
    const rows = visibleRows();
    if (root?.status === "loading" && rows.length === 0) {
      listEl.appendChild(msg("Loading…"));
    } else if (root?.status === "error") {
      listEl.appendChild(msg(`Failed to read directory: ${root.message}`));
    } else if (root?.status === "loaded" && rows.length === 0) {
      listEl.appendChild(msg("Empty directory"));
    }

    for (const row of rows) {
      if (row.type === "input") {
        listEl.appendChild(makeInputRow(row));
        continue;
      }
      if (renaming === row.rel) {
        listEl.appendChild(makeRenameRow(row));
        continue;
      }
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
        <span class="ft-name">${esc(row.name)}</span>`;
      btn.addEventListener("click", () => onRowActivate(row.rel, isDir));
      if (!isDir) {
        btn.addEventListener("dblclick", () => beginRename(row.rel));
      }
      btn.addEventListener("contextmenu", (e) => showRowMenu(e, row));
      listEl.appendChild(btn);

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

function msg(text: string): HTMLElement {
  const m = document.createElement("div");
  m.className = "empty-message";
  m.textContent = text;
  return m;
}
function esc(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
function escAttr(text: string): string {
  return text.replace(/"/g, "&quot;");
}
