import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import EasyMDE from "easymde";
import "easymde/dist/easymde.min.css";

interface MdEditorInstance {
  tabId: string;
  filePath: string;
  element: HTMLDivElement;
  easyMde: EasyMDE | null;
}

const instances = new Map<string, MdEditorInstance>();
// Map tab IDs to file paths (set before showing the panel)
const pendingFiles = new Map<string, string>();
let mainContent: HTMLElement;

export function initMarkdownEditorPanel(container: HTMLElement) {
  mainContent = container;
}

export function registerMarkdownFile(tabId: string, filePath: string) {
  pendingFiles.set(tabId, filePath);
}

export function hideMarkdownEditorPanels() {
  for (const inst of instances.values()) {
    inst.element.style.display = "none";
  }
}

export function destroyMarkdownEditorPanel(tabId: string) {
  const inst = instances.get(tabId);
  if (inst) {
    inst.easyMde?.toTextArea();
    inst.element.remove();
    instances.delete(tabId);
  }
  pendingFiles.delete(tabId);
}

export function showMarkdownEditorPanel(tabId: string) {
  let inst = instances.get(tabId);
  if (!inst) {
    const filePath = pendingFiles.get(tabId);
    if (!filePath) return;
    inst = createPanel(tabId, filePath);
    instances.set(tabId, inst);
    pendingFiles.delete(tabId);
  }
  inst.element.style.display = "flex";
}

function createPanel(tabId: string, filePath: string): MdEditorInstance {
  const el = document.createElement("div");
  el.className = "md-editor-panel";
  el.innerHTML = `
    <div class="md-editor-toolbar">
      <span class="md-editor-path" title="${esc(filePath)}">${esc(filePath)}</span>
      <button class="api-btn md-editor-save" title="Save (Ctrl+S)">Save</button>
    </div>
    <div class="md-editor-body"></div>
  `;

  mainContent.appendChild(el);

  const inst: MdEditorInstance = { tabId, filePath, element: el, easyMde: null };

  // Load file and init editor
  const wsIdx = appState.activeWorkspace;
  ipc.readFileContent(wsIdx, filePath).then((content) => {
    const bodyEl = el.querySelector(".md-editor-body")!;
    const textarea = document.createElement("textarea");
    bodyEl.appendChild(textarea);

    const easyMde = new EasyMDE({
      element: textarea,
      initialValue: content,
      spellChecker: false,
      status: false,
      autofocus: true,
      toolbar: ["bold", "italic", "heading", "|", "code", "quote", "unordered-list", "ordered-list", "|", "link", "image", "horizontal-rule", "|", "preview", "side-by-side"],
      sideBySideFullscreen: false,
      minHeight: "100%",
    });
    inst.easyMde = easyMde;

    // Save button
    el.querySelector(".md-editor-save")!.addEventListener("click", async () => {
      try {
        await ipc.writeFileContent(wsIdx, filePath, easyMde.value());
        toast("File saved", "success");
      } catch (err) {
        toast(`Save failed: ${err}`, "error");
      }
    });

    // Ctrl+S to save
    easyMde.codemirror.on("keydown", (_instance, e: KeyboardEvent) => {
      if (e.key === "s" && e.ctrlKey) {
        e.preventDefault();
        el.querySelector<HTMLButtonElement>(".md-editor-save")?.click();
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
