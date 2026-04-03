import { appState } from "../state";
import * as ipc from "../ipc";
import { renderWorkspaceList } from "./workspace-list";
import { renderSourceControl } from "./source-control";
import { renderAgentsPanel } from "./agents-panel";

export function initSidebar() {
  const explorerView = document.getElementById("explorer-view")!;
  const workspaceList = document.getElementById("workspace-list")!;
  const scView = document.getElementById("source-control-view")!;
  const agentsView = document.getElementById("agents-view")!;

  renderWorkspaceList(workspaceList);
  renderSourceControl(scView);
  renderAgentsPanel(agentsView);

  // Track last non-kanban view so we can restore sidebar when switching away
  let lastSidebarView: "explorer" | "git" | "agents" = "explorer";

  function updateView() {
    const view = appState.activeView;

    if (view === "kanban") {
      // Kanban opens as a tab, not a sidebar panel — spawn a tab and restore sidebar
      spawnKanbanTab();
      // Restore previous sidebar view
      appState.setActiveView(lastSidebarView);
      return;
    }

    lastSidebarView = view;
    explorerView.style.display = view === "explorer" ? "flex" : "none";
    scView.style.display = view === "git" ? "flex" : "none";
    agentsView.style.display = view === "agents" ? "flex" : "none";
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
