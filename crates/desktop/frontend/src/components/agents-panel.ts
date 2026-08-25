import { appState } from "../state";
import { makeInteractive } from "./a11y";
import { reportError, toast } from "./toast";
import * as ipc from "../ipc";
import { cliAgentStatusView, formatElapsed, type AgentRow } from "../types";
import { attentionRows, liveElapsedSecs, pickAttentionTarget } from "../agent-attention";
import { getShortcutKey } from "../shortcuts";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";

const ROWS_DEBOUNCE_MS = 100;
const ELAPSED_TICK_MS = 1000;

/** Keep `appState.agentRows` fresh: one debounced `list_agent_rows` fetch
 *  per burst of agent/tab events (they arrive per tool call), feeding every
 *  agent signal at once — this panel, the workspace-list rollup, the
 *  status-bar segment, the activity-bar badge and `Alt+A`. Backend-sourced
 *  because the frontend only hydrates a workspace's tab list on first
 *  visit — agents dispatched into never-visited workspaces must still
 *  show. Call once from main.ts. */
export function startAgentRowsSync() {
  let queued = false;
  async function refresh() {
    try {
      appState.setAgentRows(await ipc.listAgentRows());
    } catch (err) {
      console.error("Failed to list agent rows:", err);
    }
  }
  function schedule() {
    if (queued) return;
    queued = true;
    setTimeout(() => {
      queued = false;
      void refresh();
    }, ROWS_DEBOUNCE_MS);
  }
  appState.on("tab-shell-state-changed", schedule);
  appState.on("tabs-changed", schedule);
  appState.on("workspaces-changed", schedule);
  appState.on("active-workspace-changed", schedule);
  appState.on("active-tab-changed", schedule);
  void refresh();
}

/** Switch to the workspace + tab of `row`. Viewing the tab acknowledges its
 *  attention marker backend-side (`pty-agent-ack` follows). */
export async function jumpToAgentRow(row: AgentRow) {
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

/** `Alt+A`: land on the agent that needs you most — permission requests
 *  first, then unseen news — from any workspace; pressed again while
 *  standing on one, it walks to the next. */
export function jumpToAttention() {
  const ws = appState.activeWs;
  const target = pickAttentionTarget(appState.agentRows, {
    workspace_idx: appState.activeWorkspace,
    tab_id: ws?.tabs[ws.activeTab]?.id,
  });
  if (!target) {
    toast("No agent needs attention", "info");
    return;
  }
  void jumpToAgentRow(target);
}

/** Always-visible panel docked below the workspace list, listing the live AI
 *  agent tabs of ALL workspaces (the desktop counterpart of the TUI's
 *  bottom-left Agents pane). Renders `appState.agentRows`; click or
 *  Enter jumps to that workspace + tab, ↑/↓ move between rows. */
export function renderAgentsPanel(container: HTMLElement) {
  container.innerHTML = `
    <div class="sidebar-header">
      <span>Agents <span class="agents-attention-count" hidden></span></span>
      <span class="agents-header-actions">
        <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" id="agents-dispatch-btn" title="Dispatch Agent">＋</button>
        <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" id="agents-manage-btn" title="Manage Agents">⚙</button>
      </span>
    </div>
    <div class="agents-list" id="agents-list" role="listbox" aria-label="Agents"></div>
  `;

  const list = container.querySelector<HTMLElement>("#agents-list")!;
  const countEl = container.querySelector<HTMLElement>(".agents-attention-count")!;
  container.querySelector("#agents-manage-btn")!.addEventListener("click", () => showAgentManager());
  container.querySelector("#agents-dispatch-btn")!.addEventListener("click", () => showDispatchDialog());

  // ↑/↓/Home/End move focus between rows; Enter/Space activate (makeInteractive).
  list.addEventListener("keydown", (e) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    const rows = Array.from(list.querySelectorAll<HTMLElement>(".agent-row"));
    if (rows.length === 0) return;
    const cur = rows.indexOf((e.target as HTMLElement).closest(".agent-row") as HTMLElement);
    const next =
      e.key === "Home" ? 0
      : e.key === "End" ? rows.length - 1
      : e.key === "ArrowDown" ? Math.min(rows.length - 1, cur + 1)
      : Math.max(0, cur - 1);
    e.preventDefault();
    rows[next].focus();
  });

  function render() {
    const rows = appState.agentRows;
    const needing = attentionRows(rows).length;
    countEl.textContent = String(needing);
    countEl.hidden = needing === 0;
    countEl.title = `${needing} agent${needing === 1 ? "" : "s"} need${needing === 1 ? "s" : ""} you — ${getShortcutKey("jump-attention")} jumps there`;

    // Agent events rebuild the list continuously; keep the scroll position
    // and the keyboard focus (by tab id — rows are rebuilt from scratch).
    const prevScroll = list.scrollTop;
    const focusedId = (document.activeElement as HTMLElement | null)?.closest?.(".agent-row")
      ?.getAttribute("data-tab-id") ?? null;
    list.innerHTML = "";
    if (rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ui-empty";
      empty.innerHTML = `
        <p>No agents running</p>
        <button data-variant="secondary" data-size="sm" class="ui-btn ui-empty-cta">Dispatch Agent</button>
      `;
      empty.querySelector(".ui-empty-cta")!.addEventListener("click", () => showDispatchDialog());
      list.appendChild(empty);
      return;
    }

    const now = Date.now();
    for (const row of rows) {
      const el = document.createElement("div");
      const isCurrent =
        row.workspace_idx === appState.activeWorkspace &&
        appState.activeWs?.tabs[appState.activeWs.activeTab]?.id === row.tab_id;
      el.className = `agent-row${isCurrent ? " current" : ""}`;
      el.dataset.tabId = row.tab_id;

      // Status vocabulary shared with the tab bar / status bar; a tab without
      // a structured channel falls back to alive/exited (same as the TUI).
      const v = row.status
        ? cliAgentStatusView(row.status, row.attention)
        : row.alive
          ? { glyph: "●", label: "alive", color: "var(--text-muted)" }
          : { glyph: "○", label: "exited", color: "var(--text-muted)" };
      const elapsed = liveElapsedSecs(row, appState.agentRowsFetchedAt, now);

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
        <span class="agent-row-elapsed" title="Elapsed since the run started"${elapsed === null ? " hidden" : ""}>${elapsed === null ? "" : formatElapsed(elapsed)}</span>
        <span class="agent-row-status" style="color:${v.color}">${v.label}</span>
      `;
      el.title = row.summary ?? "";
      el.addEventListener("click", () => void jumpToAgentRow(row));
      makeInteractive(el, "option");
      el.setAttribute("aria-selected", String(isCurrent));
      list.appendChild(el);
    }
    list.scrollTop = prevScroll;
    if (focusedId) list.querySelector<HTMLElement>(`.agent-row[data-tab-id="${focusedId}"]`)?.focus();
  }

  /** Once a second, advance the elapsed labels in place — no rebuild, no
   *  IPC: the backend snapshot plus wall-clock since the fetch. */
  function tickElapsed() {
    const now = Date.now();
    for (const row of appState.agentRows) {
      const el = list.querySelector<HTMLElement>(`.agent-row[data-tab-id="${row.tab_id}"] .agent-row-elapsed`);
      if (!el) continue;
      const elapsed = liveElapsedSecs(row, appState.agentRowsFetchedAt, now);
      el.hidden = elapsed === null;
      el.textContent = elapsed === null ? "" : formatElapsed(elapsed);
    }
  }

  appState.on("agent-rows-changed", render);
  appState.on("active-workspace-changed", render);
  appState.on("active-tab-changed", render);
  setInterval(tickElapsed, ELAPSED_TICK_MS);

  render();
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
