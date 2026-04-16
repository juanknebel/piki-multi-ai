import { EditorView, basicSetup } from "codemirror";
import { EditorState, Extension } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
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
  let inst = instances.get(tabId);
  if (!inst) {
    const pending = pendingFiles.get(tabId);
    if (!pending) return;
    inst = createPanel(tabId, pending.filePath, pending.workspaceIdx);
    instances.set(tabId, inst);
    pendingFiles.delete(tabId);
  }
  inst.element.style.display = "flex";
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
      if (!inst.editorView) return;
      const newContent = inst.editorView.state.doc.toString();
      try {
        await ipc.writeFileContent(workspaceIdx, filePath, newContent);
        inst.originalContent = newContent;
        el.querySelector<HTMLElement>(".code-editor-dirty")!.style.display = "none";
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

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
