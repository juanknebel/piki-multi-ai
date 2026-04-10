import "@xterm/xterm/css/xterm.css";
import { appState } from "./state";
import * as ipc from "./ipc";
import { toast } from "./components/toast";
import { renderActivityBar } from "./components/activity-bar";
import { initSidebar } from "./components/sidebar";
import { renderTabBar } from "./components/tab-bar";
import { initTerminalPanel, openTerminalSearch } from "./components/terminal-panel";
import { initKanbanPanel } from "./components/kanban-panel";
import { initApiPanel } from "./components/api-panel";
import { initMarkdownEditorPanel } from "./components/markdown-editor-panel";
import { bindAction, handleGlobalKeydown, loadShortcuts } from "./shortcuts";
import { showSettingsDialog } from "./components/dialogs/settings-dialog";
import { showProvidersDialog } from "./components/dialogs/providers-dialog";
import { renderStatusBar } from "./components/status-bar";
import { initToasts } from "./components/toast";
import { openCommandPalette } from "./components/command-palette";
import { showWorkspaceDialog } from "./components/dialogs/workspace-dialog";
import { openWorkspaceSwitcher } from "./components/workspace-switcher";
import { showMergeDialog } from "./components/dialogs/merge-dialog";
import { openFuzzySearch } from "./components/fuzzy-search";
import { openProjectSearch } from "./components/project-search";
import { showGitLog } from "./components/dialogs/gitlog-dialog";
import { showStashDialog } from "./components/dialogs/stash-dialog";
import { showCodeReview } from "./components/code-review";
import { showAgentManager } from "./components/dialogs/agent-dialog";
import { showDispatchDialog } from "./components/dialogs/dispatch-dialog";
import { showHelpDialog } from "./components/dialogs/help-dialog";
import { showDashboard } from "./components/dialogs/dashboard-dialog";
import { showSysinfoDialog } from "./components/dialogs/sysinfo-dialog";
import { showThemeDialog } from "./components/dialogs/theme-dialog";
import { showLogsDialog } from "./components/dialogs/logs-dialog";
import { initMenuBar, toggleSidebar } from "./components/menu-bar";
import { initTooltips } from "./components/tooltip";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { themeEngine } from "./theme";

async function init() {
  // Load theme before rendering to avoid flash
  await themeEngine.loadFromStorage();

  initTooltips();
  initMenuBar(document.getElementById("menu-bar")!);
  renderActivityBar(document.getElementById("activity-bar")!);
  initSidebar();
  renderTabBar(document.getElementById("tab-bar")!);
  const mainContentEl = document.getElementById("main-content")!;
  await initTerminalPanel(mainContentEl);
  initKanbanPanel(mainContentEl);
  initApiPanel(mainContentEl);
  initMarkdownEditorPanel(mainContentEl);
  renderStatusBar(document.getElementById("status-bar")!);
  initToasts();

  try {
    const workspaces = await ipc.listWorkspaces();
    appState.setWorkspaces(workspaces);
    if (workspaces.length > 0) {
      const detail = await ipc.switchWorkspace(0);
      appState.setActiveWorkspace(0, detail);
    }
  } catch (err) {
    console.error("Failed to load workspaces:", err);
  }

  ipc.onGitRefresh((event) => {
    appState.updateFiles(event.workspace_idx, event.files, event.ahead_behind);
  });
  ipc.onSysinfoUpdate((formatted) => {
    appState.setSysinfo(formatted);
  });

  // Confirm quit when PTYs are active
  let closeConfirmPending = false;
  const win = getCurrentWindow();
  try {
    await win.onCloseRequested((event) => {
      let activeCount = 0;
      for (const ws of appState.workspaces) {
        for (const tab of ws.tabs) {
          if (tab.alive) activeCount++;
        }
      }
      if (activeCount > 0 && !closeConfirmPending) {
        event.preventDefault();
        closeConfirmPending = true;
        showCloseConfirm(activeCount, () => {
          win.destroy();
        }, () => {
          closeConfirmPending = false;
        });
      }
      // activeCount === 0: don't preventDefault, window closes normally
    });
  } catch (err) {
    console.error("Failed to register close handler:", err);
  }

  // Bind actions to the shortcut system
  bindAction("command-palette", () => openCommandPalette());
  bindAction("new-workspace", () => showWorkspaceDialog({ mode: "create" }));
  bindAction("merge-rebase", () => showMergeDialog());
  bindAction("workspace-switcher", () => openWorkspaceSwitcher());
  bindAction("fuzzy-search", () => openFuzzySearch());
  bindAction("project-search", () => openProjectSearch());
  bindAction("terminal-search", () => openTerminalSearch());
  bindAction("git-log", () => showGitLog());
  bindAction("dashboard", () => showDashboard());
  bindAction("git-stash", () => showStashDialog());
  bindAction("code-review", () => showCodeReview());
  bindAction("agent-manager", () => showAgentManager());
  bindAction("dispatch-agent", () => showDispatchDialog());
  bindAction("kanban", () => appState.setActiveView("kanban"));
  bindAction("theme", () => showThemeDialog());
  bindAction("settings", () => showSettingsDialog());
  bindAction("manage-providers", () => showProvidersDialog());
  bindAction("logs", () => showLogsDialog());
  bindAction("system-info", () => showSysinfoDialog());
  bindAction("api-jq-filter", () => document.dispatchEvent(new CustomEvent("toggle-jq")));
  bindAction("undo", () => handleUndo());
  bindAction("toggle-sidebar", () => toggleSidebar());
  bindAction("help", () => showHelpDialog());

  // Load user shortcut overrides from storage
  await loadShortcuts();

  // Tab switching via custom event from shortcut system
  document.addEventListener("switch-tab", ((e: CustomEvent) => {
    const ws = appState.activeWs;
    if (!ws || ws.tabs.length <= 1) return;
    const next = (ws.activeTab + e.detail.direction + ws.tabs.length) % ws.tabs.length;
    appState.setActiveTab(next);
  }) as EventListener);

  // Global keyboard shortcuts — capture phase so they fire before xterm.js
  document.addEventListener("keydown", handleGlobalKeydown, true);
}

async function handleUndo() {
  const entry = appState.popUndo();
  if (!entry) {
    toast("Nothing to undo", "info");
    return;
  }
  const wsIdx = appState.activeWorkspace;
  try {
    for (const file of entry.files) {
      if (entry.action === "stage") {
        await ipc.gitUnstage(wsIdx, file);
      } else {
        await ipc.gitStage(wsIdx, file);
      }
    }
    const files = await ipc.getChangedFiles(wsIdx);
    appState.updateFiles(wsIdx, files, appState.activeWs?.aheadBehind ?? null);
    toast(`Undid ${entry.action} of ${entry.files.length} file(s)`, "info");
  } catch (err) {
    toast(`Undo failed: ${err}`, "error");
  }
}

function showCloseConfirm(activeCount: number, onConfirm: () => void, onCancel: () => void) {
  document.querySelector(".ws-delete-confirm")?.remove();

  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm";
  const label = activeCount === 1 ? "1 terminal session is" : `${activeCount} terminal sessions are`;
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p>${label} still running.</p>
      <p class="ws-delete-hint">Close anyway?</p>
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-danger ws-confirm-yes">Close</button>
        <button class="dialog-btn dialog-btn-secondary ws-confirm-no">Cancel</button>
      </div>
    </div>
  `;

  overlay.querySelector(".ws-confirm-yes")!.addEventListener("click", () => {
    overlay.remove();
    onConfirm();
  });

  overlay.querySelector(".ws-confirm-no")!.addEventListener("click", () => {
    overlay.remove();
    onCancel();
  });

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      overlay.remove();
      onCancel();
    }
  });

  document.body.appendChild(overlay);
}

// Disable browser context menu so the app feels native
document.addEventListener("contextmenu", (e) => e.preventDefault());

document.addEventListener("DOMContentLoaded", init);
