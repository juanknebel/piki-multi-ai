import type {
  WorkspaceInfo,
  WorkspaceDetail,
  ChangedFile,
  TabInfo,
  WorkspaceStatus,
} from "./types";

export type SidebarView = "explorer" | "git";

export type StateEvent =
  | "workspaces-changed"
  | "active-workspace-changed"
  | "files-changed"
  | "tabs-changed"
  | "active-tab-changed"
  | "sysinfo-changed"
  | "view-changed";

interface WorkspaceState {
  info: WorkspaceInfo;
  status: WorkspaceStatus;
  changedFiles: ChangedFile[];
  aheadBehind: [number, number] | null;
  tabs: TabInfo[];
  activeTab: number;
}

class AppState extends EventTarget {
  private _workspaces: WorkspaceState[] = [];
  private _activeWorkspace = 0;
  private _sysinfo = "";
  private _activeView: SidebarView = "explorer";

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

  setWorkspaces(infos: WorkspaceInfo[]) {
    this._workspaces = infos.map((info) => ({
      info,
      status: "Idle",
      changedFiles: [],
      aheadBehind: null,
      tabs: [],
      activeTab: 0,
    }));
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
    }
    this.emit("active-workspace-changed");
    this.emit("files-changed");
    this.emit("tabs-changed");
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
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
    }
  }

  setActiveTab(tabIdx: number) {
    const ws = this.activeWs;
    if (!ws) return;
    ws.activeTab = tabIdx;
    this.emit("active-tab-changed");
  }

  removeTab(workspaceIdx: number, tabIdx: number) {
    const ws = this._workspaces[workspaceIdx];
    if (!ws) return;
    ws.tabs.splice(tabIdx, 1);
    if (ws.activeTab >= ws.tabs.length) {
      ws.activeTab = Math.max(0, ws.tabs.length - 1);
    }
    if (workspaceIdx === this._activeWorkspace) {
      this.emit("tabs-changed");
      this.emit("active-tab-changed");
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

  addWorkspace(info: WorkspaceInfo) {
    this._workspaces.push({
      info,
      status: "Idle",
      changedFiles: [],
      aheadBehind: null,
      tabs: [],
      activeTab: 0,
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

  on(event: StateEvent, callback: () => void): () => void {
    this.addEventListener(event, callback);
    return () => this.removeEventListener(event, callback);
  }

  private emit(event: StateEvent) {
    this.dispatchEvent(new Event(event));
  }
}

export const appState = new AppState();
