/** Projects sidebar view: user-defined cross-cutting groups with a colour.
 *  Each project row expands into its members — workspaces (of any repo) and
 *  plain directories, referenced by path. Which one a member *is* gets
 *  resolved against the live workspace list at render time: a path with a
 *  registered workspace renders as a workspace row and jumps on click; any
 *  other path renders as a directory row and is adopted as a Simple
 *  workspace on click (idempotent — once adopted it upgrades by itself). */
import * as ipc from "../ipc";
import { appState } from "../state";
import { settingsStore } from "../settings";
import type { Project } from "../types";
import { icon } from "./icons";
import { branchLabel } from "../labels";
import { makeInteractive } from "./a11y";
import { openContextMenu } from "./context-menu";
import { showConfirm } from "./confirm";
import { reportError } from "./toast";
import { showProjectDialog } from "./dialogs/project-dialog";

const COLLAPSED_KEY = "projectsCollapsed";

let projects: Project[] = [];
let listEl: HTMLElement | null = null;

function collapsedIds(): Set<number> {
  return new Set(settingsStore.get<number[]>(COLLAPSED_KEY) ?? []);
}

function setCollapsed(ids: Set<number>) {
  settingsStore.patch(COLLAPSED_KEY, ids.size > 0 ? [...ids] : undefined);
}

/** Swatch var for a palette index (indices are 0-based, tokens 1-based). */
export function projectSwatch(color: number): string {
  return `var(--project-swatch-${(color % 10) + 1})`;
}

export async function refreshProjects() {
  try {
    projects = await ipc.listProjects();
  } catch (err) {
    console.error("listProjects failed", err);
    projects = [];
  }
  render();
}

/** Jump to the member's workspace, adopting the path as a Simple workspace
 *  first when no workspace is registered there yet. */
async function openMember(path: string) {
  try {
    let idx = appState.workspaces.findIndex((w) => w.info.path === path);
    if (idx < 0) {
      const name = path.replace(/\/+$/, "").split("/").pop() || path;
      const info = await ipc.createWorkspace(name, "", "", path, "Simple", null);
      appState.addWorkspace(info);
      idx = appState.workspaces.findIndex((w) => w.info.path === info.path);
      if (idx < 0) return;
    }
    const detail = await ipc.switchWorkspace(idx);
    appState.setActiveWorkspace(idx, detail);
  } catch (err) {
    reportError("Failed to open project member", err);
  }
}

function confirmDelete(project: Project) {
  showConfirm({
    bodyHtml: `
      <p>Delete project "<strong>${escapeHtml(project.name)}</strong>"?</p>
      <p>Only the group is removed — its workspaces and directories are untouched.</p>
    `,
    actions: [
      {
        label: "Delete project",
        kind: "danger",
        onSelect: () => {
          void (async () => {
            if (project.id === null) return;
            try {
              await ipc.deleteProject(project.id);
              await refreshProjects();
            } catch (err) {
              reportError("Failed to delete project", err);
            }
          })();
        },
      },
      { label: "Cancel", kind: "secondary" },
    ],
  });
}

function projectMenu(project: Project, x: number, y: number) {
  openContextMenu(x, y, [
    { label: "Edit Project", action: () => showProjectDialog(project, refreshProjects) },
    { label: "Delete Project", danger: true, action: () => confirmDelete(project) },
  ]);
}

function render() {
  const list = listEl;
  if (!list) return;

  const prevScroll = list.scrollTop;
  const active = document.activeElement as HTMLElement | null;
  const focusKey =
    active?.closest?.(".project-member")?.getAttribute("data-path") ??
    active?.closest?.(".project-row")?.getAttribute("data-project-id") ??
    null;

  list.innerHTML = "";

  if (projects.length === 0) {
    const empty = document.createElement("div");
    empty.className = "ui-empty";
    empty.innerHTML = `
      <p>No projects yet</p>
      <button data-variant="secondary" data-size="sm" class="ui-btn ui-empty-cta">New Project</button>
    `;
    empty
      .querySelector(".ui-empty-cta")!
      .addEventListener("click", () => showProjectDialog(null, refreshProjects));
    list.appendChild(empty);
    return;
  }

  const collapsed = collapsedIds();
  for (const project of projects) {
    const pid = project.id ?? -1;
    const isCollapsed = collapsed.has(pid);
    const swatch = projectSwatch(project.color);

    const row = document.createElement("div");
    row.className = "project-row";
    row.dataset.projectId = String(pid);
    row.innerHTML = `
      ${icon("chevron-right", { class: `project-chevron${isCollapsed ? "" : " expanded"}` })}
      <span class="project-dot" style="background:${swatch}"></span>
      <span class="project-name">${escapeHtml(project.name)}</span>
      <span class="project-count">${project.members.length}</span>
    `;
    row.addEventListener("click", () => {
      const next = collapsedIds();
      if (next.has(pid)) next.delete(pid);
      else next.add(pid);
      setCollapsed(next);
      render();
    });
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      projectMenu(project, e.clientX, e.clientY);
    });
    makeInteractive(row, "option");
    list.appendChild(row);

    if (isCollapsed) continue;
    for (const member of project.members) {
      const ws = appState.workspaces.find((w) => w.info.path === member.path);
      const el = document.createElement("div");
      el.className = `project-member${ws ? "" : " directory"}`;
      el.dataset.path = member.path;
      el.style.setProperty("--project-stripe", swatch);
      const label = ws
        ? escapeHtml(ws.info.name)
        : escapeHtml(member.path.replace(/\/+$/, "").split("/").pop() || member.path);
      // Branch lives INSIDE the name span (workspace-list pattern): the row
      // ellipsizes as one line, so a long branch truncates before it can
      // crush the member name or overflow the sidebar.
      const branch = ws?.branch
        ? ` <span class="project-member-branch">${icon("branch")} ${escapeHtml(branchLabel(ws.branch))}</span>`
        : "";
      el.innerHTML = `<span class="project-member-name">${label}${branch}</span>`;
      el.title = ws?.branch ? `${member.path} · ${ws.branch}` : member.path;
      el.addEventListener("click", () => void openMember(member.path));
      makeInteractive(el, "option");
      list.appendChild(el);
    }
  }

  list.scrollTop = prevScroll;
  if (focusKey) {
    list
      .querySelector<HTMLElement>(
        `.project-member[data-path="${CSS.escape(focusKey)}"], .project-row[data-project-id="${CSS.escape(focusKey)}"]`,
      )
      ?.focus();
  }
}

export function renderProjectsPanel(container: HTMLElement) {
  container.innerHTML = `
    <div class="sidebar-header">
      <span>Projects</span>
      <span class="agents-header-actions">
        <button data-variant="ghost" data-size="sm" class="sc-header-btn ui-btn" id="projects-add-btn" title="New Project" aria-label="New Project">${icon("plus")}</button>
      </span>
    </div>
    <div class="projects-list" id="projects-list" role="listbox" aria-label="Projects"></div>
  `;
  listEl = container.querySelector<HTMLElement>("#projects-list")!;

  container
    .querySelector("#projects-add-btn")!
    .addEventListener("click", () => showProjectDialog(null, refreshProjects));

  listEl.addEventListener("keydown", (e) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    const rows = Array.from(listEl!.querySelectorAll<HTMLElement>(".project-row, .project-member"));
    if (rows.length === 0) return;
    const cur = rows.indexOf(
      (e.target as HTMLElement).closest(".project-row, .project-member") as HTMLElement,
    );
    const next =
      e.key === "Home" ? 0
      : e.key === "End" ? rows.length - 1
      : e.key === "ArrowDown" ? Math.min(rows.length - 1, cur + 1)
      : Math.max(0, cur - 1);
    e.preventDefault();
    rows[next].focus();
  });

  // Member resolution (workspace vs directory, branch labels) depends on the
  // live workspace list — re-render when it changes.
  appState.on("workspaces-changed", render);
  appState.on("active-workspace-changed", render);
  void refreshProjects();
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}
