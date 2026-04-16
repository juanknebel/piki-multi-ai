import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { modCtrl, formatShortcut } from "../shortcuts";

import { Editor, rootCtx, defaultValueCtx, editorViewCtx, serializerCtx } from "@milkdown/kit/core";
import { commonmark,
  toggleStrongCommand, toggleEmphasisCommand,
  wrapInHeadingCommand, toggleInlineCodeCommand,
  wrapInBlockquoteCommand, wrapInBulletListCommand,
  wrapInOrderedListCommand, insertHrCommand,
  insertImageCommand, toggleLinkCommand,
} from "@milkdown/kit/preset/commonmark";
import { gfm } from "@milkdown/kit/preset/gfm";
import { history } from "@milkdown/kit/plugin/history";
import { listener, listenerCtx } from "@milkdown/kit/plugin/listener";
import { clipboard } from "@milkdown/kit/plugin/clipboard";
import { callCommand } from "@milkdown/kit/utils";
import "@milkdown/kit/prose/view/style/prosemirror.css";

interface MdEditorInstance {
  tabId: string;
  filePath: string;
  element: HTMLDivElement;
  editor: Editor | null;
}

const instances = new Map<string, MdEditorInstance>();
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
    inst.editor?.destroy();
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

function getMarkdownFromEditor(editor: Editor): string {
  return editor.action((ctx) => {
    const view = ctx.get(editorViewCtx);
    const serializer = ctx.get(serializerCtx);
    return serializer(view.state.doc);
  });
}

interface ToolbarItem {
  label: string;
  title: string;
  action: (editor: Editor) => void;
  separator?: boolean;
}

const toolbarItems: ToolbarItem[] = [
  { label: "B", title: "Bold", action: (e) => e.action(callCommand(toggleStrongCommand.key)) },
  { label: "I", title: "Italic", action: (e) => e.action(callCommand(toggleEmphasisCommand.key)) },
  { label: "H", title: "Heading", action: (e) => e.action(callCommand(wrapInHeadingCommand.key, 2)) },
  { label: "", title: "", action: () => {}, separator: true },
  { label: "\u2039\u203A", title: "Inline Code", action: (e) => e.action(callCommand(toggleInlineCodeCommand.key)) },
  { label: "\u201C", title: "Blockquote", action: (e) => e.action(callCommand(wrapInBlockquoteCommand.key)) },
  { label: "\u2022", title: "Bullet List", action: (e) => e.action(callCommand(wrapInBulletListCommand.key)) },
  { label: "1.", title: "Ordered List", action: (e) => e.action(callCommand(wrapInOrderedListCommand.key)) },
  { label: "", title: "", action: () => {}, separator: true },
  { label: "\uD83D\uDD17", title: "Link", action: (e) => e.action(callCommand(toggleLinkCommand.key, { href: "" })) },
  { label: "\uD83D\uDDBC", title: "Image", action: (e) => e.action(callCommand(insertImageCommand.key, { src: "" })) },
  { label: "\u2014", title: "Horizontal Rule", action: (e) => e.action(callCommand(insertHrCommand.key)) },
];

function buildToolbar(el: HTMLDivElement, editor: Editor) {
  const toolbar = el.querySelector<HTMLDivElement>(".mk-toolbar")!;
  for (const item of toolbarItems) {
    if (item.separator) {
      const sep = document.createElement("span");
      sep.className = "mk-toolbar-sep";
      toolbar.appendChild(sep);
      continue;
    }
    const btn = document.createElement("button");
    btn.className = "mk-toolbar-btn";
    btn.title = item.title;
    btn.textContent = item.label;
    if (item.title === "Bold") btn.style.fontWeight = "700";
    if (item.title === "Italic") btn.style.fontStyle = "italic";
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      item.action(editor);
    });
    toolbar.appendChild(btn);
  }
}

function createPanel(tabId: string, filePath: string): MdEditorInstance {
  const el = document.createElement("div");
  el.className = "md-editor-panel";
  el.innerHTML = `
    <div class="md-editor-toolbar">
      <span class="md-editor-path" title="${esc(filePath)}">${esc(filePath)}</span>
      <div class="mk-toolbar"></div>
      <button class="api-btn md-editor-save" title="Save (${formatShortcut("Ctrl+S")})">Save</button>
    </div>
    <div class="md-editor-body"></div>
  `;

  mainContent.appendChild(el);

  const inst: MdEditorInstance = { tabId, filePath, element: el, editor: null };

  const wsIdx = appState.activeWorkspace;
  ipc.readFileContent(wsIdx, filePath).then(async (content) => {
    const bodyEl = el.querySelector<HTMLDivElement>(".md-editor-body")!;

    const editor = await Editor.make()
      .config((ctx) => {
        ctx.set(rootCtx, bodyEl);
        ctx.set(defaultValueCtx, content);
        ctx.get(listenerCtx).markdownUpdated(() => {
          // Content tracked internally; retrieved on save via getMarkdownFromEditor
        });
      })
      .use(commonmark)
      .use(gfm)
      .use(history)
      .use(listener)
      .use(clipboard)
      .create();

    inst.editor = editor;

    buildToolbar(el, editor);

    // Save button
    el.querySelector(".md-editor-save")!.addEventListener("click", async () => {
      if (!inst.editor) return;
      try {
        const md = getMarkdownFromEditor(inst.editor);
        await ipc.writeFileContent(wsIdx, filePath, md);
        toast("File saved", "success");
      } catch (err) {
        toast(`Save failed: ${err}`, "error");
      }
    });

    // Ctrl+S to save — intercept on the editor's DOM
    bodyEl.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === "s" && modCtrl(e)) {
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
