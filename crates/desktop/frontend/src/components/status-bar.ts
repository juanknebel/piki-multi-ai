import { appState } from "../state";
import { getProviderLabel, getTabLabel, cliAgentStatusView, type FileStatus } from "../types";
import { showAboutDialog } from "./dialogs/about-dialog";
import { showSessionsDialog } from "./dialogs/sessions-dialog";
import { jumpToAttention } from "./agents-panel";
import { attentionRows } from "../agent-attention";
import { getShortcutKey } from "../shortcuts";
import { branchLabel } from "../labels";
import { openBranchPicker } from "./dialogs/branch-picker";
import * as ipc from "../ipc";

const STAGED_STATUSES: FileStatus[] = ["Staged", "Added", "Renamed", "StagedModified"];

export function renderStatusBar(container: HTMLElement) {
  function render() {
    const ws = appState.activeWs;
    container.innerHTML = "";

    // App name (clickable → About)
    const appName = document.createElement("div");
    appName.className = "status-item clickable status-app-name";
    appName.textContent = "Piki Desktop";
    appName.addEventListener("click", showAboutDialog);
    container.appendChild(appName);

    // Left side — the branch, shortened by the shared rule (full in the tooltip).
    // Click (or the `switch-branch` key) opens the branch switcher.
    const branchItem = addItem(container, `⎇ ${branchLabel(ws?.branch)}`, "clickable");
    branchItem.title = ws?.branch
      ? `${ws.branch}\nClick or ${getShortcutKey("switch-branch")} to switch branch`
      : "No git branch";
    branchItem.addEventListener("click", () => openBranchPicker());

    if (ws?.aheadBehind) {
      const [ahead, behind] = ws.aheadBehind;
      if (ahead > 0 || behind > 0) {
        const sync = `${ahead > 0 ? "↑" + ahead : ""}${behind > 0 ? " ↓" + behind : ""}`.trim();
        addItem(container, sync);
      }
    }

    const files = ws?.changedFiles ?? [];
    if (files.length > 0) {
      const stagedCount = files.filter((f) => STAGED_STATUSES.includes(f.status)).length;
      const unstagedCount = files.length - stagedCount;
      const parts: string[] = [];
      if (unstagedCount > 0) {
        parts.push(`${unstagedCount} change${unstagedCount !== 1 ? "s" : ""}`);
      }
      if (stagedCount > 0) {
        parts.push(`${stagedCount} staged`);
      }
      addItem(container, parts.join(" · "));
    }

    // Spacer
    const spacer = document.createElement("div");
    spacer.className = "status-spacer";
    container.appendChild(spacer);

    // Right side.
    // Agents needing you, across ALL workspaces — the signal that survives
    // a hidden sidebar. Click = the `Alt+A` jump.
    const needing = attentionRows(appState.agentRows);
    if (needing.length > 0) {
      const item = document.createElement("div");
      item.className = "status-item clickable status-attention";
      item.textContent = `● ${needing.length} need${needing.length === 1 ? "s" : ""} you`;
      item.title = `${needing.map((r) => `${r.workspace_name} · ${r.label}`).join("\n")}\nClick or ${getShortcutKey("jump-attention")} to jump`;
      item.addEventListener("click", () => jumpToAttention());
      container.appendChild(item);
    }

    if (ws && ws.tabs.length > 0) {
      const tab = ws.tabs[ws.activeTab];
      if (tab) {
        // Shell tabs with shell-integration: show cwd before the tab label.
        if (tab.provider === "Shell") {
          const shellState = appState.getTabShellState(tab.id);
          if (shellState?.cwd) {
            addItem(container, `📁 ${formatHomeRelative(shellState.cwd)}`, "status-cwd");
          }
        }
        // Claude agent tabs: structured status glyph + summary preview.
        const agentState = appState.getTabShellState(tab.id);
        if (agentState?.agentStatus) {
          const v = cliAgentStatusView(agentState.agentStatus, agentState.attention ?? false);
          const sum = agentState.agentSummary
            ? `: ${truncate(agentState.agentSummary, 60)}`
            : "";
          const item = document.createElement("div");
          item.className = "status-item status-agent";
          item.style.color = v.color;
          item.textContent = `${v.glyph} ${v.label}${sum}`;
          item.title = agentState.agentSummary ?? v.label;
          container.appendChild(item);
        }
        const label = getTabLabel(tab);
        const alive = tab.alive ? "" : " (exited)";
        addItem(container, `${label}${alive}`);
      }
    }

    // LSP status — rendered from the cache so frequent re-renders (agent
    // events fire per tool call) don't issue an IPC round-trip each and
    // flicker while the async answer lands.
    const lspItem = document.createElement("div");
    lspItem.className = "status-item status-lsp";
    lspItem.textContent = lspCache.text;
    if (lspCache.color) lspItem.style.color = lspCache.color;
    container.appendChild(lspItem);

    const wsName = ws?.info.name ?? "No workspace";
    addItem(container, wsName);

    // Persistent-session daemon: `sessions N` / `sessions off` /
    // `sessions unavailable`, from the poll cache; click opens the dialog.
    const sessionsItem = document.createElement("div");
    sessionsItem.className = "status-item clickable status-sessions";
    applySessionsCache(sessionsItem);
    sessionsItem.addEventListener("click", () => void showSessionsDialog());
    container.appendChild(sessionsItem);

    // Sysinfo
    if (appState.sysinfo) {
      addItem(container, appState.sysinfo);
    }
  }

  appState.on("active-workspace-changed", render);
  appState.on("files-changed", render);
  appState.on("tabs-changed", render);
  appState.on("active-tab-changed", render);
  appState.on("sysinfo-changed", render);
  appState.on("tab-shell-state-changed", render);
  appState.on("agent-rows-changed", render);
  render();

  // Poll LSP status on its own cadence, patching the live element in place.
  const refreshLsp = () =>
    void refreshLspCache().then(() => {
      const el = container.querySelector<HTMLElement>(".status-lsp");
      if (el) {
        el.textContent = lspCache.text;
        if (lspCache.color) el.style.color = lspCache.color;
      }
    });
  refreshLsp();
  setInterval(refreshLsp, 5000);

  // Daemon health every 3s (the list call is the probe), patched in place.
  const refreshSessions = () =>
    void refreshSessionsCache().then(() => {
      const el = container.querySelector<HTMLElement>(".status-sessions");
      if (el) applySessionsCache(el);
    });
  refreshSessions();
  setInterval(refreshSessions, 3000);
}

const sessionsCache: { text: string; state: string; title: string } = {
  text: "sessions …",
  state: "unknown",
  title: "Persistent sessions",
};

function applySessionsCache(el: HTMLElement) {
  el.textContent = sessionsCache.text;
  el.title = sessionsCache.title;
  el.dataset.state = sessionsCache.state;
}

async function refreshSessionsCache(): Promise<void> {
  try {
    const s = await ipc.sessionStatus();
    sessionsCache.state = s.state;
    if (s.state === "on") {
      sessionsCache.text = `sessions ${s.live}`;
      sessionsCache.title = `${s.live} live session${s.live === 1 ? "" : "s"} in the daemon${s.daemon_pid != null ? ` (pid ${s.daemon_pid})` : ""} — click to manage`;
    } else if (s.state === "off") {
      sessionsCache.text = "sessions off";
      sessionsCache.title = "Persistent sessions disabled in config.toml ([sessions] enabled = false) — tabs run in-process";
    } else {
      sessionsCache.text = "sessions unavailable";
      sessionsCache.title = "The session daemon is not answering — tabs opened now run in-process and die with the window";
    }
  } catch {
    sessionsCache.state = "unavailable";
    sessionsCache.text = "sessions unavailable";
    sessionsCache.title = "Could not query the session daemon";
  }
}

/** Replace a leading `$HOME` segment with `~` so the bar stays compact. */
function formatHomeRelative(path: string): string {
  // No HOME env on the frontend; we accept whatever the backend sent verbatim.
  // Keep last 3 segments so very deep paths don't dominate the status bar.
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 3) return path;
  return ".../" + parts.slice(-3).join("/");
}

function truncate(s: string, max: number): string {
  const oneLine = s.replace(/\s+/g, " ").trim();
  return oneLine.length > max ? oneLine.slice(0, max - 1) + "…" : oneLine;
}

function addItem(container: HTMLElement, text: string, ...classes: string[]): HTMLElement {
  const item = document.createElement("div");
  item.className = ["status-item", ...classes].join(" ");
  item.textContent = text;
  container.appendChild(item);
  return item;
}

const lspCache = { text: "", color: "" };

async function refreshLspCache(): Promise<void> {
  try {
    const servers = await ipc.lspServerStatus();
    if (servers.length === 0) {
      lspCache.text = "";
      lspCache.color = "";
      return;
    }
    const active = servers.filter((s) => s.status === "active");
    const idle = servers.filter((s) => s.status === "idle");
    const parts: string[] = [];
    if (active.length > 0) {
      parts.push(active.map((s) => s.server_id).join(", "));
    }
    if (idle.length > 0) {
      parts.push(`${idle.length} idle`);
    }
    lspCache.text = `LSP: ${parts.join(" · ")}`;
    lspCache.color = active.length > 0 ? "var(--git-added)" : "var(--text-muted)";
  } catch {
    lspCache.text = "";
    lspCache.color = "";
  }
}
