import { appState } from "../state";
import { settingsStore } from "../settings";
import { activityBarWidth, clampSidebarWidth, visibleChatWidth } from "../layout-budget";
import { renderWorkspaceList } from "./workspace-list";
import { renderFileTree } from "./file-tree";
import { renderSourceControl } from "./source-control";
import { renderAgentsPanel } from "./agents-panel";
import { showAgentManager } from "./dialogs/agent-dialog";
import { openProvider } from "./open-content";

/** Show/hide the whole sidebar column (activity bar stays). */
export function toggleSidebar() {
  document.getElementById("app")!.classList.toggle("sidebar-hidden");
}

/** Room the workspace list must keep above the Agents panel: its header
 *  plus `MIN_VISIBLE_WORKSPACE_ROWS` rows (a `.workspace-item` is
 *  `--row-height-lg`, 32px at density 1 — the caller measures a live row so
 *  density / zoom are honoured). Below that the list still scrolls, but
 *  four rows are the floor a drag cannot cross. */
const MIN_VISIBLE_WORKSPACE_ROWS = 4;
const WORKSPACE_ROW_PX = 32;
const SIDEBAR_HEADER_PX = 30;
const AGENTS_HANDLE_PX = 4;

/** Largest Agents-panel height that still leaves the workspace list its
 *  minimum rows. Exported for the (pure) clamp rule; `sidebarHeight` is the
 *  sidebar's inner height. */
export function maxAgentsPanelHeight(sidebarHeight: number, rowPx = WORKSPACE_ROW_PX): number {
  const reserved = SIDEBAR_HEADER_PX + MIN_VISIBLE_WORKSPACE_ROWS * rowPx + AGENTS_HANDLE_PX;
  return Math.max(64, Math.min(sidebarHeight * 0.75, sidebarHeight - reserved));
}

/** Set an explicit Agents-panel height (px), replacing the default 40% cap.
 *  Clamped so neither the panel nor the workspace list (≥4 rows, or its own
 *  scrollbar) can be squeezed out. */
function applyAgentsPanelHeight(px: number) {
  const view = document.getElementById("agents-view");
  const sidebar = document.getElementById("sidebar");
  if (!view || !sidebar) return;
  const rowPx = document.querySelector<HTMLElement>(".workspace-item")?.offsetHeight || WORKSPACE_ROW_PX;
  const max = maxAgentsPanelHeight(sidebar.clientHeight || window.innerHeight, rowPx);
  const clamped = Math.max(32, Math.min(max, px));
  view.style.height = `${clamped}px`;
  view.style.maxHeight = "none";
}

export async function initSidebar() {
  // Restore persisted sidebar width + Agents panel height
  await settingsStore.load();
  const sidebarWidth = settingsStore.get<number>("sidebarWidth");
  if (sidebarWidth) {
    document.documentElement.style.setProperty("--sidebar-width", `${sidebarWidth}px`);
  }
  const agentsPanelHeight = settingsStore.get<number>("agentsPanelHeight");
  if (agentsPanelHeight) applyAgentsPanelHeight(agentsPanelHeight);

  const explorerView = document.getElementById("explorer-view")!;
  const workspaceList = document.getElementById("workspace-list")!;
  const filesView = document.getElementById("files-view")!;
  const scView = document.getElementById("source-control-view")!;
  const agentsView = document.getElementById("agents-view")!;

  renderWorkspaceList(workspaceList);
  renderFileTree(filesView);
  renderSourceControl(scView);
  // Agents panel docked at the bottom of the sidebar, below whichever view
  // is active (same layout as the TUI's bottom-left pane) — ALWAYS visible,
  // never a switchable view.
  renderAgentsPanel(agentsView);

  // Track last sidebar view so we can restore when a non-sidebar action triggers
  let lastSidebarView: "explorer" | "files" | "git" = "explorer";

  function updateView() {
    const view = appState.activeView;

    if (view === "kanban") {
      void openProvider("Kanban");
      appState.setActiveView(lastSidebarView);
      return;
    }

    if (view === "api") {
      void openProvider("Api");
      appState.setActiveView(lastSidebarView);
      return;
    }

    if (view === "web-preview") {
      void openProvider("WebPreview");
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
    const newWidth = clampSidebarWidth(startWidth + delta, window.innerWidth, visibleChatWidth(), activityBarWidth());
    root.style.setProperty("--sidebar-width", `${newWidth}px`);
  });

  function persistSidebarWidth() {
    const width = parseInt(getComputedStyle(root).getPropertyValue("--sidebar-width"));
    if (width) settingsStore.patch("sidebarWidth", width);
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
    const next = clampSidebarWidth(
      cur + (e.key === "ArrowRight" ? 16 : -16),
      window.innerWidth,
      visibleChatWidth(),
      activityBarWidth(),
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
    if (height) settingsStore.patch("agentsPanelHeight", height);
  }

  document.addEventListener("mouseup", () => {
    if (!hDragging) return;
    hDragging = false;
    agentsHandle.classList.remove("dragging");
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    persistAgentsHeight();
  });

  // A shorter window must not let a persisted height eat the workspace
  // list: re-apply the clamp whenever the sidebar's height changes.
  window.addEventListener("resize", () => {
    if (agentsView.style.height) applyAgentsPanelHeight(agentsView.offsetHeight);
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
