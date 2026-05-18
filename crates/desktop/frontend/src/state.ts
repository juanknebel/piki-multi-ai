import type {
  AIProvider,
  WorkspaceInfo,
  WorkspaceDetail,
  ChangedFile,
  TabInfo,
  WorkspaceStatus,
  CliAgentStatus,
  PtyAgentEvent,
} from "./types";
import * as ipc from "./ipc";
import {
  type PaneNode,
  type PaneId,
  type SplitDir,
  newLeaf,
  findPane,
  findContentPane,
  allLeaves,
  splitPane as splitPaneTree,
  closePane as closePaneTree,
  setContent as setContentTree,
  removeContent as removeContentTree,
  setSplitRatio as setSplitRatioTree,
  deserialize as deserializePaneTree,
  reconcileWithContents,
  treeContentIds,
} from "./pane-tree";

export type SidebarView = "explorer" | "files" | "git" | "agents" | "kanban" | "api" | "web-preview";
// Note: "explorer"/"files"/"git" are real sidebar views; "agents" opens the
// modal dialog, "kanban"/"api"/"web-preview" open tabs

export interface UndoEntry {
  action: "stage" | "unstage";
  files: string[];
}

const MAX_UNDO = 20;
const WS_TABS_SETTINGS_KEY = "wsTabsV2";
const LAYOUT_SAVE_DEBOUNCE_MS = 200;

export type StateEvent =
  | "workspaces-changed"
  | "active-workspace-changed"
  | "files-changed"
  | "tabs-changed"
  | "active-tab-changed"
  | "sysinfo-changed"
  | "view-changed"
  | "pane-tree-changed"
  | "active-pane-changed"
  | "tab-shell-state-changed"
  | "workspace-attention-changed";

/** A top-level workspace tab. Each one owns its own pane tree; every pane
 *  (leaf) holds at most one content item from `WorkspaceState.tabs`. */
export interface WorkspaceTab {
  id: string;
  paneTree: PaneNode;
  activePaneId: PaneId;
}

interface WorkspaceState {
  info: WorkspaceInfo;
  status: WorkspaceStatus;
  changedFiles: ChangedFile[];
  aheadBehind: [number, number] | null;
  /** Flat list of content items (terminals/agents/editors) across all tabs.
   *  Mirrors the backend tab list order so ipc index calls stay consistent. */
  tabs: TabInfo[];
  /** Index into `tabs` of the content in the active tab's active pane. */
  activeTab: number;
  /** Top-level tabs, each with its own split layout. */
  wsTabs: WorkspaceTab[];
  activeWsTab: number;
  /** True when at least one tab in this workspace has fired a `pty-attention`
   *  event the user hasn't acknowledged. Cleared on `setActiveWorkspace`. */
  needsAttention: boolean;
}

/** Per-tab state derived from `pty-shell-event`s / `pty-agent-event`s.
 *  Keyed by content id. */
export interface TabShellState {
  cwd?: string;
  /** Exit code of the last command finished by this shell tab. `undefined`
   *  before the first command completes. */
  lastExitCode?: number;
  /** Structured Claude Code agent status (Claude tabs only). `undefined`
   *  until the first cli-agent event arrives. */
  agentStatus?: CliAgentStatus;
  /** Last human-relevant agent text: permission preview, or the agent's
   *  final response preview on `done`. */
  agentSummary?: string;
}

interface SavedWsTab {
  tree: unknown;
  activePaneId: string;
}
interface SavedWsLayout {
  tabs: SavedWsTab[];
  activeWsTab: number;
}

function genWsTabId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `wt-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function makeWsTab(contentId: string | null): WorkspaceTab {
  const leaf = newLeaf(contentId);
  return { id: genWsTabId(), paneTree: leaf, activePaneId: leaf.id };
}

class AppState extends EventTarget {
  private _workspaces: WorkspaceState[] = [];
  private _activeWorkspace = 0;
  private _sysinfo = "";
  private _activeView: SidebarView = "explorer";
  private _selectedFiles = new Set<string>();
  private _undoStack: UndoEntry[] = [];
  private _savedLayouts: Record<string, SavedWsLayout> = {};
  private _layoutLoaded = false;
  private _saveTimer: ReturnType<typeof setTimeout> | null = null;
  private _tabShellStates = new Map<string, TabShellState>();

  get workspaces(): readonly WorkspaceState[] {
    return this._workspaces;
  }

  get activeWorkspace(): number {
    return this._activeWorkspace;
  }

  get activeWs(): WorkspaceState | undefined {
    return this._workspaces[this._activeWorkspace];
  }

  get sysinfo(): string {
    return this._sysinfo;
  }

  get activeView(): SidebarView {
    return this._activeView;
  }

  setActiveView(view: SidebarView) {
    if (this._activeView === view) return;
    this._activeView = view;
    this.emit("view-changed");
  }

  get selectedFiles(): ReadonlySet<string> { return this._selectedFiles; }

  toggleFileSelection(path: string) {
    if (this._selectedFiles.has(path)) this._selectedFiles.delete(path);
    else this._selectedFiles.add(path);
    this.emit("files-changed");
  }

  clearSelection() { this._selectedFiles.clear(); }

  pushUndo(entry: UndoEntry) {
    this._undoStack.push(entry);
    if (this._undoStack.length > MAX_UNDO) this._undoStack.shift();
  }

  popUndo(): UndoEntry | undefined { return this._undoStack.pop(); }

  // ── Workspace lifecycle ────────────────────────────

  private _blankWs(info: WorkspaceInfo): WorkspaceState {
    return {
      info,
      status: "Idle" as WorkspaceStatus,
      changedFiles: [],
      aheadBehind: null,
      tabs: [],
      activeTab: 0,
      wsTabs: [],
      activeWsTab: 0,
      needsAttention: false,
    };
  }

  setWorkspaces(infos: WorkspaceInfo[]) {
    this._workspaces = infos.map((info) => this._blankWs(info));
    this.emit("workspaces-changed");
  }

  addWorkspace(info: WorkspaceInfo) {
    this._workspaces.push(this._blankWs(info));
    this.emit("workspaces-changed");
  }

  removeWorkspace(index: number) {
    this._workspaces.splice(index, 1);
    if (this._activeWorkspace >= this._workspaces.length) {
      this._activeWorkspace = Math.max(0, this._workspaces.length - 1);
    }
    this.emit("workspaces-changed");
    this.emit("active-workspace-changed");
  }

  setActiveWorkspace(index: number, detail?: WorkspaceDetail) {
    this._activeWorkspace = index;
    if (detail && this._workspaces[index]) {
      const ws = this._workspaces[index];
      ws.info = detail.info;
      ws.status = detail.status;
      ws.changedFiles = detail.changed_files;
      ws.aheadBehind = detail.ahead_behind;
      ws.tabs = detail.tabs;
      this._hydrateLayout(ws, detail.active_tab);
    }
    const ws = this._workspaces[index];
    if (ws?.needsAttention) {
      ws.needsAttention = false;
      this.emit("workspace-attention-changed");
    }
    this.emit("active-workspace-changed");
    this.emit("files-changed");
    this.emit("tabs-changed");
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
  }

  updateFiles(workspaceIdx: number, files: ChangedFile[], aheadBehind: [number, number] | null) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    ws.changedFiles = files;
    ws.aheadBehind = aheadBehind;
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("files-changed");
    }
  }

  // ── Tab / pane helpers ─────────────────────────────

  private _curWsTab(ws: WorkspaceState): WorkspaceTab | undefined {
    return ws.wsTabs[ws.activeWsTab];
  }

  /** Recompute `ws.activeTab` (index into `ws.tabs`) from the active pane's
   *  content of the active workspace tab. */
  private _syncActiveContent(ws: WorkspaceState) {
    const wt = this._curWsTab(ws);
    const pane = wt ? findPane(wt.paneTree, wt.activePaneId) : null;
    const cid = pane && pane.kind === "leaf" ? pane.contentId : null;
    const idx = cid ? ws.tabs.findIndex((t) => t.id === cid) : -1;
    ws.activeTab = idx >= 0 ? idx : 0;
  }

  /** The active workspace tab (for the renderer). */
  get activeTabTree(): WorkspaceTab | undefined {
    const ws = this.activeWs;
    return ws ? this._curWsTab(ws) : undefined;
  }

  // ── Tabs ───────────────────────────────────────────

  /** Open `tab` as a NEW top-level tab (single pane holding it). This is the
   *  semantics every file/agent/editor open flow expects. */
  addTab(workspaceIdx: number, tab: TabInfo) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    ws.tabs.push(tab);
    const wt = makeWsTab(tab.id);
    ws.wsTabs.push(wt);
    ws.activeWsTab = ws.wsTabs.length - 1;
    this._syncActiveContent(ws);
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
      this.emit("pane-tree-changed");
      this.emit("active-pane-changed");
    }
    this._scheduleSave();
  }

  /** Singletons (Kanban/API/Web Preview) also open as their own top-level
   *  tab; dedup is handled by `focusSingletonTab`. Kept as a named alias so
   *  call sites read intentionally. */
  addTabToRoot(workspaceIdx: number, tab: TabInfo) {
    this.addTab(workspaceIdx, tab);
  }

  /** Create a new EMPTY top-level tab (single blank pane → content chooser). */
  newBlankTab(workspaceIdx = this._activeWorkspace) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    const wt = makeWsTab(null);
    ws.wsTabs.push(wt);
    ws.activeWsTab = ws.wsTabs.length - 1;
    this._syncActiveContent(ws);
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
      this.emit("pane-tree-changed");
      this.emit("active-pane-changed");
    }
    this._scheduleSave();
  }

  /** Place a freshly-spawned content item into a specific (blank) pane of the
   *  active workspace tab — used by the in-pane content chooser. */
  setPaneContent(paneId: PaneId, tab: TabInfo) {
    const ws = this.activeWs;
    const wt = ws ? this._curWsTab(ws) : undefined;
    if (!ws || !wt) return;
    if (!ws.tabs.some((t) => t.id === tab.id)) ws.tabs.push(tab);
    wt.paneTree = setContentTree(wt.paneTree, paneId, tab.id);
    wt.activePaneId = paneId;
    this._syncActiveContent(ws);
    this.emit("tabs-changed");
    this.emit("active-tab-changed");
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._scheduleSave();
  }

  isSingletonProvider(provider: AIProvider): boolean {
    return provider === "Kanban" || provider === "Api" || provider === "WebPreview";
  }

  /** If a singleton tab for `provider` is already open in the active
   *  workspace, focus its tab/pane and return true. */
  focusSingletonTab(provider: AIProvider): boolean {
    if (!this.isSingletonProvider(provider)) return false;
    const ws = this.activeWs;
    if (!ws) return false;
    const idx = ws.tabs.findIndex((t) => t.provider === provider);
    if (idx < 0) return false;
    this.setActiveTab(idx);
    return true;
  }

  /** Focus the content at `tabIdx` (index into `ws.tabs`) by switching to the
   *  workspace tab + pane that holds it. */
  setActiveTab(tabIdx: number) {
    const ws = this.activeWs;
    if (!ws) return;
    const content = ws.tabs[tabIdx];
    if (content) {
      for (let i = 0; i < ws.wsTabs.length; i++) {
        const leaf = findContentPane(ws.wsTabs[i].paneTree, content.id);
        if (leaf) {
          ws.activeWsTab = i;
          ws.wsTabs[i].activePaneId = leaf.id;
          break;
        }
      }
    }
    ws.activeTab = tabIdx >= 0 ? tabIdx : 0;
    this.emit("active-tab-changed");
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._scheduleSave();
    ipc.setActiveTab(this._activeWorkspace, tabIdx).catch(() => {});
  }

  /** Switch the active top-level tab by index. */
  setActiveWsTab(wsTabIdx: number) {
    const ws = this.activeWs;
    if (!ws || wsTabIdx < 0 || wsTabIdx >= ws.wsTabs.length) return;
    ws.activeWsTab = wsTabIdx;
    this._syncActiveContent(ws);
    this.emit("active-tab-changed");
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._scheduleSave();
  }

  /** Remove the content item at `tabIdx`: blank its pane. If that leaves its
   *  whole workspace tab with no content, the tab is dropped too. */
  removeTab(workspaceIdx: number, tabIdx: number) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    const removed = ws.tabs[tabIdx];
    if (!removed) return;
    ws.tabs.splice(tabIdx, 1);
    for (const wt of ws.wsTabs) {
      if (findContentPane(wt.paneTree, removed.id)) {
        wt.paneTree = removeContentTree(wt.paneTree, removed.id);
        break;
      }
    }
    this._dropEmptyWsTabs(ws);
    this._clampActiveWsTab(ws);
    this._syncActiveContent(ws);
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
      this.emit("pane-tree-changed");
      this.emit("active-pane-changed");
    }
    this._scheduleSave();
  }

  /** Close a whole top-level tab. Returns the content ids it held so the
   *  caller can tear down their PTYs / panels. */
  closeWsTab(workspaceIdx: number, wsTabIdx: number): string[] {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return [];
    const wt = ws.wsTabs[wsTabIdx];
    if (!wt) return [];
    const ids = treeContentIds(wt.paneTree);
    ws.wsTabs.splice(wsTabIdx, 1);
    ws.tabs = ws.tabs.filter((t) => !ids.includes(t.id));
    this._clampActiveWsTab(ws);
    this._syncActiveContent(ws);
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
      this.emit("pane-tree-changed");
      this.emit("active-pane-changed");
    }
    this._scheduleSave();
    return ids;
  }

  private _dropEmptyWsTabs(ws: WorkspaceState) {
    ws.wsTabs = ws.wsTabs.filter((wt) => treeContentIds(wt.paneTree).length > 0);
  }

  private _clampActiveWsTab(ws: WorkspaceState) {
    if (ws.activeWsTab >= ws.wsTabs.length) {
      ws.activeWsTab = Math.max(0, ws.wsTabs.length - 1);
    }
  }

  markTabDead(tabId: string) {
    for (const ws of this._workspaces) {
      const tab = ws.tabs.find((t) => t.id === tabId);
      if (tab) {
        tab.alive = false;
        break;
      }
    }
  }

  setSysinfo(formatted: string) {
    this._sysinfo = formatted;
    this.emit("sysinfo-changed");
  }

  // ── Shell integration & attention ─────────────────

  getTabShellState(tabId: string): TabShellState | undefined {
    return this._tabShellStates.get(tabId);
  }

  applyShellEvent(event: { tab_id: string; kind: string; exit_code?: number; cwd?: string }) {
    const existing = this._tabShellStates.get(event.tab_id) ?? {};
    let next: TabShellState = existing;
    if (event.kind === "cwd-changed" && event.cwd) {
      next = { ...existing, cwd: event.cwd };
    } else if (event.kind === "command-end") {
      next = { ...existing, lastExitCode: event.exit_code ?? undefined };
    } else {
      return;
    }
    this._tabShellStates.set(event.tab_id, next);
    this.emit("tab-shell-state-changed");
  }

  applyAgentEvent(event: PtyAgentEvent) {
    const existing = this._tabShellStates.get(event.tab_id) ?? {};
    const next: TabShellState = { ...existing, agentStatus: event.status };
    // Keep the last meaningful summary; transient events (running) carry
    // none and shouldn't wipe a permission/done message already shown.
    if (event.summary) next.agentSummary = event.summary;
    this._tabShellStates.set(event.tab_id, next);
    this.emit("tab-shell-state-changed");
  }

  markWorkspaceAttention(workspaceIdx: number) {
    if (workspaceIdx === this._activeWorkspace) return;
    const ws = this._workspaces[workspaceIdx];
    if (!ws || ws.needsAttention) return;
    ws.needsAttention = true;
    this.emit("workspace-attention-changed");
  }

  workspaceIndexForTab(tabId: string): number {
    for (let i = 0; i < this._workspaces.length; i++) {
      if (this._workspaces[i].tabs.some((t) => t.id === tabId)) return i;
    }
    return -1;
  }

  // ── Pane operations (act on the active workspace tab) ──

  get activePaneId(): PaneId | null {
    return this.activeTabTree?.activePaneId ?? null;
  }

  setActivePane(paneId: PaneId) {
    const ws = this.activeWs;
    const wt = ws ? this._curWsTab(ws) : undefined;
    if (!ws || !wt || wt.activePaneId === paneId) return;
    if (!findPane(wt.paneTree, paneId)) return;
    wt.activePaneId = paneId;
    this._syncActiveContent(ws);
    this.emit("active-pane-changed");
    this.emit("active-tab-changed");
    this._scheduleSave();
  }

  splitActivePane(dir: SplitDir): PaneId | null {
    const ws = this.activeWs;
    const wt = ws ? this._curWsTab(ws) : undefined;
    if (!ws || !wt) return null;
    return this.splitPane(wt.activePaneId, dir);
  }

  /** Split `paneId` in the active tab; the new pane is BLANK (chooser). */
  splitPane(paneId: PaneId, dir: SplitDir): PaneId | null {
    const ws = this.activeWs;
    const wt = ws ? this._curWsTab(ws) : undefined;
    if (!ws || !wt) return null;
    const { root, newPaneId } = splitPaneTree(wt.paneTree, paneId, dir);
    if (root === wt.paneTree) return null;
    wt.paneTree = root;
    wt.activePaneId = newPaneId;
    this._syncActiveContent(ws);
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this.emit("active-tab-changed");
    this._scheduleSave();
    return newPaneId;
  }

  closePane(paneId: PaneId) {
    const ws = this.activeWs;
    const wt = ws ? this._curWsTab(ws) : undefined;
    if (!ws || !wt) return;
    const result = closePaneTree(wt.paneTree, paneId);
    if (result.root === null) {
      // Closing the only pane closes the whole top-level tab.
      this.closeWsTab(this._activeWorkspace, ws.activeWsTab);
      return;
    }
    if (result.root === wt.paneTree) return;
    wt.paneTree = result.root;
    if (result.promotedPaneId && findPane(result.root, result.promotedPaneId)) {
      wt.activePaneId = result.promotedPaneId;
    } else {
      wt.activePaneId = allLeaves(result.root)[0]?.id ?? result.root.id;
    }
    this._syncActiveContent(ws);
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this.emit("active-tab-changed");
    this._scheduleSave();
  }

  setSplitRatio(splitId: PaneId, ratio: number) {
    const ws = this.activeWs;
    const wt = ws ? this._curWsTab(ws) : undefined;
    if (!ws || !wt) return;
    const next = setSplitRatioTree(wt.paneTree, splitId, ratio);
    if (next === wt.paneTree) return;
    wt.paneTree = next;
    this.emit("pane-tree-changed");
    this._scheduleSave();
  }

  // ── Persistence ────────────────────────────────────

  async loadPaneTrees(): Promise<void> {
    try {
      const raw = await ipc.getSettings();
      const all = raw ? JSON.parse(raw) : {};
      const saved = all && typeof all === "object" ? all[WS_TABS_SETTINGS_KEY] : null;
      this._savedLayouts = saved && typeof saved === "object" ? saved : {};
    } catch {
      this._savedLayouts = {};
    }
    this._layoutLoaded = true;
    for (const ws of this._workspaces) {
      if (ws.tabs.length === 0 && ws.wsTabs.length === 0) continue;
      this._hydrateLayout(ws, ws.activeTab);
    }
    this.emit("pane-tree-changed");
    this.emit("tabs-changed");
  }

  /** Build `ws.wsTabs` from the saved layout, or fall back to one tab per
   *  content. Orphan contents (not in any saved tab) are appended as tabs. */
  private _hydrateLayout(ws: WorkspaceState, preferredActiveContent: number) {
    const knownIds = new Set(ws.tabs.map((t) => t.id));

    const fallback = () => {
      ws.wsTabs = ws.tabs.map((t) => makeWsTab(t.id));
      ws.activeWsTab = Math.min(
        Math.max(0, preferredActiveContent),
        Math.max(0, ws.wsTabs.length - 1),
      );
      this._syncActiveContent(ws);
    };

    if (!this._layoutLoaded) return fallback();
    const entry = this._savedLayouts[String(ws.info.path)];
    if (!entry || !Array.isArray(entry.tabs)) return fallback();

    const rebuilt: WorkspaceTab[] = [];
    const placed = new Set<string>();
    for (const st of entry.tabs) {
      const tree = deserializePaneTree(st?.tree);
      if (!tree) continue;
      const reconciled = reconcileWithContents(tree, knownIds);
      const ids = treeContentIds(reconciled);
      // Keep a tab if it still holds content; drop persisted all-blank tabs.
      if (ids.length === 0) continue;
      for (const id of ids) placed.add(id);
      const activePaneId = findPane(reconciled, st.activePaneId)
        ? st.activePaneId
        : (allLeaves(reconciled)[0]?.id ?? reconciled.id);
      rebuilt.push({ id: genWsTabId(), paneTree: reconciled, activePaneId });
    }
    // Any content not represented in a saved tab gets its own tab.
    for (const t of ws.tabs) {
      if (!placed.has(t.id)) rebuilt.push(makeWsTab(t.id));
    }
    if (rebuilt.length === 0) return fallback();
    ws.wsTabs = rebuilt;
    ws.activeWsTab = Math.min(
      Math.max(0, entry.activeWsTab ?? 0),
      rebuilt.length - 1,
    );
    this._syncActiveContent(ws);
  }

  private _scheduleSave() {
    if (!this._layoutLoaded) return;
    if (this._saveTimer) clearTimeout(this._saveTimer);
    this._saveTimer = setTimeout(() => {
      this._saveTimer = null;
      this._flushSave().catch(() => {});
    }, LAYOUT_SAVE_DEBOUNCE_MS);
  }

  private async _flushSave(): Promise<void> {
    const snapshot: Record<string, SavedWsLayout> = {};
    for (const ws of this._workspaces) {
      snapshot[String(ws.info.path)] = {
        tabs: ws.wsTabs.map((wt) => ({
          tree: wt.paneTree,
          activePaneId: wt.activePaneId,
        })),
        activeWsTab: ws.activeWsTab,
      };
    }
    this._savedLayouts = snapshot;
    try {
      const raw = await ipc.getSettings();
      const all = raw ? JSON.parse(raw) : {};
      all[WS_TABS_SETTINGS_KEY] = snapshot;
      await ipc.setSettings(JSON.stringify(all));
    } catch {
      // Best-effort; failure to persist is non-fatal.
    }
  }

  on(event: StateEvent, callback: () => void): () => void {
    this.addEventListener(event, callback);
    return () => this.removeEventListener(event, callback);
  }

  private emit(event: StateEvent) {
    this.dispatchEvent(new Event(event));
  }
}

export const appState = new AppState();
