import { appState } from "../state";
import { getProviderLabel, cliAgentStatusView, type FileStatus } from "../types";
import { showAboutDialog } from "./dialogs/about-dialog";
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

    // Left side
    const branch = ws?.branch ?? "—";
    addItem(container, `⎇ ${branch}`, "clickable");

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

    // Right side
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
          const v = cliAgentStatusView(agentState.agentStatus);
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
        const label = getProviderLabel(tab.provider);
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

function addItem(container: HTMLElement, text: string, ...classes: string[]) {
  const item = document.createElement("div");
  item.className = ["status-item", ...classes].join(" ");
  item.textContent = text;
  container.appendChild(item);
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
