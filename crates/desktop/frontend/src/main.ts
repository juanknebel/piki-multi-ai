import "@xterm/xterm/css/xterm.css";
import { appState } from "./state";
import * as ipc from "./ipc";
import { toast } from "./components/toast";
import { renderActivityBar } from "./components/activity-bar";
import { initSidebar } from "./components/sidebar";
import { renderTabBar } from "./components/tab-bar";
import { initTerminalPanel, openTerminalSearch } from "./components/terminal-panel";
import { renderStatusBar } from "./components/status-bar";
import { initToasts } from "./components/toast";
import { openCommandPalette } from "./components/command-palette";
import { showWorkspaceDialog } from "./components/dialogs/workspace-dialog";
import { openWorkspaceSwitcher } from "./components/workspace-switcher";
import { showMergeDialog } from "./components/dialogs/merge-dialog";
import { openFuzzySearch } from "./components/fuzzy-search";
import { showGitLog } from "./components/dialogs/gitlog-dialog";
import { showStashDialog } from "./components/dialogs/stash-dialog";
import { showCodeReview } from "./components/code-review";
import { showAgentManager } from "./components/dialogs/agent-dialog";
import { showDispatchDialog } from "./components/dialogs/dispatch-dialog";
import { showHelpDialog } from "./components/dialogs/help-dialog";
import { showDashboard } from "./components/dialogs/dashboard-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";

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

  // Global keyboard shortcuts
  document.addEventListener("keydown", (e) => {
    const inTerminal = !!document.activeElement?.closest(".xterm");
    const inInput =
      document.activeElement?.tagName === "INPUT" ||
      document.activeElement?.tagName === "TEXTAREA";

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
    if (e.ctrlKey && !e.shiftKey && e.key === "f") {
      e.preventDefault();
      openFuzzySearch();
      return;
    }
    // Ctrl+Shift+F: Terminal search
    if (e.ctrlKey && e.shiftKey && e.key === "F") {
      e.preventDefault();
      openTerminalSearch();
      return;
    }
    // Alt+L: Git log
    if (e.altKey && !e.ctrlKey && e.key === "l") {
      e.preventDefault();
      showGitLog();
      return;
    }
    // Alt+D: Dashboard
    if (e.altKey && !e.ctrlKey && e.key === "d") {
      e.preventDefault();
      showDashboard();
      return;
    }
    // Ctrl+Shift+S: Git stash
    if (e.ctrlKey && e.shiftKey && e.key === "S") {
      e.preventDefault();
      showStashDialog();
      return;
    }
    // Ctrl+Shift+R: Code review
    if (e.ctrlKey && e.shiftKey && e.key === "R") {
      e.preventDefault();
      showCodeReview();
      return;
    }
    // Ctrl+Shift+A: Agent management
    if (e.ctrlKey && e.shiftKey && e.key === "A") {
      e.preventDefault();
      showAgentManager();
      return;
    }
    // Ctrl+Shift+D: Dispatch agent
    if (e.ctrlKey && e.shiftKey && e.key === "D") {
      e.preventDefault();
      showDispatchDialog();
      return;
    }
    // Ctrl+Z: Undo stage/unstage (not in terminal/input)
    if (e.ctrlKey && e.key === "z" && !e.shiftKey && !inTerminal && !inInput) {
      e.preventDefault();
      handleUndo();
      return;
    }
    // ?: Help (not in terminal/input)
    if (e.key === "?" && !inTerminal && !inInput) {
      e.preventDefault();
      showHelpDialog();
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

document.addEventListener("DOMContentLoaded", init);
