import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { showWorkspaceDialog, showWorkspaceInfo } from "./dialogs/workspace-dialog";
import { showMergeDialog } from "./dialogs/merge-dialog";
import { showGitLog } from "./dialogs/gitlog-dialog";
import { showStashDialog } from "./dialogs/stash-dialog";
import { showCodeReview } from "./code-review";
import { openFuzzySearch } from "./fuzzy-search";
import { openWorkspaceSwitcher } from "./workspace-switcher";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";
import { showHelpDialog } from "./dialogs/help-dialog";
import { showDashboard } from "./dialogs/dashboard-dialog";
import { openTerminalSearch } from "./terminal-panel";
import { PROVIDER_LABELS, type AIProvider } from "../types";

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
        ${cmd.keybinding ? `<span class="palette-key">${cmd.keybinding}</span>` : ""}
      `;
      item.addEventListener("click", () => {
        closeCommandPalette();
        cmd.action();
      });
      item.addEventListener("mouseenter", () => {
        selectedIdx = idx;
        renderResults();
      });
      results.appendChild(item);
    });

    if (filtered.length === 0) {
      results.innerHTML = '<div class="palette-empty">No matching commands</div>';
    }
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
      renderResults();
      scrollToSelected(results);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      renderResults();
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
      action: async () => {
        if (confirm(`Delete workspace "${ws.info.name}"?`)) {
          try {
            await ipc.deleteWorkspace(wsIdx);
            appState.removeWorkspace(wsIdx);
            toast(`Deleted "${ws.info.name}"`, "info");
          } catch (err) {
            toast(`Delete failed: ${err}`, "error");
          }
        }
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
  ];
  for (const provider of tabProviders) {
    cmds.push({
      id: `tab-${provider}`,
      label: `New ${PROVIDER_LABELS[provider]} Tab`,
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
      keybinding: "Ctrl+L",
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
  if (ws) {
    cmds.push({
      id: "agent-dispatch",
      label: "Dispatch Agent",
      category: "Agents",
      keybinding: "Ctrl+Shift+D",
      action: () => showDispatchDialog(),
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
    label: "Show Agents",
    category: "View",
    action: () => appState.setActiveView("agents"),
  });
  cmds.push({
    id: "view-dashboard",
    label: "Dashboard",
    category: "View",
    keybinding: "Ctrl+Shift+W",
    action: () => showDashboard(),
  });
  cmds.push({
    id: "view-help",
    label: "Keyboard Shortcuts",
    category: "Help",
    keybinding: "?",
    action: () => showHelpDialog(),
  });
  cmds.push({
    id: "terminal-search",
    label: "Search in Terminal",
    category: "Search",
    keybinding: "Ctrl+Shift+F",
    action: () => openTerminalSearch(),
  });

  return cmds;
}

async function spawnTabSafe(provider: AIProvider) {
  const wsIdx = appState.activeWorkspace;
  try {
    const tabId = await ipc.spawnTab(wsIdx, provider);
    appState.addTab(wsIdx, { id: tabId, provider, alive: true });
  } catch (err) {
    toast(`Failed to open ${PROVIDER_LABELS[provider]}: ${err}`, "error");
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

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
