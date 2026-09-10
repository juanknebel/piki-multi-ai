import { appState, type SidebarView } from "../state";
import { attentionRows } from "../agent-attention";
import { getShortcutKey } from "../shortcuts";
import { icon, type IconName } from "./icons";

// Sidebar views first — Workspaces, then Projects (the container that
// groups workspaces across repos) — then the workspace-scoped Files and
// Source Control, then the global tools. Workspaces / Projects / Files /
// git are switchable sidebar views; Agents opens the profile-manager
// dialog; Kanban / API / Web Preview open workspace tabs.
const ACTIVITIES: { id: SidebarView; label: string; icon: IconName }[] = [
  { id: "workspaces", label: "Workspaces", icon: "workspaces" },
  { id: "projects", label: "Projects", icon: "projects" },
  { id: "files", label: "Files", icon: "folder" },
  { id: "git", label: "Source Control", icon: "branch" },
  { id: "agents", label: "Manage Agents", icon: "agents" },
  { id: "kanban", label: "Kanban Board", icon: "kanban" },
  { id: "api", label: "API Explorer", icon: "api" },
  { id: "web-preview", label: "Web Preview", icon: "browser" },
];

export function renderActivityBar(container: HTMLElement) {
  container.innerHTML = "";

  const buttons = new Map<string, HTMLButtonElement>();

  ACTIVITIES.forEach((activity) => {
    const item = document.createElement("button");
    item.className = `activity-item${activity.id === appState.activeView ? " active" : ""}`;
    item.title = activity.label;
    item.dataset.id = activity.id;
    item.innerHTML = icon(activity.icon);
    buttons.set(activity.id, item);

    item.addEventListener("click", () => {
      appState.setActiveView(activity.id);
    });

    container.appendChild(item);
  });

  // Badge for source control: change count, plus ↑N when local commits
  // haven't been pushed (aheadBehind refreshes together with changedFiles,
  // so `files-changed` covers both).
  const gitBtn = buttons.get("git")!;
  const badge = document.createElement("span");
  badge.className = "activity-badge";
  badge.style.display = "none";
  gitBtn.appendChild(badge);

  const cap = (n: number) => (n > 99 ? "99+" : String(n));

  function updateBadge() {
    const ws = appState.activeWs;
    const count = ws?.changedFiles.length ?? 0;
    const ahead = ws?.aheadBehind?.[0] ?? 0;
    if (count > 0 || ahead > 0) {
      const parts: string[] = [];
      if (count > 0) parts.push(cap(count));
      if (ahead > 0) parts.push(`↑${cap(ahead)}`);
      badge.textContent = parts.join(" ");
      badge.title = [
        count > 0 ? `${count} change${count === 1 ? "" : "s"}` : "",
        ahead > 0 ? `${ahead} commit${ahead === 1 ? "" : "s"} to push` : "",
      ]
        .filter(Boolean)
        .join(" · ");
      badge.style.display = "";
    } else {
      badge.style.display = "none";
    }
  }

  // Badge for the Workspaces icon: agents needing you (all workspaces) — the
  // Agents panel lives in the sidebar, so this keeps the signal visible
  // while the sidebar is hidden or another view is up.
  const workspacesBtn = buttons.get("workspaces")!;
  const attentionBadge = document.createElement("span");
  attentionBadge.className = "activity-badge activity-badge--attention";
  attentionBadge.style.display = "none";
  workspacesBtn.appendChild(attentionBadge);

  function updateAttentionBadge() {
    const n = attentionRows(appState.agentRows).length;
    if (n > 0) {
      attentionBadge.textContent = n > 99 ? "99+" : String(n);
      attentionBadge.title = `${n} agent${n === 1 ? "" : "s"} need${n === 1 ? "s" : ""} you — ${getShortcutKey("jump-attention")} jumps there`;
      attentionBadge.style.display = "";
    } else {
      attentionBadge.style.display = "none";
    }
  }

  function updateActive() {
    for (const [id, btn] of buttons) {
      btn.classList.toggle("active", id === appState.activeView);
    }
  }

  appState.on("view-changed", updateActive);
  appState.on("files-changed", updateBadge);
  appState.on("active-workspace-changed", updateBadge);
  appState.on("agent-rows-changed", updateAttentionBadge);
  updateBadge();
  updateAttentionBadge();
}
