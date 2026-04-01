import "@xterm/xterm/css/xterm.css";
import { appState } from "./state";
import * as ipc from "./ipc";
import { renderActivityBar } from "./components/activity-bar";
import { initSidebar } from "./components/sidebar";
import { renderTabBar } from "./components/tab-bar";
import { initTerminalPanel } from "./components/terminal-panel";
import { renderStatusBar } from "./components/status-bar";
import { initToasts } from "./components/toast";
import { openCommandPalette } from "./components/command-palette";
import { showWorkspaceDialog } from "./components/dialogs/workspace-dialog";
import { openWorkspaceSwitcher } from "./components/workspace-switcher";
import { showMergeDialog } from "./components/dialogs/merge-dialog";
import { openFuzzySearch } from "./components/fuzzy-search";
import { showGitLog } from "./components/dialogs/gitlog-dialog";
import { showStashDialog } from "./components/dialogs/stash-dialog";

async function init() {
  renderActivityBar(document.getElementById("activity-bar")!);
  initSidebar();
  renderTabBar(document.getElementById("tab-bar")!);
  await initTerminalPanel(document.getElementById("main-content")!);
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

  // Global keyboard shortcuts
  document.addEventListener("keydown", (e) => {
    // Ctrl+P: Command palette
    if (e.ctrlKey && (e.key === "p" || e.key === "P")) {
      e.preventDefault();
      openCommandPalette();
      return;
    }
    // Ctrl+N: New workspace
    if (e.ctrlKey && e.key === "n") {
      e.preventDefault();
      showWorkspaceDialog({ mode: "create" });
      return;
    }
    // Ctrl+M: Merge/Rebase
    if (e.ctrlKey && e.key === "m") {
      e.preventDefault();
      showMergeDialog();
      return;
    }
    // Ctrl+Space: Workspace switcher
    if (e.ctrlKey && e.key === " ") {
      e.preventDefault();
      openWorkspaceSwitcher();
      return;
    }
    // Ctrl+F: Fuzzy file search
    if (e.ctrlKey && e.key === "f") {
      e.preventDefault();
      openFuzzySearch();
      return;
    }
    // Ctrl+L: Git log
    if (e.ctrlKey && e.key === "l") {
      e.preventDefault();
      showGitLog();
      return;
    }
    // Ctrl+S: Git stash (only when not in terminal)
    if (e.ctrlKey && e.shiftKey && e.key === "S") {
      e.preventDefault();
      showStashDialog();
      return;
    }
    // Ctrl+Tab / Ctrl+Shift+Tab: switch tabs
    if (e.ctrlKey && e.key === "Tab") {
      e.preventDefault();
      const ws = appState.activeWs;
      if (!ws || ws.tabs.length <= 1) return;
      const dir = e.shiftKey ? -1 : 1;
      const next = (ws.activeTab + dir + ws.tabs.length) % ws.tabs.length;
      appState.setActiveTab(next);
    }
  });
}

document.addEventListener("DOMContentLoaded", init);
