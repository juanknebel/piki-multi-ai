import { appState } from "../state";
import { makeInteractive } from "./a11y";
import * as ipc from "../ipc";
import { reportError } from "./toast";
import { openContextMenu, type CtxItem } from "./context-menu";
import {
  showCreateWorktreeDialog,
  showWorkspaceDialog,
  showWorkspaceInfo,
} from "./dialogs/workspace-dialog";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showMergeDialog } from "./dialogs/merge-dialog";
import { confirmDeleteWorkspace } from "./dialogs/delete-workspace";
import { branchLabel } from "../labels";
import {
  actionableStatusView,
  agentStatusSeverity,
  type WorkspaceInfo,
} from "../types";

/** Label for a row. Structure (families, the PR-review group, collapse
 *  state) is decided by the backend; only the text is decided here.
 *  Worktrees are named by their branch, a clone by its repo folder. */
function rowLabel(info: WorkspaceInfo, branch: string | null): string {
  if (info.workspace_type === "Worktree") return branch ? branchLabel(branch) : info.name;
  const folder =
    info.source_repo.replace(/\/+$/, "").split("/").pop() ||
    info.source_repo_display ||
    info.name;
  return branch ? `${folder} (${branchLabel(branch)})` : folder;
}

/** Full, untruncated text for the row tooltip. */
function rowTitle(info: WorkspaceInfo, branch: string | null): string {
  const b = branch ? ` · ⎇ ${branch}` : "";
  return `${info.name}${b}\n${info.path}`;
}

async function switchTo(idx: number) {
  try {
    const detail = await ipc.switchWorkspace(idx);
    appState.setActiveWorkspace(idx, detail);
  } catch (err) {
    reportError("Failed to switch workspace", err);
  }
}

/** The workspace row menu (right-click, or the row's `⋯`): everything the
 *  old hover buttons did plus Open / Merge, Delete last and red. Actions
 *  that only work on the active workspace (Merge) switch to it first. */
function workspaceMenuItems(idx: number): CtxItem[] {
  const ws = appState.workspaces[idx];
  if (!ws) return [];
  const info = ws.info;
  const isActive = idx === appState.activeWorkspace;
  const git = info.workspace_type !== "Simple";
  return [
    { label: "Open", disabled: isActive, action: () => void switchTo(idx) },
    { separator: true },
    { label: "Agents…", action: () => showAgentManager(idx) },
    { label: "Info", action: () => showWorkspaceInfo(idx) },
    { label: "Edit…", action: () => showWorkspaceDialog({ mode: "edit", editIndex: idx }) },
    ...(info.origin?.kind === "GitHub"
      ? [{ label: "Create Worktree…", action: () => showCreateWorktreeDialog(info) }]
      : []),
    {
      label: "Merge / Rebase…",
      disabled: !git,
      action: async () => {
        if (!isActive) await switchTo(idx);
        if (appState.activeWorkspace === idx) showMergeDialog();
      },
    },
    { separator: true },
    { label: "Delete…", danger: true, action: () => void confirmDeleteWorkspace(idx) },
  ];
}

export function renderWorkspaceList(container: HTMLElement) {
  const collapsedGroups = new Set<string>();
  // Grouped rows straight from `core::workspace::sidebar_rows`, refreshed
  // whenever the workspace list or the collapse state changes. Cached so
  // render() stays synchronous for its many event-driven callers.
  let rows: ipc.SidebarRow[] = [];

  ipc
    .getCollapsedGroups()
    .then((groups) => {
      for (const g of groups) collapsedGroups.add(g);
      return refreshRows();
    })
    .catch(() => {});

  let rowsError = false;

  async function refreshRows() {
    try {
      rows = await ipc.sidebarRows();
      rowsError = false;
    } catch (err) {
      console.error("Failed to load sidebar rows:", err);
      rows = [];
      rowsError = true;
    }
    render();
  }

  /** Worst (status, attention) among the agents of `indices`, or null when
   *  none reports. Severity order shared with core / the TUI. Reads the
   *  shared `appState.agentRows` (same source as the Agents panel). */
  function agentRollup(indices: (idx: number) => boolean) {
    let best: { status: import("../types").CliAgentStatus; attention: boolean } | null = null;
    let bestSev = 0;
    for (const row of appState.agentRows) {
      if (!row.status || !indices(row.workspace_idx)) continue;
      const sev = agentStatusSeverity(row.status, row.attention);
      if (best === null || sev > bestSev) {
        best = { status: row.status, attention: row.attention };
        bestSev = sev;
      }
    }
    return best;
  }

  function persistCollapsed() {
    ipc.setCollapsedGroups([...collapsedGroups]).catch(() => {});
    // The backend resolves collapse state, so re-fetch rather than
    // recompute here.
    void refreshRows();
  }

  function render() {
    const workspaces = appState.workspaces;
    const activeIdx = appState.activeWorkspace;

    // Frequent agent events rebuild the list; keep the scroll position.
    const prevScroll = container.scrollTop;
    container.innerHTML = "";

    // Header with create button
    const header = document.createElement("div");
    header.className = "sidebar-header";
    header.innerHTML = `
      <span>WORKSPACES</span>
      <button class="sc-header-btn" id="ws-create-btn" title="Create Workspace">+</button>
    `;
    header.querySelector("#ws-create-btn")!.addEventListener("click", (e) => {
      e.stopPropagation();
      showWorkspaceDialog({ mode: "create" });
    });
    container.appendChild(header);

    if (workspaces.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-message";
      empty.innerHTML = `
        <p>No workspaces yet</p>
        <button class="dialog-btn dialog-btn-primary empty-cta">Create Workspace</button>
      `;
      empty.querySelector(".empty-cta")!.addEventListener("click", () => {
        showWorkspaceDialog({ mode: "create" });
      });
      container.appendChild(empty);
      return;
    }

    // Workspaces exist but the grouped rows failed to load — say so instead
    // of presenting an inexplicably empty sidebar.
    if (rows.length === 0 && rowsError) {
      const error = document.createElement("div");
      error.className = "empty-message";
      error.innerHTML = `
        <p>Couldn't load workspaces</p>
        <button class="dialog-btn dialog-btn-secondary empty-cta">Retry</button>
      `;
      error.querySelector(".empty-cta")!.addEventListener("click", () => void refreshRows());
      container.appendChild(error);
      return;
    }

    for (const row of rows) {
      if (row.type === "prReviewHeader") {
        const header = document.createElement("div");
        header.className = "group-header";
        header.innerHTML = `
          <svg class="group-chevron${row.collapsed ? " collapsed" : ""}" viewBox="0 0 16 16">
            <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
          </svg>
          <span class="group-label">PR Review</span>
        `;
        const key = row.family_key;
        header.addEventListener("click", () => {
          if (collapsedGroups.has(key)) collapsedGroups.delete(key);
          else collapsedGroups.add(key);
          persistCollapsed();
        });
        makeInteractive(header);
        container.appendChild(header);
        continue;
      }

      const idx = row.index;
      // Rows are built from the same list this renders, but the fetch is
      // async — skip anything the list no longer has.
      if (idx >= workspaces.length) continue;
      const info = workspaces[idx].info;
      const item = document.createElement("div");
      item.className = `workspace-item${idx === activeIdx ? " active" : ""}${row.kind === "child" ? " grouped" : ""}`;
      item.dataset.idx = String(idx);

      const ws = workspaces[idx];
      const statusClass = getStatusClass(ws.status);

      // Agent-status rollup: a collapsed parent aggregates its hidden family
      // (rows sharing its family key = source_repo) so a needs-permission
      // agent can't hide behind a folded worktree group; a visible row only
      // reports its own agents. Only actionable states surface (TUI rule).
      const familyKey = row.kind === "parent" && row.collapsed ? row.family_key : null;
      const rollup = agentRollup((wi) =>
        familyKey !== null
          ? workspaces[wi]?.info.source_repo === familyKey
          : wi === idx,
      );
      const rollupView = rollup && actionableStatusView(rollup.status, rollup.attention);
      const agentGlyph = rollupView
        ? `<span class="workspace-agent-glyph" style="color:${rollupView.color}" title="Agent ${rollupView.label}">${rollupView.glyph}</span>`
        : "";

      const attentionDot = ws.needsAttention
        ? '<span class="workspace-attention" title="Needs attention">●</span>'
        : "";
      const restoredMark = ws.restoredUnvisited
        ? '<span class="workspace-restored" title="Sessions restored from the daemon — not visited yet">↺</span>'
        : "";

      const chevron = row.kind === "parent"
        ? `<svg class="group-chevron${row.collapsed ? " collapsed" : ""}" viewBox="0 0 16 16">
             <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
           </svg>`
        : "";

      item.innerHTML = `
        ${chevron}
        ${idx === activeIdx ? '<span class="workspace-active-marker"></span>' : ""}
        <span class="workspace-name">${escapeHtml(rowLabel(info, ws.branch))}</span>
        ${agentGlyph}
        ${attentionDot}
        ${restoredMark}
        <span class="workspace-actions">
          <button class="ws-action-btn" data-action="menu" title="Workspace menu" aria-label="Workspace menu" aria-haspopup="menu">⋯</button>
        </span>
        <span class="workspace-status ${statusClass}">${getStatusIcon(ws.status)}</span>
      `;
      item.title = rowTitle(info, ws.branch);

      // Click the chevron to toggle collapse without switching workspace.
      if (row.kind === "parent" && row.family_key) {
        const key = row.family_key;
        item.querySelector(".group-chevron")!.addEventListener("click", (e) => {
          e.stopPropagation();
          if (collapsedGroups.has(key)) {
            collapsedGroups.delete(key);
          } else {
            collapsedGroups.add(key);
          }
          persistCollapsed();
        });
      }

      // Click to switch workspace
      item.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest(".ws-action-btn")) return;
        if ((e.target as HTMLElement).closest(".group-chevron")) return;
        void switchTo(idx);
      });

      // One menu for every row action: the `⋯` button and right-click.
      const menuBtn = item.querySelector<HTMLButtonElement>('[data-action="menu"]')!;
      menuBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const r = menuBtn.getBoundingClientRect();
        openContextMenu(r.left, r.bottom + 2, workspaceMenuItems(idx));
      });
      item.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        openContextMenu(e.clientX, e.clientY, workspaceMenuItems(idx));
      });

      makeInteractive(item);
      container.appendChild(item);
    }
    container.scrollTop = prevScroll;
  }

  appState.on("workspaces-changed", () => void refreshRows());
  appState.on("active-workspace-changed", render);
  appState.on("workspace-attention-changed", render);
  // Agent lifecycle events, tab churn and tab switches (which acknowledge
  // attention backend-side) all land in `appState.agentRows`.
  appState.on("agent-rows-changed", render);
  render();
}

function getStatusClass(status: import("../types").WorkspaceStatus): string {
  if (typeof status === "string") return status.toLowerCase();
  return "error";
}

function getStatusIcon(status: import("../types").WorkspaceStatus): string {
  if (status === "Busy") return "●";
  if (status === "Done") return "✓";
  if (typeof status === "object" && "Error" in status) return "✕";
  return "";
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
