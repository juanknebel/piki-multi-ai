import { appState } from "../state";
import * as ipc from "../ipc";
import { renderWorkspaceList } from "./workspace-list";
import { renderSourceControl } from "./source-control";
import { showAgentManager } from "./dialogs/agent-dialog";

export function initSidebar() {
  const explorerView = document.getElementById("explorer-view")!;
  const workspaceList = document.getElementById("workspace-list")!;
  const scView = document.getElementById("source-control-view")!;

  renderWorkspaceList(workspaceList);
  renderSourceControl(scView);

  // Track last sidebar view so we can restore when a non-sidebar action triggers
  let lastSidebarView: "explorer" | "git" = "explorer";

  function updateView() {
    const view = appState.activeView;

    if (view === "kanban") {
      spawnKanbanTab();
      appState.setActiveView(lastSidebarView);
      return;
    }

    if (view === "api") {
      spawnApiTab();
      appState.setActiveView(lastSidebarView);
      return;
    }

    if (view === "agents") {
      showAgentManager();
      appState.setActiveView(lastSidebarView);
      return;
    }

    lastSidebarView = view;
    explorerView.style.display = view === "explorer" ? "flex" : "none";
    scView.style.display = view === "git" ? "flex" : "none";
  }

  async function spawnKanbanTab() {
    // Check if there's already a Kanban tab open in the active workspace
    const ws = appState.activeWs;
    if (ws) {
      const existingIdx = ws.tabs.findIndex((t) => t.provider === "Kanban");
      if (existingIdx >= 0) {
        appState.setActiveTab(existingIdx);
        return;
      }
    }
    try {
      const tabId = await ipc.spawnTab(appState.activeWorkspace, "Kanban");
      appState.addTab(appState.activeWorkspace, { id: tabId, provider: "Kanban", alive: true });
    } catch (err) {
      console.error("Failed to open Kanban tab:", err);
    }
  }

  async function spawnApiTab() {
    const ws = appState.activeWs;
    if (ws) {
      const existingIdx = ws.tabs.findIndex((t) => t.provider === "Api");
      if (existingIdx >= 0) {
        appState.setActiveTab(existingIdx);
        return;
      }
    }
    try {
      const tabId = await ipc.spawnTab(appState.activeWorkspace, "Api");
      appState.addTab(appState.activeWorkspace, { id: tabId, provider: "Api", alive: true });
    } catch (err) {
      console.error("Failed to open API Explorer tab:", err);
    }
  }

  appState.on("view-changed", updateView);
  updateView();

  // Vertical sidebar resize
  const handle = document.getElementById("sidebar-resize-v")!;
  const root = document.documentElement;
  let dragging = false;
  let startX = 0;
  let startWidth = 0;

  handle.addEventListener("mousedown", (e) => {
    dragging = true;
    startX = e.clientX;
    startWidth = document.getElementById("sidebar")!.offsetWidth;
    handle.classList.add("dragging");
    document.body.style.cursor = "ew-resize";
    document.body.style.userSelect = "none";
    e.preventDefault();
  });

  document.addEventListener("mousemove", (e) => {
    if (!dragging) return;
    const delta = e.clientX - startX;
    const newWidth = Math.max(150, Math.min(window.innerWidth * 0.5, startWidth + delta));
    root.style.setProperty("--sidebar-width", `${newWidth}px`);
  });

  document.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false;
    handle.classList.remove("dragging");
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  });
}
