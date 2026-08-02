import { appState } from "../state";
import * as ipc from "../ipc";
import { cliAgentStatusView, type AgentRow } from "../types";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";

/** Always-visible panel docked below the workspace list, listing the live AI
 *  agent tabs of ALL workspaces (the desktop counterpart of the TUI's
 *  bottom-left Agents pane). Rows come from the backend (`list_agent_rows`)
 *  because the frontend only hydrates a workspace's tab list on first visit —
 *  agents dispatched into never-visited workspaces must still show. Click
 *  jumps to that workspace + tab. */
export function renderAgentsPanel(container: HTMLElement) {
  container.innerHTML = `
    <div class="sidebar-header">
      <span>Agents</span>
      <span class="agents-header-actions">
        <button class="sc-header-btn" id="agents-dispatch-btn" title="Dispatch Agent">＋</button>
        <button class="sc-header-btn" id="agents-manage-btn" title="Manage Agents">⚙</button>
      </span>
    </div>
    <div class="agents-list" id="agents-list"></div>
  `;

  const list = container.querySelector<HTMLElement>("#agents-list")!;
  container.querySelector("#agents-manage-btn")!.addEventListener("click", () => showAgentManager());
  container.querySelector("#agents-dispatch-btn")!.addEventListener("click", () => showDispatchDialog());

  let refreshQueued = false;

  async function refresh() {
    let rows: AgentRow[];
    try {
      rows = await ipc.listAgentRows();
    } catch (err) {
      console.error("Failed to list agent rows:", err);
      return;
    }
    render(rows);
  }

  /** Coalesce bursts (agent events arrive per tool call) into one fetch. */
  function scheduleRefresh() {
    if (refreshQueued) return;
    refreshQueued = true;
    setTimeout(() => {
      refreshQueued = false;
      refresh();
    }, 100);
  }

  function render(rows: AgentRow[]) {
    list.innerHTML = "";
    if (rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "agents-empty";
      empty.textContent = "No agents running";
      list.appendChild(empty);
      return;
    }

    for (const row of rows) {
      const el = document.createElement("div");
      const isCurrent =
        row.workspace_idx === appState.activeWorkspace &&
        appState.activeWs?.tabs[appState.activeWs.activeTab]?.id === row.tab_id;
      el.className = `agent-row${isCurrent ? " current" : ""}`;

      // Status vocabulary shared with the tab bar / status bar; a tab without
      // a structured channel falls back to alive/exited (same as the TUI).
      const v = row.status
        ? cliAgentStatusView(row.status, row.attention)
        : row.alive
          ? { glyph: "●", label: "alive", color: "var(--text-muted)" }
          : { glyph: "○", label: "exited", color: "var(--text-muted)" };

      el.innerHTML = `
        <span class="agent-row-glyph" style="color:${v.color}">${v.glyph}</span>
        <span class="agent-row-main">
          <span class="agent-row-title">
            <span class="agent-row-ws">${escapeHtml(row.workspace_name)}</span>
            <span class="agent-row-sep">·</span>
            <span class="agent-row-label">${escapeHtml(row.label)}</span>
          </span>
          ${row.summary ? `<span class="agent-row-summary">${escapeHtml(row.summary)}</span>` : ""}
        </span>
        <span class="agent-row-status" style="color:${v.color}">${v.label}</span>
      `;
      el.title = row.summary ?? "";
      el.addEventListener("click", () => jumpTo(row));
      list.appendChild(el);
    }
  }

  async function jumpTo(row: AgentRow) {
    try {
      if (row.workspace_idx !== appState.activeWorkspace) {
        const detail = await ipc.switchWorkspace(row.workspace_idx);
        appState.setActiveWorkspace(row.workspace_idx, detail);
      }
      appState.setActiveTab(row.tab_idx);
    } catch (err) {
      console.error("Jump to agent failed:", err);
    }
  }

  appState.on("tab-shell-state-changed", scheduleRefresh);
  appState.on("tabs-changed", scheduleRefresh);
  appState.on("workspaces-changed", scheduleRefresh);
  appState.on("active-workspace-changed", scheduleRefresh);
  appState.on("active-tab-changed", scheduleRefresh);
  appState.on("view-changed", scheduleRefresh);

  refresh();
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
