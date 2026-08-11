import { appState } from "../state";
import { reportError } from "./toast";
import * as ipc from "../ipc";
import { renderWorkspaceList } from "./workspace-list";
import { renderFileTree } from "./file-tree";
import { renderSourceControl } from "./source-control";
import { renderAgentsPanel } from "./agents-panel";
import { showAgentManager } from "./dialogs/agent-dialog";
import { openWebPreviewTab } from "./web-preview-panel";

/** Set an explicit Agents-panel height (px), replacing the default 40% cap.
 *  Clamped so neither the panel nor the workspace list can be squeezed out. */
function applyAgentsPanelHeight(px: number) {
  const view = document.getElementById("agents-view");
  const explorer = document.getElementById("explorer-view");
  if (!view || !explorer) return;
  const max = Math.max(64, (explorer.clientHeight || window.innerHeight) * 0.75);
  const clamped = Math.max(32, Math.min(max, px));
  view.style.height = `${clamped}px`;
  view.style.maxHeight = "none";
}

export async function initSidebar() {
  // Restore persisted sidebar width + Agents panel height
  try {
    const raw = await ipc.getSettings();
    if (raw) {
      const settings = JSON.parse(raw);
      if (settings.sidebarWidth) {
        document.documentElement.style.setProperty("--sidebar-width", `${settings.sidebarWidth}px`);
      }
      if (settings.agentsPanelHeight) {
        applyAgentsPanelHeight(settings.agentsPanelHeight);
      }
    }
  } catch { /* ignore */ }

  const explorerView = document.getElementById("explorer-view")!;
  const workspaceList = document.getElementById("workspace-list")!;
  const filesView = document.getElementById("files-view")!;
  const scView = document.getElementById("source-control-view")!;
  const agentsView = document.getElementById("agents-view")!;

  renderWorkspaceList(workspaceList);
  renderFileTree(filesView);
  renderSourceControl(scView);
  // Always-visible Agents panel docked below the workspace list (same
  // layout as the TUI's bottom-left pane) — not a switchable view.
  renderAgentsPanel(agentsView);

  // Track last sidebar view so we can restore when a non-sidebar action triggers
  let lastSidebarView: "explorer" | "files" | "git" = "explorer";

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

    if (view === "web-preview") {
      openWebPreviewTab();
      appState.setActiveView(lastSidebarView);
      return;
    }

    if (view === "agents") {
      // The live panel lives in Explorer; the activity icon opens the
      // profile manager dialog, like before.
      showAgentManager();
      appState.setActiveView(lastSidebarView);
      return;
    }

    lastSidebarView = view;
    explorerView.style.display = view === "explorer" ? "flex" : "none";
    filesView.style.display = view === "files" ? "flex" : "none";
    scView.style.display = view === "git" ? "flex" : "none";
  }

  async function spawnKanbanTab() {
    if (appState.focusSingletonTab("Kanban")) return;
    try {
      const tabId = await ipc.spawnTab(appState.activeWorkspace, "Kanban");
      appState.addTabToRoot(appState.activeWorkspace, { id: tabId, provider: "Kanban", alive: true });
    } catch (err) {
      reportError("Failed to open Kanban tab", err);
    }
  }

  async function spawnApiTab() {
    if (appState.focusSingletonTab("Api")) return;
    try {
      const tabId = await ipc.spawnTab(appState.activeWorkspace, "Api");
      appState.addTabToRoot(appState.activeWorkspace, { id: tabId, provider: "Api", alive: true });
    } catch (err) {
      reportError("Failed to open API Explorer tab", err);
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

  function persistSidebarWidth() {
    const width = parseInt(getComputedStyle(root).getPropertyValue("--sidebar-width"));
    if (width) {
      ipc.getSettings().then((raw) => {
        const settings = raw ? JSON.parse(raw) : {};
        settings.sidebarWidth = width;
        ipc.setSettings(JSON.stringify(settings)).catch(() => {});
      }).catch(() => {});
    }
  }

  document.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false;
    handle.classList.remove("dragging");
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    persistSidebarWidth();
  });

  // Keyboard resizing on the divider itself.
  handle.tabIndex = 0;
  handle.setAttribute("role", "separator");
  handle.setAttribute("aria-orientation", "vertical");
  handle.setAttribute("aria-label", "Resize sidebar");
  handle.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const cur = document.getElementById("sidebar")!.offsetWidth;
    const next = Math.max(
      150,
      Math.min(window.innerWidth * 0.5, cur + (e.key === "ArrowRight" ? 16 : -16)),
    );
    root.style.setProperty("--sidebar-width", `${next}px`);
    persistSidebarWidth();
  });

  // Horizontal resize of the docked Agents panel (drag the divider above it).
  const agentsHandle = document.getElementById("agents-resize-h")!;
  let hDragging = false;
  let startY = 0;
  let startHeight = 0;

  agentsHandle.addEventListener("mousedown", (e) => {
    hDragging = true;
    startY = e.clientY;
    startHeight = agentsView.offsetHeight;
    agentsHandle.classList.add("dragging");
    document.body.style.cursor = "ns-resize";
    document.body.style.userSelect = "none";
    e.preventDefault();
  });

  document.addEventListener("mousemove", (e) => {
    if (!hDragging) return;
    // The handle sits above the panel: dragging up grows it.
    applyAgentsPanelHeight(startHeight + (startY - e.clientY));
  });

  function persistAgentsHeight() {
    const height = agentsView.offsetHeight;
    if (height) {
      ipc.getSettings().then((raw) => {
        const settings = raw ? JSON.parse(raw) : {};
        settings.agentsPanelHeight = height;
        ipc.setSettings(JSON.stringify(settings)).catch(() => {});
      }).catch(() => {});
    }
  }

  document.addEventListener("mouseup", () => {
    if (!hDragging) return;
    hDragging = false;
    agentsHandle.classList.remove("dragging");
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    persistAgentsHeight();
  });

  // Keyboard resizing on the divider itself (ArrowUp grows the panel).
  agentsHandle.tabIndex = 0;
  agentsHandle.setAttribute("role", "separator");
  agentsHandle.setAttribute("aria-orientation", "horizontal");
  agentsHandle.setAttribute("aria-label", "Resize agents panel");
  agentsHandle.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    applyAgentsPanelHeight(agentsView.offsetHeight + (e.key === "ArrowUp" ? 16 : -16));
    persistAgentsHeight();
  });
}
