/** The sidebar: one tree, projects all the way down.
 *
 *  Three levels — project → repository → checkout — plus the synthetic
 *  PR-review bucket. A repository in no project is not wrapped in one: it
 *  renders at the top level after the projects, so every registered workspace
 *  is reachable without inventing a group for it. There is no separate flat
 *  workspace view; this replaced it.
 *
 *  Rows come straight from `piki_core::projects::tree::project_tree` through
 *  the `project_tree` command: which repo a checkout belongs to, what is
 *  collapsed and where the buckets go are decided once, in core, so this app
 *  and the TUI cannot drift apart. Only the text and the DOM are decided here.
 */
import { appState } from "../state";
import { makeInteractive } from "./a11y";
import * as ipc from "../ipc";
import { openContextMenu, type CtxItem } from "./context-menu";
import { switchToWorkspace } from "./workspace-actions";
import {
  showCreateWorktreeDialog,
  showWorkspaceDialog,
  showWorkspaceInfo,
} from "./dialogs/workspace-dialog";
import { showAgentManager } from "./dialogs/agent-dialog";
import { showMergeDialog } from "./dialogs/merge-dialog";
import { confirmDeleteWorkspace } from "./dialogs/delete-workspace";
import { showProjectDialog } from "./dialogs/project-dialog";
import { branchLabel } from "../labels";
import {
  adoptDirectory,
  confirmDeleteProject,
  projectById,
  projectSwatch,
  projectsSnapshot,
  refreshProjects,
  setProjectMembership,
} from "./projects-panel";
import { icon } from "./icons";
import { reportError } from "./toast";
import {
  actionableStatusView,
  agentStatusSeverity,
  type WorkspaceInfo,
} from "../types";

/** Display label for a synthetic bucket — core only hands back the tag.
 *  Mirrors the TUI's `bucket_label`. Only PR review has a header: loose
 *  repositories render at the top level with none. */
function bucketLabel(bucket: string): string {
  return bucket === "prReview" ? "PR review" : "";
}

/** Name + muted branch for a checkout row. Under a repo group the repo name is
 *  already on the header, so the child says which *branch* it is — that is the
 *  point of the tree. A **hoisted** row IS its repository (its group had a
 *  single checkout), so it is named the way the old flat sidebar named a clone:
 *  the repository folder, with the branch rendered separately and dimmed. Any
 *  other depth-1 row (a non-git workspace, a PR review) keeps its own name. */
function checkoutParts(
  info: WorkspaceInfo,
  branch: string | null,
  inGroup: boolean,
  hoisted: boolean,
): { name: string; branch: string | null } {
  if (hoisted) {
    const folder =
      info.source_repo.replace(/\/+$/, "").split("/").pop() ||
      info.source_repo_display ||
      info.name;
    return { name: folder, branch: branch ? branchLabel(branch) : null };
  }
  if (inGroup) {
    if (branch) return { name: branchLabel(branch), branch: null };
    const leaf = info.path.replace(/\/+$/, "").split("/").pop();
    return { name: leaf || info.name, branch: null };
  }
  return { name: info.name, branch: branch ? branchLabel(branch) : null };
}

/** Full, untruncated text for the row tooltip: what the row is called on the
 *  first line, where it lives on the second (the tooltip wraps the path
 *  instead of eliding it — see styles/tooltip.css). */
function rowTitle(info: WorkspaceInfo, branch: string | null): string {
  const b = branch ? ` · ${branch}` : "";
  return `${info.name}${b}\n${info.path}`;
}

/** Membership toggles for every project — the one-click way in and out of a
 *  project, so adding a repo doesn't mean opening the project dialog. */
function projectMenuItems(path: string): CtxItem[] {
  const projects = projectsSnapshot();
  if (projects.length === 0) return [];
  return [
    { separator: true },
    ...projects.map((project) => {
      const member = project.members.some((m) => m.path === path);
      return {
        label: `${member ? "Remove from" : "Add to"} project "${project.name}"`,
        action: () => void setProjectMembership(project, path, !member),
      };
    }),
  ];
}

/** The checkout row menu (right-click, or the row's `⋯`): Open / Merge, then
 *  membership, Delete last and red. Actions that only work on the active
 *  workspace (Merge) switch to it first. */
/** The project a checkout row sits under, so a branch created from it joins
 *  the same project. `null` when the row is a loose repository. */
function projectOfWorkspace(idx: number): number | null {
  const path = appState.workspaces[idx]?.info.path;
  if (!path) return null;
  const owner = projectsSnapshot().find((p) => p.members.some((m) => m.path === path));
  return owner?.id ?? null;
}

function checkoutMenuItems(idx: number): CtxItem[] {
  const ws = appState.workspaces[idx];
  if (!ws) return [];
  const info = ws.info;
  const isActive = idx === appState.activeWorkspace;
  const git = info.workspace_type !== "Simple";
  return [
    { label: "Open", disabled: isActive, action: () => void switchToWorkspace(idx) },
    { separator: true },
    { label: "Agents…", action: () => showAgentManager(idx) },
    { label: "Info", action: () => showWorkspaceInfo(idx) },
    { label: "Edit…", action: () => showWorkspaceDialog({ mode: "edit", editIndex: idx }) },
    ...(info.origin?.kind === "GitHub"
      ? [
          {
            label: "New Branch / Worktree…",
            action: () => showCreateWorktreeDialog(info, projectOfWorkspace(idx)),
          },
        ]
      : []),
    {
      label: "Merge / Rebase…",
      disabled: !git,
      action: async () => {
        if (!isActive) await switchToWorkspace(idx);
        if (appState.activeWorkspace === idx) showMergeDialog();
      },
    },
    ...projectMenuItems(info.path),
    { separator: true },
    { label: "Delete…", danger: true, action: () => void confirmDeleteWorkspace(idx) },
  ];
}

/** The repo row menu: branch off it, or add another repository to the project
 *  the group sits in. A repo with no checkout loaded can't be branched (the
 *  worktree dialog reads a loaded checkout's origin and prompt). */
function repoMenuItems(root: string, projectId: number | null): CtxItem[] {
  const checkout = appState.workspaces.find(
    (w) => w.info.source_repo === root && !w.info.ephemeral,
  );
  const github = checkout?.info.origin?.kind === "GitHub";
  return [
    {
      label: "New Branch / Worktree…",
      disabled: !checkout || !github,
      action: () => checkout && showCreateWorktreeDialog(checkout.info, projectId),
    },
    {
      label: "Add Repository…",
      disabled: projectId === null,
      action: () => showWorkspaceDialog({ mode: "create", addToProject: projectId }),
    },
  ];
}

/** The project header menu. A synthetic bucket has no stored project, so it
 *  only offers the one thing that applies: making a real one. */
function projectHeaderMenuItems(projectId: number | null): CtxItem[] {
  const project = projectId === null ? null : projectById(projectId);
  if (!project) {
    return [{ label: "New Project…", action: () => showProjectDialog(null, refreshProjects) }];
  }
  return [
    { label: "Add Repository…", action: () => showWorkspaceDialog({ mode: "create", addToProject: projectId }) },
    { separator: true },
    { label: "Edit Project…", action: () => showProjectDialog(project, refreshProjects) },
    { label: "New Project…", action: () => showProjectDialog(null, refreshProjects) },
    { separator: true },
    { label: "Delete Project…", danger: true, action: () => confirmDeleteProject(project) },
  ];
}

export function renderProjectTree(container: HTMLElement) {
  const collapsedGroups = new Set<string>();
  // Rows straight from core, refreshed whenever the workspace list, the
  // project list or the collapse state changes. Cached so render() stays
  // synchronous for its many event-driven callers.
  let rows: ipc.ProjectTreeRow[] = [];
  let rowsError = false;

  ipc
    .getCollapsedGroups()
    .then((groups) => {
      for (const g of groups) collapsedGroups.add(g);
      return refreshRows();
    })
    .catch(() => {});

  async function refreshRows() {
    try {
      rows = await ipc.projectTree();
      rowsError = false;
    } catch (err) {
      console.error("Failed to load the project tree:", err);
      rows = [];
      rowsError = true;
    }
    render();
  }

  /** Worst (status, attention) among the agents of the workspaces `pick`
   *  accepts, or null when none reports. Severity order shared with core and
   *  the TUI; reads `appState.agentRows`, same source as the Agents panel. */
  function agentRollup(pick: (idx: number) => boolean) {
    let best: { status: import("../types").CliAgentStatus; attention: boolean } | null = null;
    let bestSev = 0;
    for (const row of appState.agentRows) {
      if (!row.status || !pick(row.workspace_idx)) continue;
      const sev = agentStatusSeverity(row.status, row.attention);
      if (best === null || sev > bestSev) {
        best = { status: row.status, attention: row.attention };
        bestSev = sev;
      }
    }
    return best;
  }

  /** Worst agent state among the workspaces a collapsed row hides, so an agent
   *  waiting for permission can't vanish behind a chevron. The backend says
   *  WHICH workspaces those are (`row.hidden`, from
   *  `projects::tree::hidden_checkouts`); deriving it here from `source_repo`
   *  would answer for checkouts nobody put in the project. */
  function hiddenRollup(hidden: number[]) {
    if (hidden.length === 0) return null;
    const set = new Set(hidden);
    return agentRollup((i) => set.has(i));
  }

  async function persistCollapsed() {
    // Await the write: the backend resolves collapse state from storage, so
    // re-fetching before the save lands renders the OLD state and the click
    // appears to do nothing.
    try {
      await ipc.setCollapsedGroups([...collapsedGroups]);
    } catch (err) {
      console.error("Failed to save collapsed groups:", err);
    }
    void refreshRows();
  }

  function toggleGroup(key: string) {
    if (collapsedGroups.has(key)) collapsedGroups.delete(key);
    else collapsedGroups.add(key);
    void persistCollapsed();
  }

  function render() {
    const workspaces = appState.workspaces;
    const activeIdx = appState.activeWorkspace;

    // Frequent agent events rebuild the list; keep the scroll position.
    const prevScroll = container.scrollTop;
    container.innerHTML = "";

    const header = document.createElement("div");
    header.className = "sidebar-header";
    header.innerHTML = `
      <span>PROJECTS</span>
      <span class="agents-header-actions">
        <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" id="project-new-btn" title="New Project" aria-label="New Project">${icon("projects")}</button>
        <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" id="ws-create-btn" title="Add Repository / Workspace" aria-label="Add Repository">${icon("plus")}</button>
      </span>
    `;
    header.querySelector("#project-new-btn")!.addEventListener("click", (e) => {
      e.stopPropagation();
      showProjectDialog(null, refreshProjects);
    });
    header.querySelector("#ws-create-btn")!.addEventListener("click", (e) => {
      e.stopPropagation();
      showWorkspaceDialog({ mode: "create" });
    });
    container.appendChild(header);

    if (rows.length === 0 && rowsError) {
      const error = document.createElement("div");
      error.className = "ui-empty";
      error.innerHTML = `
        <p>Couldn't load the project tree</p>
        <button data-variant="secondary" data-size="sm" class="ui-btn ui-empty-cta">Retry</button>
      `;
      error.querySelector(".ui-empty-cta")!.addEventListener("click", () => void refreshRows());
      container.appendChild(error);
      return;
    }

    if (rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ui-empty";
      empty.innerHTML = `
        <p>No projects yet</p>
        <button data-variant="primary" data-size="sm" class="ui-btn ui-empty-cta">New Project</button>
      `;
      empty
        .querySelector(".ui-empty-cta")!
        .addEventListener("click", () => showProjectDialog(null, refreshProjects));
      container.appendChild(empty);
      return;
    }

    const list = document.createElement("div");
    list.className = "projects-list";
    list.setAttribute("role", "listbox");
    list.setAttribute("aria-label", "Projects");

    /** A project or bucket header. */
    function buildProjectRow(row: Extract<ipc.ProjectTreeRow, { type: "project" }>): HTMLElement {
      const el = document.createElement("div");
      const name = row.bucket === "project" ? row.name : bucketLabel(row.bucket);
      const swatch = row.color !== null ? projectSwatch(row.color) : "var(--text-muted)";
      const rollup = row.collapsed ? hiddenRollup(row.hidden) : null;
      const rollupView = rollup && actionableStatusView(rollup.status, rollup.attention);
      el.className = `project-row${row.bucket === "project" ? "" : " bucket"}`;
      if (row.project_id !== null) el.dataset.projectId = String(row.project_id);
      el.innerHTML = `
        ${icon("chevron-right", { class: `project-chevron${row.collapsed ? "" : " expanded"}` })}
        <span class="project-dot" style="background:${swatch}"></span>
        <span class="project-name">${escapeHtml(name)}</span>
        ${rollupView ? `<span class="workspace-agent-glyph" style="color:${rollupView.color}" title="Agent ${rollupView.label}">${icon(rollupView.icon)}</span>` : ""}
      `;
      el.title = name;
      el.addEventListener("click", () => toggleGroup(row.key));
      el.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        openContextMenu(e.clientX, e.clientY, projectHeaderMenuItems(row.project_id));
      });
      makeInteractive(el, "option");
      return el;
    }

    /** A repository group — synthetic, so the only state it owns is collapse. */
    function buildRepoRow(
      row: Extract<ipc.ProjectTreeRow, { type: "repo" }>,
      projectId: number | null,
    ): HTMLElement {
      const el = document.createElement("div");
      const rollup = row.collapsed ? hiddenRollup(row.hidden) : null;
      const rollupView = rollup && actionableStatusView(rollup.status, rollup.attention);
      el.className = "repo-row";
      el.dataset.repo = row.root;
      el.dataset.depth = String(row.depth);
      el.innerHTML = `
        <span class="workspace-gutter">${icon("chevron-right", { class: `group-chevron${row.collapsed ? " collapsed" : ""}` })}</span>
        ${icon("branch", { class: "repo-icon" })}
        <span class="repo-name">${escapeHtml(row.display)}</span>
        ${row.checkouts === 0 ? `<span class="repo-empty">not open</span>` : ""}
        ${rollupView ? `<span class="workspace-agent-glyph" style="color:${rollupView.color}" title="Agent ${rollupView.label}">${icon(rollupView.icon)}</span>` : ""}
        <span class="workspace-actions">
          <button data-variant="ghost" data-icon class="ws-action-btn ui-btn" data-action="menu" title="Repository menu" aria-label="Repository menu" aria-haspopup="menu">${icon("more")}</button>
        </span>
      `;
      el.title = `${row.display}\n${row.root}`;
      el.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest(".ws-action-btn")) return;
        toggleGroup(row.key);
      });
      const menuBtn = el.querySelector<HTMLButtonElement>('[data-action="menu"]')!;
      menuBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const r = menuBtn.getBoundingClientRect();
        openContextMenu(r.left, r.bottom + 2, repoMenuItems(row.root, projectId));
      });
      el.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        openContextMenu(e.clientX, e.clientY, repoMenuItems(row.root, projectId));
      });
      makeInteractive(el, "option");
      return el;
    }

    /** One checkout row, or `null` if its workspace fell out of the list
     *  mid-fetch (rows are computed async, from the same list this renders). */
    function buildCheckoutRow(
      row: Extract<ipc.ProjectTreeRow, { type: "checkout" }>,
    ): HTMLElement | null {
      const idx = row.index;
      if (idx >= workspaces.length) return null;
      const ws = workspaces[idx];
      const info = ws.info;
      const el = document.createElement("div");
      el.className = [
        "workspace-item",
        idx === activeIdx && "active",
        // Keyed on in_group, never on depth: the same child is one level
        // further out under a loose repository.
        row.in_group && "grouped",
        // A hoisted row IS its repository, so it carries the repo's shape.
        row.hoisted && "hoisted-repo",
      ]
        .filter(Boolean)
        .join(" ");
      el.dataset.idx = String(idx);
      el.dataset.depth = String(row.depth);

      const rollup = agentRollup((wi) => wi === idx);
      const rollupView = rollup && actionableStatusView(rollup.status, rollup.attention);
      const agentGlyph = rollupView
        ? `<span class="workspace-agent-glyph" style="color:${rollupView.color}" title="Agent ${rollupView.label}">${icon(rollupView.icon)}</span>`
        : "";
      const attentionDot = ws.needsAttention
        ? `<span class="workspace-attention" title="Needs attention">${icon("dot")}</span>`
        : "";
      const restoredMark = ws.restoredUnvisited
        ? `<span class="workspace-restored" title="Sessions restored from the daemon — not visited yet">${icon("history")}</span>`
        : "";

      const { name, branch } = checkoutParts(info, ws.branch, row.in_group, row.hoisted);
      const branchHtml = branch
        ? ` <span class="workspace-branch">${escapeHtml(branch)}</span>`
        : "";
      // The gutter stays on every row so labels line up whatever the kind; a
      // checkout has nothing to collapse, so its slot is empty. A hoisted row
      // takes the repo icon instead — it is the repository, and reading as one
      // thing in the group above and another below would be a lie.
      el.innerHTML = `
        <span class="workspace-gutter"></span>
        ${row.hoisted ? icon("branch", { class: "repo-icon" }) : ""}
        <span class="workspace-name">${escapeHtml(name)}${branchHtml}</span>
        ${info.ephemeral ? `<span class="workspace-pr">PR</span>` : ""}
        ${agentGlyph}
        ${attentionDot}
        ${restoredMark}
        <span class="workspace-actions">
          <button data-variant="ghost" data-icon class="ws-action-btn ui-btn" data-action="menu" title="Workspace menu" aria-label="Workspace menu" aria-haspopup="menu">${icon("more")}</button>
        </span>
        <span class="workspace-status ${getStatusClass(ws.status)}">${getStatusIcon(ws.status)}</span>
      `;
      el.title = rowTitle(info, ws.branch);

      el.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest(".ws-action-btn")) return;
        void switchToWorkspace(idx);
      });
      const menuBtn = el.querySelector<HTMLButtonElement>('[data-action="menu"]')!;
      menuBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const r = menuBtn.getBoundingClientRect();
        openContextMenu(r.left, r.bottom + 2, checkoutMenuItems(idx));
      });
      el.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        openContextMenu(e.clientX, e.clientY, checkoutMenuItems(idx));
      });

      makeInteractive(el);
      return el;
    }

    /** A member path with no workspace at it: click adopts it. */
    function buildDirRow(
      row: Extract<ipc.ProjectTreeRow, { type: "dir" }>,
      projectId: number | null,
    ): HTMLElement {
      const el = document.createElement("div");
      el.className = "project-member directory";
      el.dataset.path = row.path;
      el.dataset.depth = "1";
      const leaf = row.path.replace(/\/+$/, "").split("/").pop() || row.path;
      el.innerHTML = `<span class="project-member-name">${escapeHtml(leaf)}</span>`;
      el.title = `${leaf}\n${row.path}`;
      el.addEventListener("click", () => void adoptAndOpen(row.path));
      el.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        const project = projectId === null ? null : projectById(projectId);
        openContextMenu(e.clientX, e.clientY, [
          { label: "Open (adopt as workspace)", action: () => void adoptAndOpen(row.path) },
          ...(project
            ? [
                { separator: true },
                {
                  label: `Remove from "${project.name}"`,
                  action: () => void setProjectMembership(project, row.path, false),
                },
                { label: "Edit Project…", action: () => showProjectDialog(project, refreshProjects) },
              ]
            : []),
        ]);
      });
      makeInteractive(el, "option");
      return el;
    }

    // Walk the rows. A repo group and its contiguous checkouts are wrapped in
    // `.ws-family` so the CSS rail can connect them with one absolutely
    // positioned line (`git log --graph` style) instead of per-row plumbing.
    let projectId: number | null = null;
    for (let i = 0; i < rows.length; i++) {
      const row = rows[i];
      if (row.type === "project") {
        projectId = row.project_id;
        list.appendChild(buildProjectRow(row));
        continue;
      }
      if (row.type === "dir") {
        list.appendChild(buildDirRow(row, projectId));
        continue;
      }
      if (row.type === "checkout") {
        const el = buildCheckoutRow(row);
        if (el) list.appendChild(el);
        continue;
      }
      // repo
      const repoEl = buildRepoRow(row, projectId);
      const children: HTMLElement[] = [];
      let j = i + 1;
      while (j < rows.length) {
        const next = rows[j];
        if (next.type !== "checkout" || !next.in_group) break;
        const childEl = buildCheckoutRow(next);
        if (childEl) children.push(childEl);
        j++;
      }
      if (children.length > 0) {
        const family = document.createElement("div");
        family.className = "ws-family";
        family.appendChild(repoEl);
        for (const el of children) family.appendChild(el);
        const rail = document.createElement("div");
        rail.className = "ws-family-rail";
        // The trunk hangs under the repo row's chevron, wherever its depth put
        // it (a loose repository sits one level further out than a project's).
        family.style.setProperty("--rail-left", `${25 + 14 * row.depth}px`);
        family.appendChild(rail);
        list.appendChild(family);
      } else {
        list.appendChild(repoEl);
      }
      i = j - 1;
    }

    list.addEventListener("keydown", (e) => {
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
      const items = Array.from(
        list.querySelectorAll<HTMLElement>(".project-row, .repo-row, .workspace-item, .project-member"),
      );
      if (items.length === 0) return;
      const cur = items.indexOf(
        (e.target as HTMLElement).closest(
          ".project-row, .repo-row, .workspace-item, .project-member",
        ) as HTMLElement,
      );
      const next =
        e.key === "Home" ? 0
        : e.key === "End" ? items.length - 1
        : e.key === "ArrowDown" ? Math.min(items.length - 1, cur + 1)
        : Math.max(0, cur - 1);
      e.preventDefault();
      items[next].focus();
    });

    container.appendChild(list);
    container.scrollTop = prevScroll;
  }

  async function adoptAndOpen(path: string) {
    try {
      const idx = await adoptDirectory(path);
      if (idx !== null) await switchToWorkspace(idx);
      await refreshRows();
    } catch (err) {
      reportError("Failed to open project member", err);
    }
  }

  appState.on("workspaces-changed", () => void refreshRows());
  // The tree's shape depends on the project list too.
  appState.on("projects-changed", () => void refreshRows());
  appState.on("active-workspace-changed", render);
  appState.on("workspace-attention-changed", render);
  // Rows label a worktree BY its branch and a clone with it.
  appState.on("workspace-branch-changed", render);
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
  if (status === "Busy") return icon("dot");
  if (status === "Done") return icon("check");
  if (typeof status === "object" && "Error" in status) return icon("close");
  return "";
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
