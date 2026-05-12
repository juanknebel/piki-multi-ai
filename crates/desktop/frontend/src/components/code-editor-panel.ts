import { EditorView, basicSetup } from "codemirror";
import { EditorState, Extension } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { vim } from "@replit/codemirror-vim";
import { languageServer } from "codemirror-languageserver";
import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { modCtrl, formatShortcut } from "../shortcuts";

const LANGUAGE_IDS: Record<string, string> = {
  rs: "rust",
  ts: "typescript",
  tsx: "typescriptreact",
  js: "javascript",
  jsx: "javascriptreact",
  py: "python",
  json: "json",
  html: "html",
  css: "css",
  md: "markdown",
  go: "go",
  c: "c",
  cpp: "cpp",
  java: "java",
  toml: "toml",
  yaml: "yaml",
  yml: "yaml",
};

interface CodeEditorInstance {
  tabId: string;
  filePath: string;
  workspaceIdx: number;
  element: HTMLDivElement;
  editorView: EditorView | null;
  originalContent: string;
}

const instances = new Map<string, CodeEditorInstance>();
const pendingFiles = new Map<string, { filePath: string; workspaceIdx: number }>();
let mainContent: HTMLElement;

export function initCodeEditorPanel(container: HTMLElement) {
  mainContent = container;

  // React to per-path file changes from the backend file watcher.
  // The event fires with the exact paths that changed, so we only re-read
  // the buffers that are actually affected.
  void ipc.onFileChanged((event) => {
    const changedSet = new Set(event.paths);
    for (const inst of instances.values()) {
      if (inst.workspaceIdx === event.workspace_idx && changedSet.has(inst.filePath)) {
        void checkSingleInstance(inst);
      }
    }
  });

  // Fallback: also react to coarse files-changed (git status refresh) so we
  // catch branch switches and commits that the path watcher may miss.
  appState.on("files-changed", () => {
    void checkExternalChanges();
  });
}

export function isCodeEditorDirty(tabId: string): boolean {
  const inst = instances.get(tabId);
  if (!inst?.editorView) return false;
  return inst.editorView.state.doc.toString() !== inst.originalContent;
}

export async function saveCodeEditor(tabId: string): Promise<void> {
  const inst = instances.get(tabId);
  if (!inst?.editorView) return;
  await saveInstance(inst);
}

async function saveInstance(inst: CodeEditorInstance): Promise<void> {
  if (!inst.editorView) return;
  const newContent = inst.editorView.state.doc.toString();
  await ipc.writeFileContent(inst.workspaceIdx, inst.filePath, newContent);
  inst.originalContent = newContent;
  inst.element.querySelector<HTMLElement>(".code-editor-dirty")!.style.display = "none";
}

export function registerCodeFile(tabId: string, filePath: string, workspaceIdx: number) {
  pendingFiles.set(tabId, { filePath, workspaceIdx });
}

export function hideCodeEditorPanels() {
  for (const inst of instances.values()) {
    inst.element.style.display = "none";
  }
}

export function destroyCodeEditorPanel(tabId: string) {
  const inst = instances.get(tabId);
  if (inst) {
    inst.editorView?.destroy();
    inst.element.remove();
    instances.delete(tabId);
  }
  pendingFiles.delete(tabId);
}

export function showCodeEditorPanel(tabId: string) {
  mountCodeEditorInto(tabId, mainContent);
}

export function mountCodeEditorInto(tabId: string, host: HTMLElement) {
  let inst = instances.get(tabId);
  if (!inst) {
    const pending = pendingFiles.get(tabId);
    if (!pending) return;
    inst = createPanel(tabId, pending.filePath, pending.workspaceIdx);
    instances.set(tabId, inst);
    pendingFiles.delete(tabId);
  }
  if (inst.element.parentElement !== host) {
    host.appendChild(inst.element);
    // Force CodeMirror to recompute its viewport for the new host size.
    inst.editorView?.requestMeasure();
  }
  inst.element.style.display = "flex";
}

export function unmountCodeEditor(tabId: string) {
  const inst = instances.get(tabId);
  if (inst) inst.element.style.display = "none";
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

function getLanguageId(filePath: string): string {
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
  return LANGUAGE_IDS[ext] ?? "plaintext";
}

function getWorkspacePath(workspaceIdx: number): string {
  const ws = appState.workspaces[workspaceIdx];
  return ws?.info.path ?? "";
}

function updateLspStatus(el: HTMLElement, status: "connected" | "unavailable" | "error" | "") {
  const badge = el.querySelector<HTMLElement>(".code-editor-lsp-status");
  if (!badge) return;
  badge.className = `code-editor-lsp-status ${status}`;
  switch (status) {
    case "connected":
      badge.textContent = "LSP";
      break;
    case "unavailable":
      badge.textContent = "";
      break;
    case "error":
      badge.textContent = "LSP error";
      break;
    default:
      badge.textContent = "";
  }
}

function createPanel(tabId: string, filePath: string, workspaceIdx: number): CodeEditorInstance {
  const el = document.createElement("div");
  el.className = "code-editor-panel";
  el.innerHTML = `
    <div class="code-editor-toolbar">
      <span class="code-editor-path" title="${esc(filePath)}">${esc(filePath)}</span>
      <span class="code-editor-dirty" style="display:none">modified</span>
      <span class="code-editor-lsp-status"></span>
      <button class="code-editor-save" title="Save (${formatShortcut("Ctrl+S")})">Save</button>
    </div>
    <div class="code-editor-body"></div>
  `;

  mainContent.appendChild(el);

  const inst: CodeEditorInstance = {
    tabId,
    filePath,
    workspaceIdx,
    element: el,
    editorView: null,
    originalContent: "",
  };

  ipc.readFileContent(workspaceIdx, filePath).then(async (content) => {
    inst.originalContent = content;
    const bodyEl = el.querySelector<HTMLDivElement>(".code-editor-body")!;

    const langExt = await getLanguageExtension(filePath);

    // Try to connect LSP
    let lspExtensions: Extension[] = [];
    try {
      const lspInfo = await ipc.lspEnsureServer(workspaceIdx, filePath);
      if (lspInfo) {
        const wsPath = getWorkspacePath(workspaceIdx);
        const rootUri = `file://${wsPath}`;
        const documentUri = `file://${wsPath}/${filePath}`;
        const serverUri = `ws://127.0.0.1:${lspInfo.ws_port}${lspInfo.ws_path}` as `ws://${string}`;

        lspExtensions = languageServer({
          serverUri,
          rootUri,
          workspaceFolders: [{ uri: rootUri, name: wsPath.split("/").pop() ?? "workspace" }],
          documentUri,
          languageId: getLanguageId(filePath),
        });
        updateLspStatus(el, "connected");
      }
    } catch (err) {
      console.warn("LSP not available for", filePath, err);
      updateLspStatus(el, "unavailable");
    }

    const state = EditorState.create({
      doc: content,
      extensions: [
        vim(),
        basicSetup,
        ...(langExt ? [langExt] : []),
        ...lspExtensions,
        oneDark,
        EditorView.theme({
          "&": { height: "100%" },
          ".cm-scroller": { overflow: "auto" },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const isDirty = update.state.doc.toString() !== inst.originalContent;
            el.querySelector<HTMLElement>(".code-editor-dirty")!.style.display =
              isDirty ? "inline" : "none";
          }
        }),
      ],
    });

    inst.editorView = new EditorView({ state, parent: bodyEl });

    // Save button
    el.querySelector(".code-editor-save")!.addEventListener("click", async () => {
      try {
        await saveInstance(inst);
        toast("File saved", "success");
      } catch (err) {
        toast(`Save failed: ${err}`, "error");
      }
    });

    // Ctrl+S to save
    bodyEl.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === "s" && modCtrl(e)) {
        e.preventDefault();
        el.querySelector<HTMLButtonElement>(".code-editor-save")?.click();
      }
    });
  }).catch((err) => {
    toast(`Failed to load file: ${err}`, "error");
  });

  return inst;
}

async function checkSingleInstance(inst: CodeEditorInstance): Promise<void> {
  if (!inst.editorView) return;
  let onDisk: string;
  try {
    onDisk = await ipc.readFileContent(inst.workspaceIdx, inst.filePath);
  } catch {
    return;
  }
  if (onDisk === inst.originalContent) return;

  const editorContent = inst.editorView.state.doc.toString();
  const localDirty = editorContent !== inst.originalContent;

  if (!localDirty) {
    inst.editorView.dispatch({
      changes: { from: 0, to: inst.editorView.state.doc.length, insert: onDisk },
    });
    inst.originalContent = onDisk;
    toast(`Reloaded ${shortName(inst.filePath)} from disk`, "info");
  } else {
    showReloadConflictPrompt(inst, onDisk);
  }
}

async function checkExternalChanges(): Promise<void> {
  for (const inst of instances.values()) {
    await checkSingleInstance(inst);
  }
}

function showReloadConflictPrompt(inst: CodeEditorInstance, onDisk: string): void {
  if (inst.element.querySelector(".code-editor-reload-prompt")) return;

  const fileName = shortName(inst.filePath);
  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm code-editor-reload-prompt";
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p><strong>${esc(fileName)}</strong> changed on disk</p>
      <p class="ws-delete-hint">You have unsaved local changes. What would you like to do?</p>
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-secondary keep-yours">Keep yours</button>
        <button class="dialog-btn dialog-btn-danger reload-disk">Reload from disk</button>
      </div>
    </div>
  `;

  overlay.querySelector(".reload-disk")!.addEventListener("click", () => {
    if (inst.editorView) {
      inst.editorView.dispatch({
        changes: { from: 0, to: inst.editorView.state.doc.length, insert: onDisk },
      });
    }
    inst.originalContent = onDisk;
    inst.element.querySelector<HTMLElement>(".code-editor-dirty")!.style.display = "none";
    overlay.remove();
    toast(`Reloaded ${fileName} from disk`, "info");
  });

  overlay.querySelector(".keep-yours")!.addEventListener("click", () => {
    // Rebase the dirty baseline so the indicator contrasts with the new disk content.
    // On next save, the user's buffer wins (last-write-wins semantics).
    inst.originalContent = onDisk;
    // Dirty indicator was already showing; keep it on since editor still differs.
    overlay.remove();
  });

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) overlay.remove();
  });

  document.body.appendChild(overlay);
}

export function showUnsavedChangesPrompt(
  tabId: string,
  onResolve: (action: "save" | "discard" | "cancel") => void,
): void {
  const inst = instances.get(tabId);
  if (!inst) {
    onResolve("discard");
    return;
  }
  const fileName = shortName(inst.filePath);

  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm code-editor-unsaved-prompt";
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p>Unsaved changes in <strong>${esc(fileName)}</strong></p>
      <p class="ws-delete-hint">What would you like to do?</p>
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-primary action-save">Save</button>
        <button class="dialog-btn dialog-btn-danger action-discard">Discard</button>
        <button class="dialog-btn dialog-btn-secondary action-cancel">Cancel</button>
      </div>
    </div>
  `;

  overlay.querySelector(".action-save")!.addEventListener("click", async () => {
    try {
      await saveInstance(inst);
      toast("File saved", "success");
      overlay.remove();
      onResolve("save");
    } catch (err) {
      toast(`Save failed: ${err}`, "error");
    }
  });

  overlay.querySelector(".action-discard")!.addEventListener("click", () => {
    overlay.remove();
    onResolve("discard");
  });

  overlay.querySelector(".action-cancel")!.addEventListener("click", () => {
    overlay.remove();
    onResolve("cancel");
  });

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      overlay.remove();
      onResolve("cancel");
    }
  });

  document.body.appendChild(overlay);
}

function shortName(filePath: string): string {
  return filePath.split("/").pop() || filePath;
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
