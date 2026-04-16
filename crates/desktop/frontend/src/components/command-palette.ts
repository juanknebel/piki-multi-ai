import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { showWorkspaceDialog, showWorkspaceInfo } from "./dialogs/workspace-dialog";
import { showMergeDialog } from "./dialogs/merge-dialog";
import { showGitLog } from "./dialogs/gitlog-dialog";
import { showStashDialog } from "./dialogs/stash-dialog";
import { showCodeReview } from "./code-review";
import { openFuzzySearch } from "./fuzzy-search";
import { openProjectSearch } from "./project-search";
import { showSettingsDialog } from "./dialogs/settings-dialog";
import { showProvidersDialog } from "./dialogs/providers-dialog";
import { openWorkspaceSwitcher } from "./workspace-switcher";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";
import { showHelpDialog } from "./dialogs/help-dialog";
import { showDashboard } from "./dialogs/dashboard-dialog";
import { showSysinfoDialog } from "./dialogs/sysinfo-dialog";
import { openTerminalSearch } from "./terminal-panel";
import { showThemeDialog } from "./dialogs/theme-dialog";
import { showLogsDialog } from "./dialogs/logs-dialog";
import { showAboutDialog } from "./dialogs/about-dialog";
import { getProviderLabel, getProviderKey, type AIProvider } from "../types";
import { themeEngine } from "../theme";
import { getShortcutKey, formatShortcut } from "../shortcuts";

interface Command {
  id: string;
  label: string;
  category: string;
  keybinding?: string;
  action: () => void | Promise<void>;
}

let paletteEl: HTMLElement | null = null;

export function openCommandPalette() {
  if (paletteEl) {
    closeCommandPalette();
    return;
  }

  const commands = buildCommands();

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";

  const palette = document.createElement("div");
  palette.className = "palette";

  palette.innerHTML = `
    <input class="palette-input" type="text" placeholder="Type a command..." autofocus />
    <div class="palette-results"></div>
  `;

  backdrop.appendChild(palette);
  document.body.appendChild(backdrop);
  paletteEl = backdrop;

  const input = palette.querySelector<HTMLInputElement>(".palette-input")!;
  const results = palette.querySelector<HTMLElement>(".palette-results")!;
  let selectedIdx = 0;
  let filtered = commands;

  function renderResults() {
    results.innerHTML = "";
    filtered.forEach((cmd, idx) => {
      const item = document.createElement("div");
      item.className = `palette-item${idx === selectedIdx ? " selected" : ""}`;
      item.innerHTML = `
        <span class="palette-category">${escapeHtml(cmd.category)}</span>
        <span class="palette-label">${highlightMatch(cmd.label, input.value)}</span>
        ${cmd.keybinding ? `<span class="palette-key">${formatShortcut(cmd.keybinding)}</span>` : ""}
      `;
      item.addEventListener("click", () => {
        closeCommandPalette();
        cmd.action();
      });
      item.addEventListener("mouseenter", () => {
        if (selectedIdx === idx) return;
        selectedIdx = idx;
        updateSelection();
      });
      results.appendChild(item);
    });

    if (filtered.length === 0) {
      results.innerHTML = '<div class="palette-empty">No matching commands</div>';
    }
  }

  function updateSelection() {
    results.querySelectorAll<HTMLElement>(".palette-item").forEach((el, i) => {
      el.classList.toggle("selected", i === selectedIdx);
    });
  }

  function filter() {
    const query = input.value.toLowerCase();
    if (!query) {
      filtered = commands;
    } else {
      filtered = commands.filter(
        (cmd) =>
          cmd.label.toLowerCase().includes(query) ||
          cmd.category.toLowerCase().includes(query),
      );
    }
    selectedIdx = 0;
    renderResults();
  }

  input.addEventListener("input", filter);

  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, filtered.length - 1);
      updateSelection();
      scrollToSelected(results);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      updateSelection();
      scrollToSelected(results);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filtered[selectedIdx]) {
        closeCommandPalette();
        filtered[selectedIdx].action();
      }
    } else if (e.key === "Escape") {
      closeCommandPalette();
    }
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeCommandPalette();
  });

  renderResults();
  input.focus();
}

export function closeCommandPalette() {
  paletteEl?.remove();
  paletteEl = null;
}

function buildCommands(): Command[] {
  const cmds: Command[] = [];

  // Workspace commands
  cmds.push({
    id: "ws-create",
    label: "Create Workspace",
    category: "Workspace",
    keybinding: "Ctrl+N",
    action: () => showWorkspaceDialog({ mode: "create" }),
  });

  const ws = appState.activeWs;
  const wsIdx = appState.activeWorkspace;

  if (ws) {
    cmds.push({
      id: "ws-edit",
      label: `Edit "${ws.info.name}"`,
      category: "Workspace",
      action: () => showWorkspaceDialog({ mode: "edit", editIndex: wsIdx }),
    });
    cmds.push({
      id: "ws-info",
      label: `Info "${ws.info.name}"`,
      category: "Workspace",
      action: () => showWorkspaceInfo(wsIdx),
    });
    cmds.push({
      id: "ws-clone",
      label: `Clone "${ws.info.name}"`,
      category: "Workspace",
      action: () =>
        showWorkspaceDialog({ mode: "clone", cloneFrom: ws.info }),
    });
    cmds.push({
      id: "ws-delete",
      label: `Delete "${ws.info.name}"`,
      category: "Workspace",
      action: () => {
        showConfirmDialog(
          `Delete workspace "${ws.info.name}"?`,
          "This will remove the worktree and branch.",
          async () => {
            try {
              await ipc.deleteWorkspace(wsIdx);
              appState.removeWorkspace(wsIdx);
              toast(`Deleted "${ws.info.name}"`, "info");
            } catch (err) {
              toast(`Delete failed: ${err}`, "error");
            }
          },
        );
      },
    });
  }

  // Switch workspace commands
  appState.workspaces.forEach((w, i) => {
    if (i === wsIdx) return;
    cmds.push({
      id: `ws-switch-${i}`,
      label: `Switch to "${w.info.name}"`,
      category: "Switch",
      keybinding: i < 9 ? String(i + 1) : undefined,
      action: async () => {
        try {
          const detail = await ipc.switchWorkspace(i);
          appState.setActiveWorkspace(i, detail);
        } catch (err) {
          toast(`Switch failed: ${err}`, "error");
        }
      },
    });
  });

  // Tab commands
  const tabProviders: AIProvider[] = [
    "Shell",
    "Claude",
    "Gemini",
    "OpenCode",
    "Kilo",
    "Codex",
    "Api",
  ];
  for (const provider of tabProviders) {
    cmds.push({
      id: `tab-${provider}`,
      label: `New ${getProviderLabel(provider)} Tab`,
      category: "Tab",
      action: () => spawnTabSafe(provider),
    });
  }

  // Git commands
  if (ws) {
    cmds.push({
      id: "git-commit",
      label: "Commit",
      category: "Git",
      action: () => {
        appState.setActiveView("git");
        setTimeout(() => {
          document.querySelector<HTMLTextAreaElement>(".sc-commit-input")?.focus();
        }, 50);
      },
    });
    cmds.push({
      id: "git-push",
      label: "Push",
      category: "Git",
      action: async () => {
        try {
          await ipc.gitPush(wsIdx);
          toast("Pushed successfully", "success");
        } catch (err) {
          toast(`Push failed: ${err}`, "error");
        }
      },
    });
    cmds.push({
      id: "git-stage-all",
      label: "Stage All Changes",
      category: "Git",
      action: async () => {
        try {
          await ipc.gitStageAll(wsIdx);
          const files = await ipc.getChangedFiles(wsIdx);
          appState.updateFiles(wsIdx, files, ws.aheadBehind);
          toast("All changes staged", "success");
        } catch (err) {
          toast(`Stage all failed: ${err}`, "error");
        }
      },
    });
    cmds.push({
      id: "git-merge",
      label: "Merge / Rebase into Main",
      category: "Git",
      keybinding: "Ctrl+M",
      action: () => showMergeDialog(),
    });
    cmds.push({
      id: "git-unstage-all",
      label: "Unstage All Changes",
      category: "Git",
      action: async () => {
        try {
          await ipc.gitUnstageAll(wsIdx);
          const files = await ipc.getChangedFiles(wsIdx);
          appState.updateFiles(wsIdx, files, ws.aheadBehind);
          toast("All changes unstaged", "success");
        } catch (err) {
          toast(`Unstage all failed: ${err}`, "error");
        }
      },
    });
    cmds.push({
      id: "git-log",
      label: "Git Log",
      category: "Git",
      keybinding: "Alt+L",
      action: () => showGitLog(),
    });
    cmds.push({
      id: "git-stash",
      label: "Git Stash",
      category: "Git",
      keybinding: "Ctrl+Shift+S",
      action: () => showStashDialog(),
    });
  }

  // Code review
  if (ws) {
    cmds.push({
      id: "code-review",
      label: "Code Review (PR)",
      category: "Review",
      keybinding: "Ctrl+Shift+R",
      action: () => showCodeReview(),
    });
  }

  // Agent commands
  cmds.push({
    id: "agent-manage",
    label: "Manage Agents",
    category: "Agents",
    keybinding: "Ctrl+Shift+A",
    action: () => showAgentManager(),
  });
  cmds.push({
    id: "manage-providers",
    label: "Manage Providers",
    category: "Settings",
    keybinding: "Alt+P",
    action: () => showProvidersDialog(),
  });
  if (ws) {
    cmds.push({
      id: "agent-dispatch",
      label: "Dispatch Agent",
      category: "Agents",
      keybinding: "Ctrl+Shift+D",
      action: () => showDispatchDialog(),
    });
  }

  // Undo
  if (ws) {
    cmds.push({
      id: "git-undo",
      label: "Undo Stage / Unstage",
      category: "Git",
      keybinding: "Ctrl+Z",
      action: async () => {
        const entry = appState.popUndo();
        if (!entry) { toast("Nothing to undo", "info"); return; }
        try {
          for (const file of entry.files) {
            if (entry.action === "stage") {
              await ipc.gitUnstage(wsIdx, file);
            } else {
              await ipc.gitStage(wsIdx, file);
            }
          }
          const files = await ipc.getChangedFiles(wsIdx);
          appState.updateFiles(wsIdx, files, ws.aheadBehind);
          toast(`Undid ${entry.action} of ${entry.files.length} file(s)`, "info");
        } catch (err) {
          toast(`Undo failed: ${err}`, "error");
        }
      },
    });
  }

  // Search commands
  cmds.push({
    id: "search-files",
    label: "Find File",
    category: "Search",
    keybinding: "Ctrl+F",
    action: () => openFuzzySearch(),
  });
  cmds.push({
    id: "search-project",
    label: "Search in Project",
    category: "Search",
    keybinding: "Ctrl+Shift+F",
    action: () => openProjectSearch(),
  });
  cmds.push({
    id: "search-workspace",
    label: "Switch Workspace",
    category: "Search",
    keybinding: "Ctrl+Space",
    action: () => openWorkspaceSwitcher(),
  });

  // View commands
  cmds.push({
    id: "view-explorer",
    label: "Show Explorer",
    category: "View",
    action: () => appState.setActiveView("explorer"),
  });
  cmds.push({
    id: "view-git",
    label: "Show Source Control",
    category: "View",
    action: () => appState.setActiveView("git"),
  });
  cmds.push({
    id: "view-agents",
    label: "Manage Agents",
    category: "View",
    action: () => showAgentManager(),
  });
  cmds.push({
    id: "view-kanban",
    label: "Show Kanban Board",
    category: "View",
    keybinding: "Alt+K",
    action: () => appState.setActiveView("kanban"),
  });
  cmds.push({
    id: "view-dashboard",
    label: "Dashboard",
    category: "View",
    keybinding: "Alt+D",
    action: () => showDashboard(),
  });
  cmds.push({
    id: "view-sysinfo",
    label: "System Info",
    category: "View",
    keybinding: "Alt+I",
    action: () => showSysinfoDialog(),
  });
  cmds.push({
    id: "view-help",
    label: "Keyboard Shortcuts",
    category: "Help",
    keybinding: "?",
    action: () => showHelpDialog(),
  });
  cmds.push({
    id: "settings",
    label: "Settings",
    category: "Edit",
    keybinding: "Alt+S",
    action: () => showSettingsDialog(),
  });
  cmds.push({
    id: "terminal-search",
    label: "Search in Terminal",
    category: "Search",
    keybinding: "Ctrl+Shift+B",
    action: () => openTerminalSearch(),
  });
  cmds.push({
    id: "api-jq-filter",
    label: "API jq Filter",
    category: "Search",
    keybinding: getShortcutKey("api-jq-filter"),
    action: () => { document.dispatchEvent(new CustomEvent("toggle-jq")); },
  });

  // About
  cmds.push({
    id: "about",
    label: "About Piki Desktop",
    category: "Help",
    action: () => showAboutDialog(),
  });

  // Logs
  cmds.push({
    id: "app-logs",
    label: "Application Logs",
    category: "View",
    keybinding: "Alt+Shift+L",
    action: () => showLogsDialog(),
  });

  // LSP commands
  cmds.push({
    id: "lsp-status",
    label: "LSP: Show Server Status",
    category: "LSP",
    action: async () => {
      try {
        const servers = await ipc.lspServerStatus();
        if (servers.length === 0) {
          toast("No LSP servers running", "info");
        } else {
          const lines = servers.map(
            (s) => `${s.server_id} [${s.status}] ${s.root_path.split("/").pop()}`
          );
          toast(lines.join("\n"), "info");
        }
      } catch (err) {
        toast(`LSP status failed: ${err}`, "error");
      }
    },
  });
  cmds.push({
    id: "lsp-stop-all",
    label: "LSP: Stop All Servers",
    category: "LSP",
    action: async () => {
      try {
        const servers = await ipc.lspServerStatus();
        for (const s of servers) {
          await ipc.lspStopServer(s.server_id, s.root_path);
        }
        toast(`Stopped ${servers.length} LSP server(s)`, "info");
      } catch (err) {
        toast(`LSP stop failed: ${err}`, "error");
      }
    },
  });

  // Theme commands
  cmds.push({
    id: "theme-settings",
    label: "Theme Settings",
    category: "Theme",
    keybinding: "Alt+T",
    action: () => showThemeDialog(),
  });
  for (const preset of themeEngine.getPresets()) {
    cmds.push({
      id: `theme-${preset.id}`,
      label: `Theme: ${preset.name}`,
      category: "Theme",
      action: () => themeEngine.setPreset(preset.id),
    });
  }

  return cmds;
}

async function spawnTabSafe(provider: AIProvider) {
  const wsIdx = appState.activeWorkspace;
  try {
    const tabId = await ipc.spawnTab(wsIdx, getProviderKey(provider));
    appState.addTab(wsIdx, { id: tabId, provider, alive: true });
  } catch (err) {
    toast(`Failed to open ${getProviderLabel(provider)}: ${err}`, "error");
  }
}

function highlightMatch(text: string, query: string): string {
  if (!query) return escapeHtml(text);
  const lower = text.toLowerCase();
  const qLower = query.toLowerCase();
  const idx = lower.indexOf(qLower);
  if (idx === -1) return escapeHtml(text);
  const before = text.slice(0, idx);
  const match = text.slice(idx, idx + query.length);
  const after = text.slice(idx + query.length);
  return `${escapeHtml(before)}<strong>${escapeHtml(match)}</strong>${escapeHtml(after)}`;
}

function scrollToSelected(container: HTMLElement) {
  const selected = container.querySelector(".palette-item.selected");
  selected?.scrollIntoView({ block: "nearest" });
}

function showConfirmDialog(message: string, hint: string, onConfirm: () => void) {
  document.querySelector(".ws-delete-confirm")?.remove();
  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm";
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p>${escapeHtml(message)}</p>
      <p class="ws-delete-hint">${escapeHtml(hint)}</p>
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-danger ws-confirm-yes">Delete</button>
        <button class="dialog-btn dialog-btn-secondary ws-confirm-no">Cancel</button>
      </div>
    </div>
  `;
  overlay.querySelector(".ws-confirm-yes")!.addEventListener("click", () => { overlay.remove(); onConfirm(); });
  overlay.querySelector(".ws-confirm-no")!.addEventListener("click", () => overlay.remove());
  overlay.addEventListener("click", (e) => { if (e.target === overlay) overlay.remove(); });
  document.body.appendChild(overlay);
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
