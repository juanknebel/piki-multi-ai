import { appState } from "../state";
import { makeInteractive } from "./a11y";
import { reportError } from "./toast";
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
    let external: ipc.ExternalTreePayload[] = [];
    try {
      rows = await ipc.listAgentRows();
    } catch (err) {
      console.error("Failed to list agent rows:", err);
      return;
    }
    try {
      external = await ipc.listExternalAgents();
    } catch {
      external = [];
    }
    render(rows, external);
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

  // Poll external agents every 2s (they live outside piki's PTY)
  setInterval(() => {
    ipc.listExternalAgents().then((ext) => {
      // re-render with current rows + new external; fetch rows again to stay fresh
      ipc.listAgentRows().then((rows) => render(rows, ext)).catch(() => {});
    }).catch(() => {});
  }, 2000);

  function render(rows: AgentRow[], external: ipc.ExternalTreePayload[] = []) {
    const prevScroll = list.scrollTop;
    list.innerHTML = "";
    if (rows.length === 0 && external.length === 0) {
      const empty = document.createElement("div");
      empty.className = "agents-empty";
      empty.innerHTML = `
        <p>No agents running</p>
        <button class="dialog-btn dialog-btn-secondary empty-cta">Dispatch Agent</button>
      `;
      empty.querySelector(".empty-cta")!.addEventListener("click", () => showDispatchDialog());
      list.appendChild(empty);
    } else {
      for (const row of rows) {
        const el = document.createElement("div");
        const isCurrent =
          row.workspace_idx === appState.activeWorkspace &&
          appState.activeWs?.tabs[appState.activeWs.activeTab]?.id === row.tab_id;
        el.className = `agent-row${isCurrent ? " current" : ""}`;

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
        makeInteractive(el);
        list.appendChild(el);
      }
    }
    // External agents section (via /proc, claude only)
    if (external.length > 0) {
      const header = document.createElement("div");
      header.className = "agents-external-header";
      header.textContent = `External (${external.length})`;
      list.appendChild(header);
      for (const tree of external) {
        const wsName = tree.root.workspace_name ?? "Outside";
        const cwd = tree.root.cwd ? tree.root.cwd.slice(0, 40) : "—";
        const el = document.createElement("div");
        el.className = "agent-row external";
        el.innerHTML = `
          <span class="agent-row-glyph" style="color:var(--text-muted)">◉</span>
          <span class="agent-row-main">
            <span class="agent-row-title">
              <span class="agent-row-ws">${escapeHtml(wsName)}</span>
              <span class="agent-row-sep">·</span>
              <span class="agent-row-label">${escapeHtml(tree.root.provider)} #${tree.root.pid}</span>
            </span>
            <span class="agent-row-summary">${escapeHtml(cwd)}${tree.children.length ? ` · ↳ ${tree.children.length} sub` : ""}</span>
          </span>
          <button class="agent-row-action" title="Open terminal at cwd">⎘</button>
        `;
        const btn = el.querySelector<HTMLButtonElement>(".agent-row-action")!;
        btn.addEventListener("click", async (e) => {
          e.stopPropagation();
          const targetIdx = tree.root.workspace_idx ?? appState.activeWorkspace;
          const cwdPath = tree.root.cwd ?? "";
          if (cwdPath) {
            try {
              await ipc.spawnTerminalAt(targetIdx, cwdPath);
            } catch (err) {
              reportError("Failed to open terminal", err);
            }
          }
        });
        el.title = tree.root.cmd;
        list.appendChild(el);
        for (const child of tree.children) {
          const cEl = document.createElement("div");
          cEl.className = "agent-row external child";
          cEl.innerHTML = `
            <span class="agent-row-glyph" style="color:var(--text-muted)">└─</span>
            <span class="agent-row-main">
              <span class="agent-row-title"><span class="agent-row-label">${escapeHtml(child.provider)} #${child.pid}</span></span>
              <span class="agent-row-summary">${escapeHtml(child.cmd.slice(0, 50))}</span>
            </span>
          `;
          cEl.title = child.cmd;
          list.appendChild(cEl);
        }
      }
    }
    if (rows.length > 0 || external.length > 0) {
      list.scrollTop = prevScroll;
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
      reportError("Jump to agent failed", err);
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
