import type {
  WorkspaceInfo,
  WorkspaceDetail,
  ChangedFile,
  TabInfo,
  WorkspaceStatus,
} from "./types";
import * as ipc from "./ipc";
import {
  type PaneNode,
  type PaneId,
  type SplitDir,
  newLeaf,
  findPane,
  findTabPane,
  allLeaves,
  splitPane as splitPaneTree,
  closePane as closePaneTree,
  removeTab as removeTabFromTree,
  moveTabToPane as moveTabBetweenPanes,
  addTabToPane,
  setActiveTabInPane as setActiveTabInPaneTree,
  setSplitRatio as setSplitRatioTree,
  deserialize as deserializePaneTree,
  reconcileWithTabs,
} from "./pane-tree";

export type SidebarView = "explorer" | "git" | "agents" | "kanban" | "api";
// Note: "agents" opens the modal dialog, "kanban"/"api" open tabs — none are real sidebar views

export interface UndoEntry {
  action: "stage" | "unstage";
  files: string[];
}

const MAX_UNDO = 20;
const PANE_TREES_SETTINGS_KEY = "paneTrees";
const PANE_TREE_SAVE_DEBOUNCE_MS = 200;

export type StateEvent =
  | "workspaces-changed"
  | "active-workspace-changed"
  | "files-changed"
  | "tabs-changed"
  | "active-tab-changed"
  | "sysinfo-changed"
  | "view-changed"
  | "pane-tree-changed"
  | "active-pane-changed";

interface WorkspaceState {
  info: WorkspaceInfo;
  status: WorkspaceStatus;
  changedFiles: ChangedFile[];
  aheadBehind: [number, number] | null;
  tabs: TabInfo[];
  activeTab: number;
  paneTree: PaneNode;
  activePaneId: PaneId;
}

interface SavedPaneTreeEntry {
  tree: unknown;
  activePaneId: string;
}

class AppState extends EventTarget {
  private _workspaces: WorkspaceState[] = [];
  private _activeWorkspace = 0;
  private _sysinfo = "";
  private _activeView: SidebarView = "explorer";
  private _selectedFiles = new Set<string>();
  private _undoStack: UndoEntry[] = [];
  private _savedPaneTrees: Record<string, SavedPaneTreeEntry> = {};
  private _paneTreesLoaded = false;
  private _paneSaveTimer: ReturnType<typeof setTimeout> | null = null;

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

  setWorkspaces(infos: WorkspaceInfo[]) {
    this._workspaces = infos.map((info) => {
      const root = newLeaf([]);
      return {
        info,
        status: "Idle" as WorkspaceStatus,
        changedFiles: [] as ChangedFile[],
        aheadBehind: null,
        tabs: [] as TabInfo[],
        activeTab: 0,
        paneTree: root,
        activePaneId: root.id,
      };
    });
    this.emit("workspaces-changed");
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
      ws.activeTab = detail.active_tab;
      const restored = this._hydratePaneTree(ws);
      ws.paneTree = restored.tree;
      ws.activePaneId = restored.activePaneId;
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

  addTab(workspaceIdx: number, tab: TabInfo) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    ws.tabs.push(tab);
    ws.activeTab = ws.tabs.length - 1;
    // Add to the active pane (or fall back to the root leaf if active is stale).
    const targetPaneId = findPane(ws.paneTree, ws.activePaneId)
      ? ws.activePaneId
      : ws.paneTree.id;
    ws.paneTree = addTabToPane(ws.paneTree, targetPaneId, tab.id);
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
      this.emit("pane-tree-changed");
    }
    this._schedulePaneSave();
  }

  setActiveTab(tabIdx: number) {
    const ws = this.activeWs;
    if (!ws) return;
    ws.activeTab = tabIdx;
    const tab = ws.tabs[tabIdx];
    if (tab) {
      const owner = findTabPane(ws.paneTree, tab.id);
      if (owner) {
        ws.paneTree = setActiveTabInPaneTree(ws.paneTree, owner.id, tab.id);
        if (ws.activePaneId !== owner.id) {
          ws.activePaneId = owner.id;
          this.emit("active-pane-changed");
        }
      }
    }
    this.emit("active-tab-changed");
    this.emit("pane-tree-changed");
    this._schedulePaneSave();
    // Sync to backend so switching workspaces preserves the active tab
    ipc.setActiveTab(this._activeWorkspace, tabIdx).catch(() => {});
  }

  removeTab(workspaceIdx: number, tabIdx: number) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    const removed = ws.tabs[tabIdx];
    ws.tabs.splice(tabIdx, 1);
    if (ws.activeTab >= ws.tabs.length) {
      ws.activeTab = Math.max(0, ws.tabs.length - 1);
    }
    if (removed) {
      const result = removeTabFromTree(ws.paneTree, removed.id);
      if (result.root) {
        ws.paneTree = result.root;
      } else {
        // Root collapsed entirely → create an empty root leaf.
        const fresh = newLeaf([]);
        ws.paneTree = fresh;
        ws.activePaneId = fresh.id;
      }
      // If the leaf that owned the active pane was collapsed, promote a sibling.
      if (!findPane(ws.paneTree, ws.activePaneId)) {
        const leaves = allLeaves(ws.paneTree);
        ws.activePaneId = leaves[0]?.id ?? ws.paneTree.id;
        if (workspaceIdx === this._activeWorkspace) {
          this.emit("active-pane-changed");
        }
      }
    }
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
      this.emit("pane-tree-changed");
    }
    this._schedulePaneSave();
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

  addWorkspace(info: WorkspaceInfo) {
    const root = newLeaf([]);
    this._workspaces.push({
      info,
      status: "Idle",
      changedFiles: [],
      aheadBehind: null,
      tabs: [],
      activeTab: 0,
      paneTree: root,
      activePaneId: root.id,
    });
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

  // ── Pane tree operations ───────────────────────────

  get activePaneId(): PaneId | null {
    return this.activeWs?.activePaneId ?? null;
  }

  setActivePane(paneId: PaneId) {
    const ws = this.activeWs;
    if (!ws || ws.activePaneId === paneId) return;
    if (!findPane(ws.paneTree, paneId)) return;
    ws.activePaneId = paneId;
    // Sync ws.activeTab to the active pane's active tab (best-effort).
    const pane = findPane(ws.paneTree, paneId);
    if (pane && pane.kind === "leaf" && pane.activeTabId) {
      const idx = ws.tabs.findIndex((t) => t.id === pane.activeTabId);
      if (idx >= 0 && idx !== ws.activeTab) {
        ws.activeTab = idx;
        ipc.setActiveTab(this._activeWorkspace, idx).catch(() => {});
        this.emit("active-tab-changed");
      }
    }
    this.emit("active-pane-changed");
    this._schedulePaneSave();
  }

  setActiveTabInPane(paneId: PaneId, tabId: string) {
    const ws = this.activeWs;
    if (!ws) return;
    ws.paneTree = setActiveTabInPaneTree(ws.paneTree, paneId, tabId);
    if (ws.activePaneId !== paneId) {
      ws.activePaneId = paneId;
      this.emit("active-pane-changed");
    }
    const idx = ws.tabs.findIndex((t) => t.id === tabId);
    if (idx >= 0 && idx !== ws.activeTab) {
      ws.activeTab = idx;
      ipc.setActiveTab(this._activeWorkspace, idx).catch(() => {});
    }
    this.emit("active-tab-changed");
    this.emit("pane-tree-changed");
    this._schedulePaneSave();
  }

  splitActivePane(dir: SplitDir, withTabId?: string): PaneId | null {
    const ws = this.activeWs;
    if (!ws) return null;
    const tabId = withTabId
      ?? (findPane(ws.paneTree, ws.activePaneId) as { activeTabId?: string } | null)?.activeTabId
      ?? undefined;
    const { root, newPaneId } = splitPaneTree(ws.paneTree, ws.activePaneId, dir, tabId);
    if (root === ws.paneTree) return null;
    ws.paneTree = root;
    ws.activePaneId = newPaneId;
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._schedulePaneSave();
    return newPaneId;
  }

  splitPane(paneId: PaneId, dir: SplitDir, withTabId?: string): PaneId | null {
    const ws = this.activeWs;
    if (!ws) return null;
    const { root, newPaneId } = splitPaneTree(ws.paneTree, paneId, dir, withTabId);
    if (root === ws.paneTree) return null;
    ws.paneTree = root;
    ws.activePaneId = newPaneId;
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._schedulePaneSave();
    return newPaneId;
  }

  closePane(paneId: PaneId) {
    const ws = this.activeWs;
    if (!ws) return;
    const result = closePaneTree(ws.paneTree, paneId);
    if (result.root === null) {
      // The root was closed — recreate an empty root leaf.
      const fresh = newLeaf([]);
      ws.paneTree = fresh;
      ws.activePaneId = fresh.id;
    } else if (result.root !== ws.paneTree) {
      ws.paneTree = result.root;
      if (result.promotedPaneId && findPane(result.root, result.promotedPaneId)) {
        ws.activePaneId = result.promotedPaneId;
      } else {
        ws.activePaneId = allLeaves(result.root)[0]?.id ?? result.root.id;
      }
    } else {
      return;
    }
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._schedulePaneSave();
  }

  moveTabToPane(tabId: string, dstPaneId: PaneId) {
    const ws = this.activeWs;
    if (!ws) return;
    const next = moveTabBetweenPanes(ws.paneTree, tabId, dstPaneId);
    if (next === ws.paneTree) return;
    ws.paneTree = next;
    if (findPane(ws.paneTree, dstPaneId)) {
      ws.activePaneId = dstPaneId;
    } else {
      ws.activePaneId = allLeaves(ws.paneTree)[0]?.id ?? ws.paneTree.id;
    }
    const idx = ws.tabs.findIndex((t) => t.id === tabId);
    if (idx >= 0) {
      ws.activeTab = idx;
      ipc.setActiveTab(this._activeWorkspace, idx).catch(() => {});
      this.emit("active-tab-changed");
    }
    this.emit("pane-tree-changed");
    this.emit("active-pane-changed");
    this._schedulePaneSave();
  }

  setSplitRatio(splitId: PaneId, ratio: number) {
    const ws = this.activeWs;
    if (!ws) return;
    const next = setSplitRatioTree(ws.paneTree, splitId, ratio);
    if (next === ws.paneTree) return;
    ws.paneTree = next;
    this.emit("pane-tree-changed");
    this._schedulePaneSave();
  }

  // ── Persistence ────────────────────────────────────

  async loadPaneTrees(): Promise<void> {
    try {
      const raw = await ipc.getSettings();
      const all = raw ? JSON.parse(raw) : {};
      const saved = all && typeof all === "object" ? all[PANE_TREES_SETTINGS_KEY] : null;
      this._savedPaneTrees = saved && typeof saved === "object" ? saved : {};
    } catch {
      this._savedPaneTrees = {};
    }
    this._paneTreesLoaded = true;
    // If any workspaces were already populated, retroactively hydrate them.
    for (const ws of this._workspaces) {
      if (ws.tabs.length === 0) continue;
      const restored = this._hydratePaneTree(ws);
      ws.paneTree = restored.tree;
      ws.activePaneId = restored.activePaneId;
    }
    this.emit("pane-tree-changed");
  }

  private _hydratePaneTree(ws: WorkspaceState): { tree: PaneNode; activePaneId: PaneId } {
    const fallback = (): { tree: PaneNode; activePaneId: PaneId } => {
      const activeTab = ws.tabs[ws.activeTab];
      const leaf = newLeaf(ws.tabs.map((t) => t.id), activeTab?.id ?? null);
      return { tree: leaf, activePaneId: leaf.id };
    };
    if (!this._paneTreesLoaded) return fallback();
    const entry = this._savedPaneTrees[ws.info.path];
    if (!entry) return fallback();
    const tree = deserializePaneTree(entry.tree);
    if (!tree) return fallback();
    const reconciled = reconcileWithTabs(tree, new Set(ws.tabs.map((t) => t.id)));
    if (!reconciled) return fallback();
    // Add any tabs not present in the saved tree to the first leaf.
    const knownInTree = new Set<string>();
    for (const leaf of allLeaves(reconciled)) {
      for (const t of leaf.tabIds) knownInTree.add(t);
    }
    let final: PaneNode = reconciled;
    const firstLeaf = allLeaves(final)[0];
    if (firstLeaf) {
      for (const tab of ws.tabs) {
        if (!knownInTree.has(tab.id)) {
          final = addTabToPane(final, firstLeaf.id, tab.id);
        }
      }
    }
    const activePaneId = findPane(final, entry.activePaneId)
      ? entry.activePaneId
      : (allLeaves(final)[0]?.id ?? final.id);
    return { tree: final, activePaneId };
  }

  private _schedulePaneSave() {
    if (!this._paneTreesLoaded) return;
    if (this._paneSaveTimer) clearTimeout(this._paneSaveTimer);
    this._paneSaveTimer = setTimeout(() => {
      this._paneSaveTimer = null;
      this._flushPaneSave().catch(() => {});
    }, PANE_TREE_SAVE_DEBOUNCE_MS);
  }

  private async _flushPaneSave(): Promise<void> {
    const snapshot: Record<string, SavedPaneTreeEntry> = {};
    for (const ws of this._workspaces) {
      snapshot[ws.info.path] = {
        tree: ws.paneTree,
        activePaneId: ws.activePaneId,
      };
    }
    this._savedPaneTrees = snapshot;
    try {
      const raw = await ipc.getSettings();
      const all = raw ? JSON.parse(raw) : {};
      all[PANE_TREES_SETTINGS_KEY] = snapshot;
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
