// The ONE place a content is opened somewhere: as a new top-level tab (the
// menu bar, palette, sidebar, file tree, …) or into a specific BLANK pane
// (the chooser a blank pane shows). Every opener takes an optional
// `{ paneId }`; without it the behaviour is the classic "new tab".
//
// Singletons (Kanban, API Explorer, Web Preview): when one already exists in
// the workspace and the user picks it for a pane, the confirm offers
// *Move here* (re-parent it — `appState.moveContentToPane`) / *Go there* /
// Cancel. Without a pane target the existing one is simply focused
// (`focusSingletonTab`), as before.
//
// This module also owns the `ContentRestorer` the layout snapshot uses to
// bring non-PTY contents back after a restart (`installContentRestorer`).

import { appState, type ContentRestorer } from "../state";
import * as ipc from "../ipc";
import type { PaneId } from "../pane-tree";
import type { AIProvider, TabInfo } from "../types";
import { getProviderKey, getProviderLabel, getTabLabel } from "../types";
import { describeContent } from "../layout-snapshot";
import { isMarkdownPath } from "../file-kind";
import { showConfirm, escapeHtml } from "./confirm";
import { toast, reportError } from "./toast";
import { getCachedProviderTabs, preloadProviderTabs } from "./provider-cache";
import { registerCodeFile, getCodeEditorFilePath, getCodeEditorFileName, hasCodeEditorInstance } from "./code-editor-panel";
import { registerMarkdownFile, getMarkdownEditorFilePath, getMarkdownEditorFileName, hasMarkdownEditorInstance } from "./markdown-editor-panel";
import { openWebPreviewTab, registerWebPreview, getWebPreviewUrl } from "./web-preview-panel";

export interface OpenTarget {
  /** Put the content into this blank pane of the active workspace tab
   *  instead of opening a new top-level tab. */
  paneId?: PaneId;
}

/** Label of a content for chrome that names it: editors show their file
 *  name, everything else `getTabLabel` (custom title > OSC title > provider). */
export function contentLabel(c: TabInfo): string {
  if (c.provider === "CodeEditor") return c.custom_title || getCodeEditorFileName(c.id) || "Editor";
  if (c.provider === "Markdown") return c.custom_title || getMarkdownEditorFileName(c.id) || "Markdown";
  return getTabLabel(c, appState.getTabShellState(c.id)?.title);
}

/** Providers offerable as a process in a blank pane / empty workspace:
 *  Shell first, then the configured agents (shared provider cache). */
export async function getPaneProviderChoices(): Promise<AIProvider[]> {
  await preloadProviderTabs();
  return ["Shell", ...getCachedProviderTabs()];
}

/** The non-process contents every chooser offers, in display order. */
export const TOOL_CHOICES: readonly AIProvider[] = ["WebPreview", "Kanban", "Api"];

/** Place `tab` per `target`: into the pane, or as a new top-level tab. */
function place(tab: TabInfo, target: OpenTarget) {
  if (target.paneId) appState.setPaneContent(target.paneId, tab);
  else appState.addTab(appState.activeWorkspace, tab);
}

/** For a singleton provider with a pane target: resolve what to do when one
 *  already exists. "create" = none exists, go ahead; "done" = handled
 *  (moved, focused or cancelled). */
async function resolveSingleton(provider: AIProvider, target: OpenTarget): Promise<"create" | "done"> {
  if (!appState.isSingletonProvider(provider)) return "create";
  const ws = appState.activeWs;
  const existing = ws?.tabs.find((t) => t.provider === provider);
  if (!ws || !existing) return "create";
  if (!target.paneId) {
    appState.focusSingletonTab(provider);
    return "done";
  }
  const paneId = target.paneId;
  const label = getProviderLabel(provider);
  await new Promise<void>((resolve) => {
    showConfirm({
      bodyHtml: `
        <p><strong>${escapeHtml(label)}</strong> is already open in this workspace.</p>
        <p class="ws-delete-hint">Move it into this pane, or go to where it is. Only one ${escapeHtml(label)} per workspace.</p>
      `,
      actions: [
        {
          label: "Move here",
          kind: "primary",
          isDefault: true,
          autofocus: true,
          onSelect: () => {
            if (!appState.moveContentToPane(existing.id, paneId)) toast(`Could not move ${label} here`, "error");
            resolve();
          },
        },
        { label: "Go there", kind: "secondary", onSelect: () => { appState.focusSingletonTab(provider); resolve(); } },
        { label: "Cancel", kind: "secondary", onSelect: () => resolve() },
      ],
      onDismiss: () => resolve(),
    });
  });
  return "done";
}

/** Open a backend-spawned provider (Shell, an agent, Kanban, API) or the Web
 *  Preview. The one entry point for File ▸ New Tab, the palette, the
 *  sidebar icons, the empty workspace and the blank pane. */
export async function openProvider(provider: AIProvider, target: OpenTarget = {}) {
  if (!appState.activeWs) {
    toast("Create a workspace first", "error");
    return;
  }
  if ((await resolveSingleton(provider, target)) === "done") return;
  if (provider === "WebPreview") {
    openWebPreviewTab({ paneId: target.paneId });
    return;
  }
  const wsIdx = appState.activeWorkspace;
  try {
    const tabId = await ipc.spawnTab(wsIdx, getProviderKey(provider));
    place({ id: tabId, provider, alive: true }, target);
  } catch (err) {
    reportError(`Open ${getProviderLabel(provider)} failed`, err);
  }
}

/** Open a workspace file in an editor pane/tab: Markdown files get the
 *  markdown editor unless `forceCode`. Same for the file tree, the fuzzy
 *  finder, the file viewer, Source Control and a restored layout. */
export function openFileInEditor(
  wsIdx: number,
  path: string,
  opts: OpenTarget & { forceCode?: boolean } = {},
) {
  const tabId = crypto.randomUUID();
  let tab: TabInfo;
  if (!opts.forceCode && isMarkdownPath(path)) {
    registerMarkdownFile(tabId, path);
    tab = { id: tabId, provider: "Markdown", alive: true };
  } else {
    registerCodeFile(tabId, path, wsIdx);
    tab = { id: tabId, provider: "CodeEditor", alive: true };
  }
  if (opts.paneId && wsIdx === appState.activeWorkspace) appState.setPaneContent(opts.paneId, tab);
  else appState.addTab(wsIdx, tab);
}

/** `$EDITOR` on a file in a shell pane/tab. */
export async function openFileInExternalEditor(wsIdx: number, path: string, target: OpenTarget = {}) {
  try {
    const tabId = await ipc.spawnEditorTab(wsIdx, path);
    const tab: TabInfo = { id: tabId, provider: "Shell", alive: true };
    if (target.paneId && wsIdx === appState.activeWorkspace) appState.setPaneContent(target.paneId, tab);
    else appState.addTab(wsIdx, tab);
  } catch (err) {
    reportError("Failed to open editor", err);
  }
}

// ── Layout restore ────────────────────────────────────

const pathOf = (id: string) => getCodeEditorFilePath(id) ?? getMarkdownEditorFilePath(id);

/** Wire the layout snapshot to the panels: what to save per content and how
 *  to bring it back. Call once from main.ts before `loadPaneTrees`. */
export function installContentRestorer() {
  const restorer: ContentRestorer = {
    describe(tab) {
      return describeContent(tab, pathOf, getWebPreviewUrl);
    },
    restoreFrontend(saved, wsIdx) {
      const tab: TabInfo = { id: saved.id, provider: saved.kind, alive: true, custom_title: saved.title ?? null };
      if (saved.kind === "CodeEditor") {
        if (!saved.path) return null;
        if (!getCodeEditorFilePath(saved.id)) registerCodeFile(saved.id, saved.path, wsIdx);
      } else if (saved.kind === "Markdown") {
        if (!saved.path) return null;
        if (!getMarkdownEditorFilePath(saved.id)) registerMarkdownFile(saved.id, saved.path);
      } else if (saved.kind === "WebPreview") {
        registerWebPreview(saved.id, saved.url ?? "");
      } else {
        return null;
      }
      return tab;
    },
    async respawn(saved, wsIdx) {
      return ipc.spawnTab(wsIdx, saved.kind);
    },
    async verify(saved, wsIdx) {
      if (!saved.path) return true;
      // A live panel (in-session re-hydration) is trusted as is — it may hold
      // unsaved edits; only a cold restore checks the file is still there.
      const live = saved.kind === "CodeEditor" ? hasCodeEditorInstance(saved.id) : hasMarkdownEditorInstance(saved.id);
      if (live) return true;
      try {
        await ipc.readFileContent(wsIdx, saved.path);
        return true;
      } catch {
        return false;
      }
    },
    onDropped(saved, _wsIdx, reason) {
      const what = saved.path ?? getProviderLabel(saved.kind);
      toast(`Could not restore ${what}: ${String(reason)} — the pane is blank`, "error");
    },
  };
  appState.setContentRestorer(restorer);
}
