import { appState } from "../state";
import { reportError } from "./toast";
import * as ipc from "../ipc";
import { settingsStore } from "../settings";
import { showFileDiff } from "./diff-viewer";
import { showMarkdown } from "./markdown-viewer";
import { showWorkspaceDialog } from "./dialogs/workspace-dialog";
import { registerCodeFile } from "./code-editor-panel";
import { revealInFileTree } from "./file-tree";
import { fileGlyph } from "./file-icons";
import { FILE_STATUS_LABELS, FILE_STATUS_CSS } from "../types";
import { modCtrl } from "../shortcuts";
import { isInFlight, onInFlightChange } from "../in-flight";
import { confirmDiscardFile, pullKey, pullWorkspace, pushKey, pushWorkspace } from "./git-actions";
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
let scStagedHeightRestored = false;
/** "Amend last commit" is checked — survives the panel's full re-renders. */
let amendMode = false;
/** What the user had typed before Amend prefilled the box, restored on uncheck. */
let messageBeforeAmend = "";
let inFlightUnsub: (() => void) | null = null;

/** Show the Source Control view with the commit box focused; `amend` also
 *  ticks "Amend last commit" (Git menu / palette entry points). */
export function focusCommitBox(opts: { amend?: boolean } = {}) {
  appState.setActiveView("git");
  setTimeout(() => {
    if (opts.amend && !amendMode) {
      // Toggle through the rendered checkbox so the prefill runs (its
      // change handler also focuses the box).
      const check = document.querySelector<HTMLInputElement>(".sc-amend-check");
      if (check) {
        check.checked = true;
        check.dispatchEvent(new Event("change"));
        return;
      }
      amendMode = true;
    }
    document.querySelector<HTMLTextAreaElement>(".sc-commit-input")?.focus();
  }, 50);
}

async function restoreScStagedHeight() {
  if (scStagedHeightRestored) return;
  scStagedHeightRestored = true;
  await settingsStore.load();
  const pct = settingsStore.get("scStagedHeightPct");
  if (typeof pct === "number") {
    document.documentElement.style.setProperty("--sc-staged-height", `${pct}%`);
  }
}

export function renderSourceControl(container: HTMLElement) {
  void restoreScStagedHeight();
  function render() {
    const ws = appState.activeWs;
    if (ws?.info.workspace_type === "Project") {
      renderProjectView(container, ws.info.path);
      return;
    }
    if (ws && ws.info.origin?.kind === "Local") {
      renderLocalOriginPlaceholder(container);
      return;
    }
    const files = ws?.changedFiles ?? [];
    const aheadBehind = ws?.aheadBehind;
    const wsIdx = appState.activeWorkspace;

    const staged = files.filter((f) => STAGED_STATUSES.includes(f.status));
    const unstaged = files.filter((f) => UNSTAGED_STATUSES.includes(f.status));

    // Preserve commit message (and its caret) before clearing the DOM
    const existingTextarea = container.querySelector<HTMLTextAreaElement>(".sc-commit-input");
    let savedSelection: [number, number] | null = null;
    let hadTextareaFocus = false;
    if (existingTextarea) {
      savedCommitMessage = existingTextarea.value;
      savedSelection = [existingTextarea.selectionStart, existingTextarea.selectionEnd];
      hadTextareaFocus = document.activeElement === existingTextarea;
    }

    container.innerHTML = "";

    // Header
    const header = document.createElement("div");
    header.className = "sidebar-header sc-header";
    header.innerHTML = `
      <span>SOURCE CONTROL</span>
      <span class="sc-header-actions">
        ${syncButton("pull", "↓", aheadBehind?.[1] ?? 0, isInFlight(pullKey(wsIdx)))}
        ${syncButton("push", "↑", aheadBehind?.[0] ?? 0, isInFlight(pushKey(wsIdx)))}
        <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" data-action="refresh" title="Refresh">↻</button>
      </span>
    `;
    container.appendChild(header);

    // Wire header actions. Push/pull go through the shared guarded actions
    // (a second click while one runs is a no-op); the button is disabled
    // and reads `…` until the op settles (re-rendered by onInFlightChange).
    header.querySelectorAll<HTMLButtonElement>(".sc-header-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const action = btn.dataset.action;
        if (action === "push") {
          await pushWorkspace(wsIdx);
        } else if (action === "pull") {
          await pullWorkspace(wsIdx);
        } else if (action === "refresh") {
          try {
            const status = await ipc.getWorkspaceGitStatus(wsIdx);
            appState.updateFiles(wsIdx, status.files, status.ahead_behind);
          } catch (err) {
            reportError("Source control refresh failed", err);
          }
        }
      });
    });

    // Commit input area
    const commitArea = document.createElement("div");
    commitArea.className = "sc-commit-area";
    commitArea.innerHTML = `
      <textarea class="sc-commit-input ui-input" rows="3"></textarea>
      <button data-variant="primary" class="sc-commit-btn ui-btn" disabled>
        <span class="sc-commit-icon">✓</span> <span class="sc-commit-label">Commit</span>
      </button>
      <label class="sc-amend" title="Replace the last commit with the staged changes and this message (git commit --amend)">
        <input type="checkbox" class="sc-amend-check" /> Amend last commit
      </label>
    `;
    container.appendChild(commitArea);

    const textarea = commitArea.querySelector<HTMLTextAreaElement>(".sc-commit-input")!;
    const commitBtn = commitArea.querySelector<HTMLButtonElement>(".sc-commit-btn")!;
    const commitLabel = commitArea.querySelector<HTMLSpanElement>(".sc-commit-label")!;
    const amendCheck = commitArea.querySelector<HTMLInputElement>(".sc-amend-check")!;

    // Restore saved commit message, caret, and focus
    if (savedCommitMessage) {
      textarea.value = savedCommitMessage;
      if (savedSelection) textarea.setSelectionRange(savedSelection[0], savedSelection[1]);
      if (hadTextareaFocus) textarea.focus();
    }

    // Amend: the button is live with staged changes OR a message (an empty
    // message keeps the current one); a plain commit needs both.
    function syncCommitButton() {
      const hasMsg = textarea.value.trim().length > 0;
      commitBtn.disabled = amendMode ? !hasMsg && staged.length === 0 : !hasMsg || staged.length === 0;
      commitLabel.textContent = amendMode ? "Amend" : "Commit";
      textarea.placeholder = amendMode
        ? "Message (empty keeps the current one, Ctrl+Enter to amend)"
        : "Message (press Ctrl+Enter to commit)";
      amendCheck.checked = amendMode;
    }
    syncCommitButton();

    textarea.addEventListener("input", syncCommitButton);

    textarea.addEventListener("keydown", (e) => {
      if (modCtrl(e) && e.key === "Enter") {
        e.preventDefault();
        if (!commitBtn.disabled) commitBtn.click();
      }
    });

    amendCheck.addEventListener("change", async () => {
      amendMode = amendCheck.checked;
      if (amendMode) {
        messageBeforeAmend = textarea.value;
        syncCommitButton();
        try {
          const last = await ipc.gitLastCommitMessage(wsIdx);
          // Only prefill when the box holds nothing the user typed; the panel
          // may have re-rendered meanwhile, so target the live box.
          const box = document.querySelector<HTMLTextAreaElement>(".sc-commit-input") ?? textarea;
          if (amendMode && box.value.trim().length === 0) {
            box.value = last;
            savedCommitMessage = last;
          }
        } catch (err) {
          reportError("Could not read the last commit message", err);
        }
      } else {
        textarea.value = messageBeforeAmend;
        savedCommitMessage = messageBeforeAmend;
      }
      syncCommitButton();
      textarea.focus();
    });

    commitBtn.addEventListener("click", async () => {
      const msg = textarea.value.trim();
      const amending = amendMode;
      if (amending ? !msg && staged.length === 0 : !msg || staged.length === 0) return;
      commitBtn.disabled = true;
      commitLabel.textContent = amending ? "Amending…" : "Committing…";
      try {
        if (amending) await ipc.gitAmend(wsIdx, msg || null);
        else await ipc.gitCommit(wsIdx, msg);
        textarea.value = "";
        savedCommitMessage = "";
        messageBeforeAmend = "";
        amendMode = false;
        syncCommitButton();
        const status = await ipc.getWorkspaceGitStatus(wsIdx);
        appState.updateFiles(wsIdx, status.files, status.ahead_behind);
      } catch (err) {
        reportError(amending ? "Amend failed" : "Commit failed", err);
        syncCommitButton();
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
      empty.className = "ui-empty";
      empty.style.padding = "16px 20px";
      empty.textContent = "No changes in this workspace.";
      container.appendChild(empty);
    }

    // If both sections are visible, install a draggable splitter between them
    const sections = container.querySelectorAll<HTMLElement>(".sc-section");
    if (sections.length === 2) {
      sections[0].classList.add("sc-staged");
      sections[1].classList.add("sc-changes");
      const handle = document.createElement("div");
      handle.className = "sc-section-resize";
      handle.title = "Drag to resize";
      sections[1].before(handle);
      wireSectionSplitter(container, handle, sections[0]);
    }
  }

  appState.on("files-changed", render);
  appState.on("active-workspace-changed", render);
  // Push/pull started anywhere (menu, palette, header) flip the header
  // buttons to their busy state here.
  inFlightUnsub?.();
  inFlightUnsub = onInFlightChange((key) => {
    if (key.startsWith("git-push:") || key.startsWith("git-pull:")) render();
  });
  render();
}

/** `↑N` / `↓N` header button; hidden at 0, disabled + `…` while running. */
function syncButton(action: "push" | "pull", glyph: string, count: number, busy: boolean): string {
  if (count === 0 && !busy) return "";
  const verb = action === "push" ? "Push" : "Pull";
  const title = busy ? `${verb} in progress…` : `${verb} (${glyph}${count})`;
  return `<button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" data-action="${action}" title="${title}"${busy ? " disabled" : ""}>${busy ? `${glyph}…` : `${glyph}${count}`}</button>`;
}

const projectSubdirCache = new Map<number, string[]>();

function renderLocalOriginPlaceholder(container: HTMLElement) {
  container.innerHTML = "";
  const header = document.createElement("div");
  header.className = "sidebar-header sc-header";
  header.innerHTML = `<span>SOURCE CONTROL</span>`;
  container.appendChild(header);

  const empty = document.createElement("div");
  empty.className = "ui-empty";
  empty.style.padding = "16px 20px";
  empty.style.color = "var(--text-muted)";
  empty.style.lineHeight = "1.5";
  empty.textContent =
    "Source control is unavailable for local-folder workspaces. Recreate the workspace from a GitHub URL to enable git operations.";
  container.appendChild(empty);
}

function renderProjectView(container: HTMLElement, projectPath: string) {
  const wsIdx = appState.activeWorkspace;
  container.innerHTML = "";

  // Header
  const header = document.createElement("div");
  header.className = "sidebar-header sc-header";
  header.innerHTML = `
    <span>PROJECT</span>
    <span class="sc-header-actions">
      <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" data-action="refresh" title="Refresh">↻</button>
    </span>
  `;
  container.appendChild(header);

  // List container
  const listWrap = document.createElement("div");
  listWrap.className = "sc-subdir-wrap";
  container.appendChild(listWrap);

  function paint(subdirs: string[]) {
    listWrap.innerHTML = "";
    if (subdirs.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ui-empty";
      empty.style.padding = "16px 20px";
      empty.textContent = "No sub-directories found.";
      listWrap.appendChild(empty);
      return;
    }
    const list = document.createElement("div");
    list.className = "sc-subdir-list";
    for (const name of subdirs) {
      const item = document.createElement("div");
      item.className = "sc-subdir-item";
      item.innerHTML = `<span class="sc-subdir-icon">📁</span><span class="sc-subdir-name"></span>`;
      item.querySelector(".sc-subdir-name")!.textContent = name;
      item.title = `${projectPath}/${name}`;
      item.addEventListener("click", () => {
        showWorkspaceDialog({
          mode: "create",
          prefill: {
            dir: `${projectPath.replace(/\/+$/, "")}/${name}`,
          },
        });
      });
      list.appendChild(item);
    }
    listWrap.appendChild(list);
  }

  async function load() {
    listWrap.innerHTML = '<div class="ui-empty" data-tone="loading">Loading...</div>';
    try {
      const subdirs = await ipc.listProjectSubdirs(wsIdx);
      projectSubdirCache.set(wsIdx, subdirs);
      paint(subdirs);
    } catch (err) {
      listWrap.innerHTML = `<div class="ui-empty" data-tone="error">Failed to load: ${String(err)}</div>`;
    }
  }

  header.querySelector<HTMLButtonElement>('.sc-header-btn[data-action="refresh"]')!
    .addEventListener("click", () => {
      projectSubdirCache.delete(wsIdx);
      void load();
    });

  const cached = projectSubdirCache.get(wsIdx);
  if (cached) {
    paint(cached);
  } else {
    void load();
  }
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
  let lastClickedIdx: number | null = null;

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
      <input type="checkbox" class="sc-section-check" title="Toggle all" />
      <span class="sc-section-title">${escapeHtml(title)} (${files.length})</span>
    </span>
    <span class="sc-section-actions">
      <button data-variant="ghost" data-icon class="sc-section-action ui-btn sc-selected-action" style="display:none" title="${action === "stage" ? "Stage Selected" : "Unstage Selected"}">
        ${action === "stage" ? "+" : "−"}<span class="sc-selected-count"></span>
      </button>
      <button data-variant="ghost" data-icon class="sc-section-action ui-btn" title="${action === "stage" ? "Stage All" : "Unstage All"}">
        ${action === "stage" ? "++" : "−−"}
      </button>
    </span>
  `;

  // Chevron toggles collapse; clicking the section title (not the checkbox) also toggles
  header.querySelector(".sc-section-title")!.addEventListener("click", () => {
    onToggle(!collapsed);
  });
  header.querySelector(".group-chevron")!.addEventListener("click", () => {
    onToggle(!collapsed);
  });

  // Section-level select-all checkbox (tri-state)
  const sectionCheck = header.querySelector<HTMLInputElement>(".sc-section-check")!;
  sectionCheck.addEventListener("click", (e) => e.stopPropagation());
  function updateSectionCheck() {
    if (selected.size === 0) {
      sectionCheck.checked = false;
      sectionCheck.indeterminate = false;
    } else if (selected.size === files.length) {
      sectionCheck.checked = true;
      sectionCheck.indeterminate = false;
    } else {
      sectionCheck.checked = false;
      sectionCheck.indeterminate = true;
    }
  }
  sectionCheck.addEventListener("change", () => {
    const checkAll = selected.size < files.length;
    selected.clear();
    if (checkAll) {
      for (const f of files) selected.add(f.path);
    }
    // Sync per-item checkboxes
    section.querySelectorAll<HTMLInputElement>(".file-check").forEach((cb) => {
      const path = cb.dataset.path!;
      cb.checked = selected.has(path);
      cb.closest(".file-item")?.classList.toggle("selected", selected.has(path));
    });
    updateSelectedBtn();
    updateSectionCheck();
    lastClickedIdx = null;
  });

  // Bulk all action
  header.querySelectorAll<HTMLButtonElement>(".sc-section-action")[1].addEventListener("click", async (e) => {
    e.stopPropagation();
    try {
      await onBulkAction();
    } catch (err) {
      reportError(`Bulk ${action} failed`, err);
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
      reportError(`Bulk ${action} failed`, err);
    }
  });

  section.appendChild(header);

  // File list
  if (!collapsed) {
    const list = document.createElement("div");
    list.className = "sc-file-list";
    list.tabIndex = 0;

    // Ctrl+A inside the list selects all files in the section
    list.addEventListener("keydown", (e) => {
      if (modCtrl(e) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        for (const f of files) selected.add(f.path);
        list.querySelectorAll<HTMLInputElement>(".file-check").forEach((cb) => {
          cb.checked = true;
          cb.closest(".file-item")?.classList.add("selected");
        });
        updateSelectedBtn();
        updateSectionCheck();
      }
    });

    let fileIdx = -1;
    for (const file of files) {
      fileIdx++;
      const item = document.createElement("div");
      item.className = "file-item";

      const statusLabel = FILE_STATUS_LABELS[file.status];
      const statusCss = FILE_STATUS_CSS[file.status];
      const fileName = file.path.split("/").pop() || file.path;
      const dirPath = file.path.includes("/")
        ? file.path.substring(0, file.path.lastIndexOf("/"))
        : "";

      const isMarkdown = /\.(md|markdown)$/i.test(file.path);
      const previewBtn = isMarkdown
        ? `<button data-variant="ghost" data-icon class="file-action-btn ui-btn" data-action="preview" title="Preview rendered markdown">👁</button>`
        : "";
      const isDeleted = file.status === "Deleted";
      const revealBtn = isDeleted
        ? ""
        : `<button data-variant="ghost" data-icon class="file-action-btn ui-btn" data-action="reveal" title="Reveal in Files">⌖</button>`;
      const editBtn = isDeleted
        ? ""
        : `<button data-variant="ghost" data-icon class="file-action-btn ui-btn" data-action="edit" title="Edit in inline editor">✏️</button>`;
      // Working-tree changes can be thrown away (confirmed, irreversible);
      // for an untracked file that means deleting it.
      const discardBtn = action === "stage"
        ? `<button data-variant="ghost" data-icon class="file-action-btn ui-btn file-action-danger" data-action="discard" title="${file.status === "Untracked" ? "Delete file" : "Discard changes"}">⟲</button>`
        : "";

      const itemIdx = fileIdx;
      const fi = fileGlyph(fileName);
      item.innerHTML = `
        <input type="checkbox" class="file-check" data-path="${escapeAttr(file.path)}" data-idx="${itemIdx}" title="Select" />
        <span class="file-status ${statusCss}">${statusLabel}</span>
        <span class="${fi.cls}">${fi.glyph}</span>
        <span class="file-path" title="${escapeAttr(file.path)}">
          ${escapeHtml(fileName)}${dirPath ? ` <span style="color:var(--text-muted)">${escapeHtml(dirPath)}</span>` : ""}
        </span>
        <span class="file-actions">
          ${previewBtn}
          ${revealBtn}
          ${editBtn}
          ${discardBtn}
          <button data-variant="ghost" data-icon class="file-action-btn ui-btn" data-action="${action}" title="${action === "stage" ? "Stage" : "Unstage"}">
            ${action === "stage" ? "+" : "−"}
          </button>
        </span>
      `;

      // Wire preview button (markdown files only)
      if (isMarkdown) {
        item
          .querySelector<HTMLButtonElement>('.file-action-btn[data-action="preview"]')!
          .addEventListener("click", (e) => {
            e.stopPropagation();
            showMarkdown(file.path);
          });
      }

      // Wire reveal button (skip for deleted files)
      if (!isDeleted) {
        item
          .querySelector<HTMLButtonElement>('.file-action-btn[data-action="reveal"]')!
          .addEventListener("click", (e) => {
            e.stopPropagation();
            revealInFileTree(file.path);
          });
      }

      // Wire edit button (skip for deleted files)
      if (!isDeleted) {
        item
          .querySelector<HTMLButtonElement>('.file-action-btn[data-action="edit"]')!
          .addEventListener("click", (e) => {
            e.stopPropagation();
            const wsIdx = appState.activeWorkspace;
            const tabId = crypto.randomUUID();
            registerCodeFile(tabId, file.path, wsIdx);
            appState.addTab(wsIdx, { id: tabId, provider: "CodeEditor", alive: true });
          });
      }

      // Wire discard button (Changes section only)
      item
        .querySelector<HTMLButtonElement>('.file-action-btn[data-action="discard"]')
        ?.addEventListener("click", (e) => {
          e.stopPropagation();
          confirmDiscardFile(appState.activeWorkspace, file, refreshFiles);
        });

      // Checkbox toggle (supports shift+click for range select)
      const checkbox = item.querySelector<HTMLInputElement>(".file-check")!;
      checkbox.addEventListener("click", (e) => {
        e.stopPropagation();
        // After native click the checkbox state has already toggled; capture target state
        const targetChecked = checkbox.checked;
        const me = e as MouseEvent;
        if (me.shiftKey && lastClickedIdx !== null && lastClickedIdx !== itemIdx) {
          const [lo, hi] = lastClickedIdx < itemIdx
            ? [lastClickedIdx, itemIdx]
            : [itemIdx, lastClickedIdx];
          for (let k = lo; k <= hi; k++) {
            const path = files[k].path;
            if (targetChecked) selected.add(path);
            else selected.delete(path);
            const cb = list.querySelector<HTMLInputElement>(`.file-check[data-idx="${k}"]`);
            if (cb) {
              cb.checked = targetChecked;
              cb.closest(".file-item")?.classList.toggle("selected", targetChecked);
            }
          }
        } else {
          if (targetChecked) {
            selected.add(file.path);
            item.classList.add("selected");
          } else {
            selected.delete(file.path);
            item.classList.remove("selected");
          }
        }
        lastClickedIdx = itemIdx;
        updateSelectedBtn();
        updateSectionCheck();
      });

      // Click file to show diff
      item.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest(".file-action-btn")) return;
        if ((e.target as HTMLElement).closest(".file-check")) return;
        const isStaged = action === "unstage";
        showFileDiff(appState.activeWorkspace, file.path, isStaged);
      });

      // Show actions on hover (handled by CSS visibility)

      // Wire action button (stage/unstage)
      item.querySelector<HTMLButtonElement>(`.file-action-btn[data-action="${action}"]`)!.addEventListener(
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
            reportError(`${action} failed`, err);
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
    reportError("Failed to refresh files", err);
  }
}

function wireSectionSplitter(
  container: HTMLElement,
  handle: HTMLElement,
  topSection: HTMLElement,
) {
  handle.addEventListener("mousedown", (e) => {
    const startY = e.clientY;
    const startTopHeight = topSection.offsetHeight;
    const containerHeight = container.clientHeight;
    handle.classList.add("dragging");
    document.body.style.cursor = "ns-resize";
    document.body.style.userSelect = "none";
    e.preventDefault();

    function onMove(ev: MouseEvent) {
      const delta = ev.clientY - startY;
      const minPx = 60;
      const maxPx = Math.max(minPx, containerHeight - 120);
      const newPx = Math.max(minPx, Math.min(maxPx, startTopHeight + delta));
      const pct = (newPx / containerHeight) * 100;
      document.documentElement.style.setProperty("--sc-staged-height", `${pct.toFixed(2)}%`);
    }

    function onUp() {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      handle.classList.remove("dragging");
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      const pct = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--sc-staged-height"));
      if (!isNaN(pct)) settingsStore.patch("scStagedHeightPct", pct);
    }

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  });
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}
