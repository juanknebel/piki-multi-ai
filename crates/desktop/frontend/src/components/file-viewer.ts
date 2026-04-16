import { EditorView, basicSetup } from "codemirror";
import { EditorState, Compartment, Extension } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { vim } from "@replit/codemirror-vim";
import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { modCtrl, formatShortcut } from "../shortcuts";
import { registerCodeFile } from "./code-editor-panel";

const readOnlyComp = new Compartment();

export async function showFileViewer(workspaceIdx: number, path: string) {
  // Remove existing viewer
  document.querySelector(".file-viewer-backdrop")?.remove();

  let content: string;
  try {
    content = await ipc.readFileContent(workspaceIdx, path);
  } catch (err) {
    toast(`Failed to read file: ${err}`, "error");
    return;
  }

  const fileName = path.split("/").pop() || path;

  const backdrop = document.createElement("div");
  backdrop.className = "file-viewer-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "file-viewer-dialog";

  dialog.innerHTML = `
    <div class="file-viewer-header">
      <span class="file-viewer-title" title="${escapeAttr(path)}">${escapeHtml(fileName)}<span class="file-viewer-path">${escapeHtml(path)}</span></span>
      <div class="file-viewer-actions">
        <button class="file-viewer-btn file-viewer-open-editor" title="Open in Editor Tab">Open in Editor</button>
        <button class="file-viewer-btn file-viewer-inline-edit" title="Quick Edit (${formatShortcut("Ctrl+I")})">Quick Edit</button>
        <button class="file-viewer-btn file-viewer-edit" title="Open in $EDITOR (${formatShortcut("Ctrl+E")})">Edit</button>
        <button class="file-viewer-btn file-viewer-copy" title="Copy to clipboard">Copy</button>
        <button class="file-viewer-btn file-viewer-close">&times;</button>
      </div>
    </div>
    <div class="file-viewer-body"></div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

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
        ...(langExt ? [langExt] : []),
        oneDark,
        readOnlyComp.of(EditorState.readOnly.of(true)),
        EditorView.theme({
          "&": { height: "100%" },
          ".cm-scroller": { overflow: "auto" },
        }),
      ],
    }),
    parent: body,
  });

  let editing = false;

  const close = () => {
    editorView.destroy();
    backdrop.remove();
  };

  // ── View mode actions ──────────────────────

  dialog.querySelector(".file-viewer-close")!.addEventListener("click", close);

  function openInEditorTab() {
    const tabId = crypto.randomUUID();
    registerCodeFile(tabId, path, workspaceIdx);
    appState.addTab(workspaceIdx, { id: tabId, provider: "CodeEditor", alive: true });
    close();
  }

  dialog.querySelector(".file-viewer-open-editor")!.addEventListener("click", openInEditorTab);

  dialog.querySelector(".file-viewer-edit")!.addEventListener("click", async () => {
    close();
    try {
      const tabId = await ipc.spawnEditorTab(workspaceIdx, path);
      appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
    } catch (err) {
      toast(`Failed to open editor: ${err}`, "error");
    }
  });

  dialog.querySelector(".file-viewer-copy")!.addEventListener("click", () => {
    ipc.clipboardCopy(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
  });

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
      <button class="file-viewer-btn file-viewer-save">Save</button>
      <button class="file-viewer-btn file-viewer-cancel">Cancel</button>
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
      exitEditMode();
    });
  }

  function exitEditMode() {
    editing = false;

    // Restore content and make read-only
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: content },
      effects: readOnlyComp.reconfigure(EditorState.readOnly.of(true)),
    });

    actionsDiv.innerHTML = `
      <button class="file-viewer-btn file-viewer-open-editor" title="Open in Editor Tab">Open in Editor</button>
      <button class="file-viewer-btn file-viewer-inline-edit" title="Quick Edit (${formatShortcut("Ctrl+I")})">Quick Edit</button>
      <button class="file-viewer-btn file-viewer-edit" title="Open in $EDITOR (${formatShortcut("Ctrl+E")})">Edit</button>
      <button class="file-viewer-btn file-viewer-copy" title="Copy to clipboard">Copy</button>
      <button class="file-viewer-btn file-viewer-close">&times;</button>
    `;

    actionsDiv.querySelector(".file-viewer-open-editor")!.addEventListener("click", openInEditorTab);
    actionsDiv.querySelector(".file-viewer-inline-edit")!.addEventListener("click", enterEditMode);
    actionsDiv.querySelector(".file-viewer-edit")!.addEventListener("click", async () => {
      close();
      try {
        const tabId = await ipc.spawnEditorTab(workspaceIdx, path);
        appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
      } catch (err) {
        toast(`Failed to open editor: ${err}`, "error");
      }
    });
    actionsDiv.querySelector(".file-viewer-copy")!.addEventListener("click", () => {
      ipc.clipboardCopy(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
    });
    actionsDiv.querySelector(".file-viewer-close")!.addEventListener("click", close);

    backdrop.focus();
  }

  dialog.querySelector(".file-viewer-inline-edit")!.addEventListener("click", enterEditMode);

  // ── Keyboard shortcuts ──────────────────────

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop && !editing) close();
  });

  backdrop.addEventListener("keydown", (e) => {
    if (editing) {
      if (e.key === "s" && modCtrl(e)) {
        e.preventDefault();
        (actionsDiv.querySelector(".file-viewer-save") as HTMLButtonElement)?.click();
      }
      if (e.key === "Escape") {
        e.preventDefault();
        exitEditMode();
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
      close();
      ipc.spawnEditorTab(workspaceIdx, path).then((tabId) => {
        appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
      }).catch((err) => {
        toast(`Failed to open editor: ${err}`, "error");
      });
    }
  });

  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
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
