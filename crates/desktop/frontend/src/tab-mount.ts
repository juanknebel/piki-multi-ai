// Tab mount dispatcher.
//
// Each provider type has its own panel module with `mountInto(tabId, host)` and
// `unmountTab(tabId)` functions. This dispatcher routes by provider so the rest
// of the UI can mount/unmount a tab without knowing its content type.

import type { TabInfo } from "./types";
import { mountTerminalInto, unmountTerminal } from "./components/terminal-panel";
import { mountKanbanInto, unmountKanban } from "./components/kanban-panel";
import { mountApiInto, unmountApi } from "./components/api-panel";
import { mountMarkdownEditorInto, unmountMarkdownEditor } from "./components/markdown-editor-panel";
import { mountCodeEditorInto, unmountCodeEditor } from "./components/code-editor-panel";
import { mountWebPreviewInto, unmountWebPreview } from "./components/web-preview-panel";

function isTerminalProvider(tab: TabInfo): boolean {
  if (tab.provider === "Shell") return true;
  if (typeof tab.provider === "object" && "Custom" in tab.provider) return true;
  return false;
}

/** Mount a tab's content into the given host element. Idempotent. */
export function mountTab(tab: TabInfo, host: HTMLElement, wsIdx: number) {
  if (isTerminalProvider(tab)) {
    mountTerminalInto(tab.id, host);
  } else if (tab.provider === "Kanban") {
    void mountKanbanInto(tab.id, host);
  } else if (tab.provider === "Api") {
    mountApiInto(tab.id, wsIdx, host);
  } else if (tab.provider === "Markdown") {
    mountMarkdownEditorInto(tab.id, host);
  } else if (tab.provider === "CodeEditor") {
    mountCodeEditorInto(tab.id, host);
  } else if (tab.provider === "WebPreview") {
    mountWebPreviewInto(tab.id, host);
  }
}

/** Hide a tab's content without destroying its state. */
export function unmountTab(tab: TabInfo) {
  if (isTerminalProvider(tab)) {
    unmountTerminal(tab.id);
  } else if (tab.provider === "Kanban") {
    unmountKanban(tab.id);
  } else if (tab.provider === "Api") {
    unmountApi(tab.id);
  } else if (tab.provider === "Markdown") {
    unmountMarkdownEditor(tab.id);
  } else if (tab.provider === "CodeEditor") {
    unmountCodeEditor(tab.id);
  } else if (tab.provider === "WebPreview") {
    unmountWebPreview(tab.id);
  }
}
