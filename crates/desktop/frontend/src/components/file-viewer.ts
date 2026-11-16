import { EditorView, basicSetup } from "codemirror";
import { showConfirm } from "./confirm";
import { EditorState, EditorSelection, Compartment, Extension } from "@codemirror/state";
import { search, openSearchPanel, setSearchQuery, SearchQuery } from "@codemirror/search";
import { vim } from "@replit/codemirror-vim";
import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { modCtrl, formatShortcut } from "../shortcuts";
import { openFileInEditor } from "./open-content";
import { buildCmTheme } from "../cm-theme";
import { themeEngine } from "../theme";
import { attachDialogResize } from "./dialog-resize";
import {
  clampLine,
  clampPosition,
  positionKey,
  recallFilePosition,
  rememberFilePosition,
} from "../file-position";

const readOnlyComp = new Compartment();

/** Where a caller wants the viewer to land. A project-search hit knows both
 *  (`project-search.ts` passes the match's line and the query it was found
 *  by); the file explorer knows neither and gets the remembered reading
 *  position instead. */
export interface FileViewerTarget {
  /** 1-based line to put the cursor on, centred in the viewport. */
  line?: number;
  /** Find query to pre-fill the search panel with. */
  query?: string;
}

/** The open viewer's close path. A second open runs it instead of just
 *  dropping the node, so the first file's reading position is saved and its
 *  editor destroyed rather than orphaned. */
let closeOpenViewer: (() => void) | null = null;

export async function showFileViewer(
  workspaceIdx: number,
  path: string,
  target: FileViewerTarget = {},
) {
  closeOpenViewer?.();

  let content: string;
  try {
    content = await ipc.readFileContent(workspaceIdx, path);
  } catch (err) {
    toast(`Failed to read file: ${err}`, "error");
    return;
  }

  const fileName = path.split("/").pop() || path;
  const posKey = positionKey(workspaceIdx, path);

  const backdrop = document.createElement("div");
  backdrop.className = "file-viewer-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "file-viewer-dialog ui-surface";

  dialog.innerHTML = `
    <div class="ui-header">
      <span class="ui-header-title" title="${escapeAttr(path)}">${escapeHtml(fileName)}<span class="file-viewer-path">${escapeHtml(path)}</span></span>
      <div class="file-viewer-actions"></div>
    </div>
    <div class="file-viewer-body"></div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);
  attachDialogResize(dialog, "file-viewer");

  const body = dialog.querySelector<HTMLElement>(".file-viewer-body")!;
  const actionsDiv = dialog.querySelector<HTMLElement>(".file-viewer-actions")!;

  // Create CodeMirror editor in read-only mode
  const langExt = await getLanguageExtension(path);

  const editorView = new EditorView({
    state: EditorState.create({
      doc: content,
      extensions: [
        vim(),
        basicSetup,
        // basicSetup carries searchKeymap already; this adds the panel state
        // `setSearchQuery` writes into, and pins the find bar to the top.
        search({ top: true }),
        ...(langExt ? [langExt] : []),
        buildCmTheme(
          (k) => themeEngine.getEffectiveColor(k),
          themeEngine.getActivePreset().isDark,
        ),
        readOnlyComp.of(EditorState.readOnly.of(true)),
      ],
    }),
    parent: body,
  });

  let editing = false;

  const close = () => {
    rememberFilePosition(posKey, {
      top: editorView.scrollDOM.scrollTop,
      anchor: editorView.state.selection.main.anchor,
      head: editorView.state.selection.main.head,
    });
    closeOpenViewer = null;
    editorView.destroy();
    backdrop.remove();
  };
  closeOpenViewer = close;

  // ── Where to land ──────────────────────────
  // An explicit line from a search hit wins over the remembered position.

  if (target.line !== undefined) {
    const doc = editorView.state.doc;
    const at = doc.line(clampLine(target.line, doc.lines)).from;
    editorView.dispatch({
      selection: EditorSelection.cursor(at),
      effects: EditorView.scrollIntoView(at, { y: "center" }),
    });
  } else {
    const back = clampPosition(recallFilePosition(posKey), editorView.state.doc.length);
    if (back) {
      editorView.dispatch({ selection: EditorSelection.range(back.anchor, back.head) });
      editorView.scrollDOM.scrollTop = back.top;
    }
  }

  // ── View mode actions ──────────────────────

  function openInEditorTab() {
    openFileInEditor(workspaceIdx, path, { forceCode: true });
    close();
  }

  async function openExternalEditor() {
    close();
    try {
      const tabId = await ipc.spawnEditorTab(workspaceIdx, path);
      appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
    } catch (err) {
      toast(`Failed to open editor: ${err}`, "error");
    }
  }

  function copyToClipboard() {
    ipc.clipboardCopy(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
  }

  /** The view-mode header row. One definition — edit mode swaps the row out
   *  and `exitEditMode` asks for it back. */
  function renderViewActions() {
    actionsDiv.innerHTML = `
      <button data-variant="secondary" data-size="sm" class="ui-btn file-viewer-open-editor" title="Open in Editor Tab">Open in Editor</button>
      <button data-variant="secondary" data-size="sm" class="ui-btn file-viewer-find" title="Find in file (${formatShortcut("Ctrl+F")})">Find</button>
      <button data-variant="secondary" data-size="sm" class="ui-btn file-viewer-inline-edit" title="Quick Edit (${formatShortcut("Ctrl+I")})">Quick Edit</button>
      <button data-variant="secondary" data-size="sm" class="ui-btn file-viewer-edit" title="Open in $EDITOR (${formatShortcut("Ctrl+E")})">Edit</button>
      <button data-variant="secondary" data-size="sm" class="ui-btn file-viewer-copy" title="Copy to clipboard">Copy</button>
      <button data-variant="ghost" data-icon class="file-viewer-close ui-btn" title="Close" aria-label="Close">&times;</button>
    `;
    actionsDiv.querySelector(".file-viewer-open-editor")!.addEventListener("click", openInEditorTab);
    actionsDiv.querySelector(".file-viewer-find")!.addEventListener("click", () => {
      openSearchPanel(editorView);
    });
    actionsDiv.querySelector(".file-viewer-inline-edit")!.addEventListener("click", enterEditMode);
    actionsDiv.querySelector(".file-viewer-edit")!.addEventListener("click", openExternalEditor);
    actionsDiv.querySelector(".file-viewer-copy")!.addEventListener("click", copyToClipboard);
    actionsDiv.querySelector(".file-viewer-close")!.addEventListener("click", close);
  }

  renderViewActions();

  // ── Inline edit mode ──────────────────────

  function enterEditMode() {
    if (editing) return;
    editing = true;

    // Make editor writable
    editorView.dispatch({
      effects: readOnlyComp.reconfigure(EditorState.readOnly.of(false)),
    });
    editorView.focus();

    // Swap action buttons
    actionsDiv.innerHTML = `
      <button data-variant="primary" data-size="sm" class="file-viewer-save ui-btn">Save</button>
      <button data-variant="secondary" data-size="sm" class="ui-btn file-viewer-cancel">Cancel</button>
    `;

    actionsDiv.querySelector(".file-viewer-save")!.addEventListener("click", async () => {
      const newContent = editorView.state.doc.toString();
      try {
        await ipc.writeFileContent(workspaceIdx, path, newContent);
        content = newContent;
        toast("File saved", "success");
        exitEditMode();
      } catch (err) {
        toast(`Failed to save: ${err}`, "error");
      }
    });

    actionsDiv.querySelector(".file-viewer-cancel")!.addEventListener("click", () => {
      requestExitEditMode();
    });
  }

  /** Leave edit mode, confirming first when the buffer has unsaved edits. */
  function requestExitEditMode() {
    if (editorView.state.doc.toString() === content) {
      exitEditMode();
      return;
    }
    showConfirm({
      bodyHtml: `
        <p>Discard changes?</p>
        <p class="ws-delete-hint">Your unsaved edits will be lost.</p>
      `,
      actions: [
        { label: "Discard", kind: "danger", onSelect: () => exitEditMode() },
        { label: "Keep editing", kind: "secondary", isDefault: true },
      ],
    });
  }

  function exitEditMode() {
    editing = false;

    // Restore content and make read-only
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: content },
      effects: readOnlyComp.reconfigure(EditorState.readOnly.of(true)),
    });

    renderViewActions();
    editorView.focus();
  }

  // ── Keyboard shortcuts ──────────────────────

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop && !editing) close();
  });

  /** Keys typed inside CodeMirror's own panel (the find bar) belong to
   *  CodeMirror: Escape there closes the panel, not the viewer. */
  const inCmPanel = (e: KeyboardEvent) =>
    !!(e.target as HTMLElement | null)?.closest(".cm-panels");

  backdrop.addEventListener("keydown", (e) => {
    if (inCmPanel(e)) return;

    if (editing) {
      if (e.key === "s" && modCtrl(e)) {
        e.preventDefault();
        (actionsDiv.querySelector(".file-viewer-save") as HTMLButtonElement)?.click();
      }
      if (e.key === "Escape") {
        e.preventDefault();
        requestExitEditMode();
      }
      return;
    }

    if (e.key === "Escape") close();
    if (e.key === "i" && modCtrl(e)) {
      e.preventDefault();
      enterEditMode();
    }
    if (e.key === "e" && modCtrl(e)) {
      e.preventDefault();
      openExternalEditor();
    }
  });

  // A click on the dialog's chrome lands here, so the shortcuts above keep
  // working — but the initial focus goes to the editor, never the backdrop:
  // every key the viewer offers (Ctrl+F's find panel, vim's `/`, the arrows,
  // PageDown) is CodeMirror's, and a focused backdrop swallowed all of them.
  backdrop.setAttribute("tabindex", "0");

  if (target.query) {
    editorView.dispatch({
      effects: setSearchQuery.of(new SearchQuery({ search: target.query })),
    });
    openSearchPanel(editorView); // focuses its own input, pre-filled
  } else {
    editorView.focus();
  }
}

async function getLanguageExtension(filePath: string): Promise<Extension | null> {
  const ext = filePath.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "rs":
      return (await import("@codemirror/lang-rust")).rust();
    case "ts":
    case "tsx":
      return (await import("@codemirror/lang-javascript")).javascript({ typescript: true, jsx: ext === "tsx" });
    case "js":
    case "jsx":
      return (await import("@codemirror/lang-javascript")).javascript({ jsx: ext === "jsx" });
    case "py":
      return (await import("@codemirror/lang-python")).python();
    case "json":
      return (await import("@codemirror/lang-json")).json();
    case "html":
      return (await import("@codemirror/lang-html")).html();
    case "css":
      return (await import("@codemirror/lang-css")).css();
    case "md":
    case "markdown":
      return (await import("@codemirror/lang-markdown")).markdown();
    default:
      return null;
  }
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
