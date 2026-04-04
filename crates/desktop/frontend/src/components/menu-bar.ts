import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { showWorkspaceDialog } from "./dialogs/workspace-dialog";
import { showMergeDialog } from "./dialogs/merge-dialog";
import { showGitLog } from "./dialogs/gitlog-dialog";
import { showStashDialog } from "./dialogs/stash-dialog";
import { showCodeReview } from "./code-review";
import { openFuzzySearch } from "./fuzzy-search";
import { openWorkspaceSwitcher } from "./workspace-switcher";
import { openTerminalSearch } from "./terminal-panel";
import { openProjectSearch } from "./project-search";
import { showSettingsDialog } from "./dialogs/settings-dialog";
import { openCommandPalette } from "./command-palette";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";
import { showHelpDialog } from "./dialogs/help-dialog";
import { showDashboard } from "./dialogs/dashboard-dialog";
import { showThemeDialog } from "./dialogs/theme-dialog";
import { showLogsDialog } from "./dialogs/logs-dialog";
import { showAboutDialog } from "./dialogs/about-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PROVIDER_LABELS, type AIProvider } from "../types";

// ── Types ───────────────────────────────────────

interface MenuItem {
  label: string;
  shortcut?: string;
  action?: () => void | Promise<void>;
  disabled?: () => boolean;
  separator?: boolean;
  submenu?: MenuItem[];
}

interface MenuDefinition {
  label: string;
  items: () => MenuItem[];
}

// ── Helpers ─────────────────────────────────────

const noWs = () => !appState.activeWs;

function spawnTab(provider: AIProvider) {
  const wsIdx = appState.activeWorkspace;
  ipc.spawnTab(wsIdx, provider).then((tabId) => {
    appState.addTab(wsIdx, { id: tabId, provider, alive: true });
  }).catch((err) => {
    toast(`Failed to open ${PROVIDER_LABELS[provider]}: ${err}`, "error");
  });
}

const SEP: MenuItem = { label: "", separator: true };

// ── Menu definitions ────────────────────────────

const MENUS: MenuDefinition[] = [
  {
    label: "File",
    items: () => [
      { label: "New Workspace", shortcut: "Ctrl+N", action: () => showWorkspaceDialog({ mode: "create" }) },
      {
        label: "New Tab",
        disabled: noWs,
        submenu: (["Shell", "Claude", "Gemini", "OpenCode", "Kilo", "Codex", "Kanban"] as AIProvider[]).map(
          (p) => ({ label: PROVIDER_LABELS[p], action: () => spawnTab(p) }),
        ),
      },
      SEP,
      {
        label: "Close Tab",
        disabled: () => {
          const ws = appState.activeWs;
          return !ws || ws.tabs.length === 0;
        },
        action: () => {
          const ws = appState.activeWs;
          if (!ws || ws.tabs.length === 0) return;
          const tab = ws.tabs[ws.activeTab];
          if (!tab) return;
          const wsIdx = appState.activeWorkspace;
          ipc.closeTab(wsIdx, ws.activeTab).catch(() => {});
          appState.removeTab(wsIdx, ws.activeTab);
        },
      },
      SEP,
      { label: "Quit", action: () => getCurrentWindow().close() },
    ],
  },
  {
    label: "Edit",
    items: () => [
      {
        label: "Undo Stage / Unstage",
        shortcut: "Ctrl+Z",
        disabled: noWs,
        action: async () => {
          const entry = appState.popUndo();
          if (!entry) { toast("Nothing to undo", "info"); return; }
          const wsIdx = appState.activeWorkspace;
          try {
            for (const file of entry.files) {
              if (entry.action === "stage") await ipc.gitUnstage(wsIdx, file);
              else await ipc.gitStage(wsIdx, file);
            }
            const files = await ipc.getChangedFiles(wsIdx);
            appState.updateFiles(wsIdx, files, appState.activeWs?.aheadBehind ?? null);
            toast(`Undid ${entry.action} of ${entry.files.length} file(s)`, "info");
          } catch (err) {
            toast(`Undo failed: ${err}`, "error");
          }
        },
      },
      SEP,
      { label: "Find File", shortcut: "Ctrl+F", action: () => openFuzzySearch() },
      { label: "Search in Project", shortcut: "Ctrl+Shift+F", action: () => openProjectSearch() },
      { label: "Search in Terminal", shortcut: "Ctrl+Shift+B", action: () => openTerminalSearch() },
      SEP,
      { label: "Theme Settings", shortcut: "Alt+T", action: () => showThemeDialog() },
      { label: "Settings", shortcut: "Alt+S", action: () => showSettingsDialog() },
    ],
  },
  {
    label: "View",
    items: () => [
      { label: "Explorer", action: () => appState.setActiveView("explorer") },
      { label: "Source Control", action: () => appState.setActiveView("git") },
      { label: "Agents", action: () => appState.setActiveView("agents") },
      { label: "Kanban Board", shortcut: "Alt+K", action: () => appState.setActiveView("kanban") },
      SEP,
      { label: "Command Palette", shortcut: "Ctrl+P", action: () => openCommandPalette() },
      { label: "Workspace Switcher", shortcut: "Ctrl+Space", action: () => openWorkspaceSwitcher() },
      { label: "Dashboard", shortcut: "Alt+D", action: () => showDashboard() },
      { label: "Application Logs", shortcut: "Alt+Shift+L", action: () => showLogsDialog() },
      SEP,
      { label: "Next Tab", shortcut: "Ctrl+Tab", action: () => cycleTab(1) },
      { label: "Previous Tab", shortcut: "Ctrl+Shift+Tab", action: () => cycleTab(-1) },
    ],
  },
  {
    label: "Git",
    items: () => [
      {
        label: "Commit",
        disabled: noWs,
        action: () => {
          appState.setActiveView("git");
          setTimeout(() => document.querySelector<HTMLTextAreaElement>(".sc-commit-input")?.focus(), 50);
        },
      },
      {
        label: "Push",
        disabled: noWs,
        action: async () => {
          try {
            await ipc.gitPush(appState.activeWorkspace);
            toast("Pushed successfully", "success");
          } catch (err) {
            toast(`Push failed: ${err}`, "error");
          }
        },
      },
      SEP,
      {
        label: "Stage All",
        disabled: noWs,
        action: async () => {
          const wsIdx = appState.activeWorkspace;
          const ws = appState.activeWs;
          try {
            await ipc.gitStageAll(wsIdx);
            const files = await ipc.getChangedFiles(wsIdx);
            appState.updateFiles(wsIdx, files, ws?.aheadBehind ?? null);
            toast("All changes staged", "success");
          } catch (err) {
            toast(`Stage all failed: ${err}`, "error");
          }
        },
      },
      {
        label: "Unstage All",
        disabled: noWs,
        action: async () => {
          const wsIdx = appState.activeWorkspace;
          const ws = appState.activeWs;
          try {
            await ipc.gitUnstageAll(wsIdx);
            const files = await ipc.getChangedFiles(wsIdx);
            appState.updateFiles(wsIdx, files, ws?.aheadBehind ?? null);
            toast("All changes unstaged", "success");
          } catch (err) {
            toast(`Unstage all failed: ${err}`, "error");
          }
        },
      },
      SEP,
      { label: "Merge / Rebase", shortcut: "Ctrl+M", disabled: noWs, action: () => showMergeDialog() },
      { label: "Git Log", shortcut: "Alt+L", disabled: noWs, action: () => showGitLog() },
      { label: "Git Stash", shortcut: "Ctrl+Shift+S", disabled: noWs, action: () => showStashDialog() },
    ],
  },
  {
    label: "Agents",
    items: () => [
      { label: "Manage Agents", shortcut: "Ctrl+Shift+A", action: () => showAgentManager() },
      { label: "Dispatch Agent", shortcut: "Ctrl+Shift+D", disabled: noWs, action: () => showDispatchDialog() },
    ],
  },
  {
    label: "Help",
    items: () => [
      { label: "Keyboard Shortcuts", shortcut: "?", action: () => showHelpDialog() },
      { label: "Code Review", shortcut: "Ctrl+Shift+R", disabled: noWs, action: () => showCodeReview() },
      SEP,
      { label: "About Piki Desktop", action: () => showAboutDialog() },
    ],
  },
];

function cycleTab(dir: number) {
  const ws = appState.activeWs;
  if (!ws || ws.tabs.length <= 1) return;
  const next = (ws.activeTab + dir + ws.tabs.length) % ws.tabs.length;
  appState.setActiveTab(next);
}

// ── State ───────────────────────────────────────

let openIdx: number | null = null;
let dropdownEl: HTMLElement | null = null;
let backdropEl: HTMLElement | null = null;
let submenuEl: HTMLElement | null = null;
let topButtons: HTMLButtonElement[] = [];

// ── Init ────────────────────────────────────────

export function initMenuBar(container: HTMLElement) {
  // Menu items (left side)
  MENUS.forEach((menu, idx) => {
    const btn = document.createElement("button");
    btn.className = "menu-top-item";
    btn.textContent = menu.label;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      if (openIdx === idx) {
        closeMenu();
      } else {
        openMenu(idx);
      }
    });
    btn.addEventListener("mouseenter", () => {
      if (openIdx !== null && openIdx !== idx) {
        openMenu(idx);
      }
    });
    container.appendChild(btn);
    topButtons.push(btn);
  });

  // Drag region — double-click to toggle maximize
  const dragRegion = document.createElement("div");
  dragRegion.className = "menu-drag-region";
  const win = getCurrentWindow();
  dragRegion.addEventListener("mousedown", (e) => {
    if (e.button === 0 && e.detail === 1) win.startDragging();
  });
  dragRegion.addEventListener("dblclick", () => win.toggleMaximize());
  container.appendChild(dragRegion);

  // Window controls (right side)
  const controls = document.createElement("div");
  controls.className = "window-controls";

  for (const { label, cls, action } of [
    { label: "\u2013", cls: "wc-minimize", action: () => win.minimize() },
    { label: "\u25A1", cls: "wc-maximize", action: () => win.toggleMaximize() },
    { label: "\u00D7", cls: "wc-close", action: () => win.close() },
  ]) {
    const btn = document.createElement("button");
    btn.className = `wc-btn ${cls}`;
    btn.textContent = label;
    btn.addEventListener("click", action);
    controls.appendChild(btn);
  }

  container.appendChild(controls);
}

export function toggleMenu(idx: number) {
  if (openIdx === idx) closeMenu();
  else openMenu(idx);
}

// ── Open / Close ────────────────────────────────

function openMenu(idx: number) {
  closeMenu();
  openIdx = idx;
  topButtons[idx].classList.add("open");

  const rect = topButtons[idx].getBoundingClientRect();
  const items = MENUS[idx].items();

  // Backdrop
  backdropEl = document.createElement("div");
  backdropEl.className = "menu-backdrop";
  backdropEl.addEventListener("click", closeMenu);
  document.body.appendChild(backdropEl);

  // Dropdown
  dropdownEl = document.createElement("div");
  dropdownEl.className = "menu-dropdown";
  dropdownEl.style.top = rect.bottom + "px";
  dropdownEl.style.left = rect.left + "px";

  renderItems(dropdownEl, items, true);

  document.body.appendChild(dropdownEl);

  // Keep dropdown in viewport
  const ddRect = dropdownEl.getBoundingClientRect();
  if (ddRect.right > window.innerWidth) {
    dropdownEl.style.left = Math.max(0, window.innerWidth - ddRect.width) + "px";
  }

  // Keyboard nav
  dropdownEl.addEventListener("keydown", handleDropdownKeys);
  dropdownEl.setAttribute("tabindex", "-1");
  dropdownEl.focus();
}

function closeMenu() {
  submenuEl?.remove();
  submenuEl = null;
  dropdownEl?.remove();
  dropdownEl = null;
  backdropEl?.remove();
  backdropEl = null;
  if (openIdx !== null) {
    topButtons[openIdx]?.classList.remove("open");
    openIdx = null;
  }
}

// ── Render items ────────────────────────────────

function renderItems(container: HTMLElement, items: MenuItem[], isRoot: boolean) {
  items.forEach((item) => {
    if (item.separator) {
      const sep = document.createElement("div");
      sep.className = "menu-separator";
      container.appendChild(sep);
      return;
    }

    const el = document.createElement("div");
    el.className = "menu-item";
    const isDisabled = item.disabled?.() ?? false;
    if (isDisabled) el.classList.add("disabled");

    // Label
    const label = document.createElement("span");
    label.textContent = item.label;
    el.appendChild(label);

    // Shortcut or submenu arrow
    if (item.submenu) {
      const arrow = document.createElement("span");
      arrow.className = "menu-submenu-arrow";
      arrow.textContent = "\u25B8";
      el.appendChild(arrow);
    } else if (item.shortcut) {
      const badge = document.createElement("span");
      badge.className = "menu-shortcut";
      badge.textContent = item.shortcut;
      el.appendChild(badge);
    }

    // Submenu hover
    if (item.submenu && !isDisabled) {
      el.addEventListener("mouseenter", () => {
        submenuEl?.remove();
        const elRect = el.getBoundingClientRect();
        submenuEl = document.createElement("div");
        submenuEl.className = "menu-dropdown";
        submenuEl.style.top = elRect.top + "px";
        submenuEl.style.left = elRect.right + "px";
        renderItems(submenuEl, item.submenu!, false);
        document.body.appendChild(submenuEl);

        // Keep in viewport
        const smRect = submenuEl.getBoundingClientRect();
        if (smRect.right > window.innerWidth) {
          submenuEl.style.left = (elRect.left - smRect.width) + "px";
        }
      });
    } else if (isRoot) {
      el.addEventListener("mouseenter", () => {
        submenuEl?.remove();
        submenuEl = null;
      });
    }

    // Click action
    if (item.action && !isDisabled) {
      el.addEventListener("click", () => {
        closeMenu();
        item.action!();
      });
    }

    container.appendChild(el);
  });
}

// ── Keyboard navigation ─────────────────────────

function handleDropdownKeys(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    closeMenu();
    return;
  }

  const target = dropdownEl;
  if (!target) return;
  const items = Array.from(target.querySelectorAll<HTMLElement>(".menu-item:not(.disabled)"));
  if (items.length === 0) return;

  let highlighted = target.querySelector<HTMLElement>(".menu-item.highlighted");
  let idx = highlighted ? items.indexOf(highlighted) : -1;

  if (e.key === "ArrowDown") {
    e.preventDefault();
    highlighted?.classList.remove("highlighted");
    idx = idx < items.length - 1 ? idx + 1 : 0;
    items[idx].classList.add("highlighted");
    items[idx].scrollIntoView({ block: "nearest" });
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    highlighted?.classList.remove("highlighted");
    idx = idx > 0 ? idx - 1 : items.length - 1;
    items[idx].classList.add("highlighted");
    items[idx].scrollIntoView({ block: "nearest" });
  } else if (e.key === "Enter" && highlighted) {
    e.preventDefault();
    highlighted.click();
  } else if (e.key === "ArrowRight" && openIdx !== null) {
    e.preventDefault();
    openMenu((openIdx + 1) % MENUS.length);
  } else if (e.key === "ArrowLeft" && openIdx !== null) {
    e.preventDefault();
    openMenu((openIdx - 1 + MENUS.length) % MENUS.length);
  }
}
