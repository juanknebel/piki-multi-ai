/** Project state shared by the sidebar tree and the row menus.
 *
 *  This used to render a Projects *view* next to a flat Workspaces view; both
 *  are gone — `project-tree.ts` renders the one sidebar, built from
 *  `piki_core::projects::tree`. What stays here is the project list cache plus
 *  the mutations the tree and the menus call: membership toggles, delete, and
 *  adopting a plain-directory member as a workspace.
 */
import * as ipc from "../ipc";
import { appState } from "../state";
import type { Project } from "../types";
import { showConfirm } from "./confirm";
import { reportError, toast } from "./toast";

let projects: Project[] = [];

/** Swatch var for a palette index (indices are 0-based, tokens 1-based). */
export function projectSwatch(color: number): string {
  return `var(--project-swatch-${(color % 10) + 1})`;
}

/** Reload the project list and tell the tree to rebuild. */
export async function refreshProjects() {
  try {
    projects = await ipc.listProjects();
  } catch (err) {
    console.error("listProjects failed", err);
    projects = [];
  }
  appState.notifyProjectsChanged();
}

/** The projects as last loaded — read by the tree and the row menus so they
 *  don't each fetch. */
export function projectsSnapshot(): readonly Project[] {
  return projects;
}

export function projectById(id: number): Project | null {
  return projects.find((p) => p.id === id) ?? null;
}

/** Add or remove one path from a project and persist it. Used by the sidebar
 *  menus, so membership can be changed without opening the dialog (the dialog
 *  remains the place to rename / recolour / reorder). */
export async function setProjectMembership(
  project: Project,
  path: string,
  member: boolean,
): Promise<void> {
  const has = project.members.some((m) => m.path === path);
  if (has === member) return;
  const members = member
    ? [...project.members, { path, kind: "Auto" as const }]
    : project.members.filter((m) => m.path !== path);
  try {
    await ipc.saveProject({ ...project, members });
    await refreshProjects();
    toast(
      member
        ? `Added to project "${project.name}"`
        : `Removed from project "${project.name}"`,
      "success",
    );
  } catch (err) {
    reportError("Failed to update project", err);
  }
}

/** Register `path` as a `Simple` workspace and return its index, or the index
 *  it already had. Membership stores only the path, so the directory row
 *  upgrades to a checkout row by itself once this lands. */
export async function adoptDirectory(path: string): Promise<number | null> {
  const existing = appState.workspaces.findIndex((w) => w.info.path === path);
  if (existing >= 0) return existing;
  const name = path.replace(/\/+$/, "").split("/").pop() || path;
  const info = await ipc.createWorkspace(name, "", "", path, "Simple", null);
  appState.addWorkspace(info);
  const idx = appState.workspaces.findIndex((w) => w.info.path === info.path);
  return idx >= 0 ? idx : null;
}

/** Delete confirm for a project. Only the group goes; what it grouped stays. */
export function confirmDeleteProject(project: Project) {
  showConfirm({
    bodyHtml: `
      <p>Delete project "<strong>${escapeHtml(project.name)}</strong>"?</p>
      <p>Only the group is removed — its repositories, worktrees and directories are untouched.</p>
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

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}
