// Stylesheet order lives in styles/index.css; xterm.css stays last so its
// base rules are what terminal.css was written against.
import "./styles/index.css";
import "@xterm/xterm/css/xterm.css";
import { appState } from "./state";
import * as ipc from "./ipc";
import { toast, reportError } from "./components/toast";
import { showConfirm } from "./components/confirm";
import { renderActivityBar } from "./components/activity-bar";
import { initSidebar, toggleSidebar } from "./components/sidebar";
import { initTerminalPanel, openTerminalSearch } from "./components/terminal-panel";
import { initKanbanPanel } from "./components/kanban-panel";
import { initApiPanel } from "./components/api-panel";
import { initMarkdownEditorPanel } from "./components/markdown-editor-panel";
import { initCodeEditorPanel } from "./components/code-editor-panel";
import { initWebPreviewPanel, openWebPreviewTab } from "./components/web-preview-panel";
import { tearDownAndClosePane } from "./components/tab-bar";
import { initPaneView } from "./components/pane-view";
import { bindAction, handleGlobalKeydown, loadShortcuts } from "./shortcuts";
import { settingsStore } from "./settings";
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
import { showSessionsDialog } from "./components/dialogs/sessions-dialog";
import { initMenuBar } from "./components/menu-bar";
import { initChatPanel, initChatResize, toggleChatPanel } from "./components/chat-panel";
import { initUiZoom, resetZoom, zoomIn, zoomOut } from "./ui-zoom";
import { initTooltips } from "./components/tooltip";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { themeEngine } from "./theme";

async function init() {
  // The settings document backs every persisted UI preference (sidebar
  // width, shortcuts, pane layouts, …); load it once before anything reads.
  await settingsStore.load();
  // Load theme before rendering to avoid flash
  await themeEngine.loadFromStorage();
  // Persisted UI zoom (rem scale + terminal font) — before anything renders.
  initUiZoom();

  initTooltips();
  initMenuBar(document.getElementById("menu-bar")!);
  renderActivityBar(document.getElementById("activity-bar")!);
  initSidebar();
  const mainContentEl = document.getElementById("main-content")!;
  await initTerminalPanel(mainContentEl);
  initKanbanPanel(mainContentEl);
  initApiPanel(mainContentEl);
  initMarkdownEditorPanel(mainContentEl);
  initCodeEditorPanel(mainContentEl);
  initWebPreviewPanel(mainContentEl);
  initPaneView(mainContentEl);
  renderStatusBar(document.getElementById("status-bar")!);
  initToasts();
  await initChatPanel(document.getElementById("chat-panel")!);
  initChatResize();

  appState.setSessionsAvailable(await ipc.sessionsAvailable().catch(() => false));

  try {
    await appState.loadPaneTrees();
    const workspaces = await ipc.listWorkspaces();
    appState.setWorkspaces(workspaces);
    if (workspaces.length > 0) {
      const detail = await ipc.switchWorkspace(0);
      appState.setActiveWorkspace(0, detail);
    }
  } catch (err) {
    reportError("Failed to load workspaces", err);
  }

  // Say what the session daemon brought back, and badge the workspaces the
  // user hasn't looked at yet.
  ipc.restoreSummary().then((rs) => {
    if (rs.sessions === 0) return;
    const n = rs.sessions;
    const m = rs.workspaces.length;
    toast(`Restored ${n} session${n === 1 ? "" : "s"} in ${m} workspace${m === 1 ? "" : "s"}`, "info");
    appState.markRestored(rs.workspaces);
  }).catch(() => {});

  // Notify LSP backend when workspace focus changes
  let lastFocusedWorkspace = -1;
  appState.on("active-workspace-changed", () => {
    const newIdx = appState.activeWorkspace;
    if (lastFocusedWorkspace >= 0 && lastFocusedWorkspace !== newIdx) {
      ipc.lspNotifyWorkspaceFocus(lastFocusedWorkspace, false).catch(() => {});
    }
    ipc.lspNotifyWorkspaceFocus(newIdx, true).catch(() => {});
    lastFocusedWorkspace = newIdx;
  });

  ipc.onGitRefresh((event) => {
    appState.updateFiles(event.workspace_idx, event.files, event.ahead_behind, event.branch);
  });
  ipc.onSysinfoUpdate((formatted) => {
    appState.setSysinfo(formatted);
  });

  // Shell-integration events from shell tabs (cwd + exit code).
  ipc.onPtyShellEvent((event) => {
    appState.applyShellEvent(event);
    if (event.kind === "command-end") {
      const wsIdx = appState.workspaceIndexForTab(event.tab_id);
      if (wsIdx >= 0) appState.markWorkspaceAttention(wsIdx);
    }
  });

  // Structured Claude Code lifecycle events (per-tab agent status). The
  // backend already rides the `pty-attention` rail for the attention-worthy
  // ones, so here we only update per-tab state.
  ipc.onPtyAgentEvent((event) => {
    appState.applyAgentEvent(event);
  });

  // Provider-tab idle notifications (and any other backend "needs attention").
  ipc.onPtyAttention((event) => {
    appState.markWorkspaceAttention(event.workspace_idx);
  });

  // Confirm quit when processes are alive. The backend knows which tabs the
  // daemon keeps and which die with the window; ask it (fast: no I/O) and
  // word the dialog accordingly.
  let closeConfirmPending = false;
  let quitApproved = false;
  const win = getCurrentWindow();
  try {
    await win.onCloseRequested((event) => {
      if (quitApproved || closeConfirmPending) return;
      // Cheap local pre-check: if nothing is alive at all, just close.
      const anyAlive = appState.workspaces.some((ws) => ws.tabs.some((t) => t.alive));
      if (!anyAlive) return;
      event.preventDefault();
      closeConfirmPending = true;
      ipc.quitSummary().catch(() => ({ persistent: 0, local: 0 })).then((q) => {
        if (q.persistent === 0 && q.local === 0) {
          quitApproved = true;
          win.destroy();
          return;
        }
        showCloseConfirm(q, () => {
          quitApproved = true;
          win.destroy();
        }, () => {
          closeConfirmPending = false;
        });
      });
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
  bindAction("web-preview", () => openWebPreviewTab());
  bindAction("theme", () => showThemeDialog());
  bindAction("settings", () => showSettingsDialog());
  bindAction("manage-providers", () => showProvidersDialog());
  bindAction("logs", () => showLogsDialog());
  bindAction("sessions", () => showSessionsDialog());
  bindAction("system-info", () => showSysinfoDialog());
  bindAction("api-jq-filter", () => document.dispatchEvent(new CustomEvent("toggle-jq")));
  bindAction("undo", () => handleUndo());
  bindAction("toggle-sidebar", () => toggleSidebar());
  bindAction("toggle-chat", () => toggleChatPanel());
  bindAction("help", () => showHelpDialog());
  bindAction("zoom-in", zoomIn);
  bindAction("zoom-out", zoomOut);
  bindAction("zoom-reset", resetZoom);
  bindAction("zoom-in-terminal", zoomIn);
  bindAction("zoom-out-terminal", zoomOut);
  bindAction("zoom-reset-terminal", resetZoom);
  bindAction("new-tab", () => appState.newBlankTab());
  bindAction("split-right", () => appState.splitActivePane("right"));
  bindAction("split-down", () => appState.splitActivePane("down"));
  bindAction("close-pane", () => {
    const id = appState.activePaneId;
    if (id) tearDownAndClosePane(id);
  });

  // Apply user shortcut overrides (from the settings store)
  loadShortcuts();

  // Tab switching via custom event from shortcut system
  document.addEventListener("switch-tab", ((e: CustomEvent) => {
    const ws = appState.activeWs;
    if (!ws || ws.tabs.length <= 1) return;
    const next = (ws.activeTab + e.detail.direction + ws.tabs.length) % ws.tabs.length;
    appState.setActiveTab(next);
  }) as EventListener);

  // Alt+1…9 from the shortcut system: jump straight to workspace N.
  document.addEventListener("switch-workspace", ((e: CustomEvent) => {
    const idx: number = e.detail.index;
    if (idx === appState.activeWorkspace || idx >= appState.workspaces.length) return;
    ipc.switchWorkspace(idx).then((detail) => {
      appState.setActiveWorkspace(idx, detail);
    }).catch((err) => reportError("Workspace switch failed", err));
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

function showCloseConfirm(
  q: { persistent: number; local: number },
  onConfirm: () => void,
  onCancel: () => void,
) {
  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  const lines: string[] = [];
  if (q.persistent > 0) {
    lines.push(`<p>${plural(q.persistent, "session")} keep${q.persistent === 1 ? "s" : ""} running in the background — the session daemon holds ${q.persistent === 1 ? "it" : "them"} and ${q.persistent === 1 ? "it" : "they"} reattach on the next launch.</p>`);
  }
  if (q.local > 0) {
    lines.push(`<p>${plural(q.local, "terminal session")} run${q.local === 1 ? "s" : ""} in-process and will be <strong>terminated</strong>.</p>`);
  }
  const destructive = q.local > 0;
  showConfirm({
    bodyHtml: `${lines.join("")}<p class="ws-delete-hint">${destructive ? "Quit anyway?" : "Quit?"}</p>`,
    actions: [
      { label: "Quit", kind: destructive ? "danger" : "primary", isDefault: true, onSelect: () => onConfirm() },
      { label: "Cancel", kind: "secondary", onSelect: () => onCancel() },
    ],
    onDismiss: onCancel,
  });
}

// Disable browser context menu so the app feels native
document.addEventListener("contextmenu", (e) => e.preventDefault());

document.addEventListener("DOMContentLoaded", init);
