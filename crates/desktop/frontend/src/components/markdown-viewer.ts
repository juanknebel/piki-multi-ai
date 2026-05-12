import { Marked } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js/lib/common";
import "highlight.js/styles/atom-one-dark.css";
import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { registerMarkdownFile } from "./markdown-editor-panel";
import { modCtrl, formatShortcut } from "../shortcuts";

// CommonMark + GFM (tables, strikethrough, task lists) with syntax-highlighted
// code fences via highlight.js. Instantiated once and reused for every render.
const md = new Marked(
  markedHighlight({
    emptyLangClass: "hljs",
    langPrefix: "hljs language-",
    highlight(code, lang) {
      const language = hljs.getLanguage(lang) ? lang : "plaintext";
      return hljs.highlight(code, { language, ignoreIllegals: true }).value;
    },
  }),
);
md.setOptions({ gfm: true, breaks: false });

let overlayEl: HTMLElement | null = null;

export async function showMarkdown(filePath: string) {
  if (overlayEl) { overlayEl.remove(); overlayEl = null; }

  const wsIdx = appState.activeWorkspace;
  let content: string;
  try {
    content = await ipc.readMarkdownFile(wsIdx, filePath);
  } catch (err) {
    console.error("Failed to read markdown:", err);
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.style.paddingTop = "3vh";

  const viewer = document.createElement("div");
  viewer.className = "diff-viewer";
  viewer.style.maxWidth = "800px";

  const header = document.createElement("div");
  header.className = "diff-header";

  const fileName = filePath.split("/").pop() || filePath;

  const close = () => { overlayEl?.remove(); overlayEl = null; };
  let editing = false;

  function renderViewMode() {
    header.innerHTML = `
      <span class="diff-title">${esc(fileName)}<span style="color:var(--text-muted);font-weight:400;margin-left:8px;font-size:11px">${esc(filePath)}</span></span>
      <div style="display:flex;gap:4px;align-items:center">
        <button class="file-viewer-btn md-quick-edit" title="Quick Edit (${formatShortcut("Ctrl+I")})">Quick Edit</button>
        <button class="file-viewer-btn md-edit" title="Open in $EDITOR (${formatShortcut("Ctrl+E")})">Edit</button>
        <button class="file-viewer-btn md-copy" title="Copy to clipboard">Copy</button>
        <button class="dialog-close">×</button>
      </div>
    `;

    const body = viewer.querySelector(".md-body") as HTMLElement;
    body.innerHTML = "";
    const mdContent = document.createElement("div");
    mdContent.className = "md-content";
    mdContent.innerHTML = renderMarkdown(content);
    body.appendChild(mdContent);

    header.querySelector(".dialog-close")!.addEventListener("click", close);
    header.querySelector(".md-quick-edit")!.addEventListener("click", enterEditMode);
    header.querySelector(".md-edit")!.addEventListener("click", () => {
      close();
      const tabId = `md-${Date.now()}`;
      registerMarkdownFile(tabId, filePath);
      appState.addTab(wsIdx, { id: tabId, provider: "Markdown", alive: true });
    });
    header.querySelector(".md-copy")!.addEventListener("click", () => {
      ipc.clipboardCopy(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
    });
  }

  function enterEditMode() {
    if (editing) return;
    editing = true;

    header.innerHTML = `
      <span class="diff-title">${esc(fileName)} <span style="color:var(--text-muted);font-weight:400;font-size:11px">(editing)</span></span>
      <div style="display:flex;gap:4px;align-items:center">
        <button class="file-viewer-btn md-save">Save</button>
        <button class="file-viewer-btn md-cancel">Cancel</button>
      </div>
    `;

    const body = viewer.querySelector(".md-body") as HTMLElement;
    body.innerHTML = "";
    const textarea = document.createElement("textarea");
    textarea.className = "file-viewer-textarea";
    textarea.value = content;
    textarea.spellcheck = false;
    body.appendChild(textarea);
    textarea.focus();

    header.querySelector(".md-save")!.addEventListener("click", async () => {
      try {
        await ipc.writeFileContent(wsIdx, filePath, textarea.value);
        content = textarea.value;
        toast("File saved", "success");
        editing = false;
        renderViewMode();
      } catch (err) {
        toast(`Failed to save: ${err}`, "error");
      }
    });
    header.querySelector(".md-cancel")!.addEventListener("click", () => {
      editing = false;
      renderViewMode();
    });
  }

  viewer.appendChild(header);

  const body = document.createElement("div");
  body.className = "md-body";
  body.style.cssText = "flex:1;overflow:auto;min-height:0";
  viewer.appendChild(body);

  backdrop.appendChild(viewer);
  document.body.appendChild(backdrop);
  overlayEl = backdrop;

  renderViewMode();

  backdrop.addEventListener("click", (e) => { if (e.target === backdrop && !editing) close(); });
  backdrop.addEventListener("keydown", (e) => {
    if (editing) {
      if (e.key === "s" && modCtrl(e)) {
        e.preventDefault();
        (header.querySelector(".md-save") as HTMLButtonElement)?.click();
      }
      if (e.key === "Escape") { e.preventDefault(); editing = false; renderViewMode(); }
      return;
    }
    if (e.key === "Escape") close();
    if (e.key === "i" && modCtrl(e)) { e.preventDefault(); enterEditMode(); }
    if (e.key === "e" && modCtrl(e)) {
      e.preventDefault();
      close();
      ipc.spawnEditorTab(wsIdx, filePath).then((tabId) => {
        appState.addTab(wsIdx, { id: tabId, provider: "Shell", alive: true });
      }).catch((err) => toast(`Failed to open editor: ${err}`, "error"));
    }
  });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

/** Render markdown source to HTML via marked (GFM) + highlight.js fences. */
function renderMarkdown(src: string): string {
  // `marked.parse` is synchronous when no async extensions are registered;
  // the Marked typings still surface a Promise type, so we coerce. The cast
  // is safe because we only configure synchronous extensions above.
  return md.parse(src, { async: false }) as string;
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
