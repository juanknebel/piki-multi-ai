import { appState, type SidebarView } from "../state";

const ACTIVITIES: { id: SidebarView; label: string; icon: string }[] = [
  {
    id: "explorer",
    label: "Explorer",
    icon: `<svg viewBox="0 0 24 24"><path d="M17.5 0h-9L7 1.5V6H2.5L1 7.5v15.07L2.5 24h12.07L16 22.57V18h4.7l1.3-1.43V4.5L17.5 0zm0 2.12l2.38 2.38H17.5V2.12zm-3 20.38h-12v-15H7v9.07L8.5 18h6v4.5zm6-6h-12v-15h6V6h6v10.5z"/></svg>`,
  },
  {
    id: "git",
    label: "Source Control",
    icon: `<svg viewBox="0 0 24 24"><path d="M21.007 8.222A3.738 3.738 0 0 0 15.045 5.2a3.737 3.737 0 0 0 1.156 6.583 2.988 2.988 0 0 1-2.668 1.67h-2.99a4.456 4.456 0 0 0-2.989 1.165V7.559a3.738 3.738 0 1 0-1.494 0v8.883a3.737 3.737 0 1 0 1.498.058 2.992 2.992 0 0 1 2.989-2.747h2.989a4.49 4.49 0 0 0 4.223-3.03 3.74 3.74 0 0 0 3.248-4.501zM7.773 3.27a2.24 2.24 0 1 1 0 4.48 2.24 2.24 0 0 1 0-4.48zm0 17.46a2.24 2.24 0 1 1 0-4.48 2.24 2.24 0 0 1 0 4.48zm9.483-9.48a2.24 2.24 0 1 1 0-4.48 2.24 2.24 0 0 1 0 4.48z"/></svg>`,
  },
];

export function renderActivityBar(container: HTMLElement) {
  container.innerHTML = "";

  const buttons = new Map<string, HTMLButtonElement>();

  ACTIVITIES.forEach((activity) => {
    const item = document.createElement("button");
    item.className = `activity-item${activity.id === appState.activeView ? " active" : ""}`;
    item.title = activity.label;
    item.dataset.id = activity.id;
    item.innerHTML = activity.icon;
    buttons.set(activity.id, item);

    item.addEventListener("click", () => {
      appState.setActiveView(activity.id);
    });

    container.appendChild(item);
  });

  // Badge for source control (change count)
  const gitBtn = buttons.get("git")!;
  const badge = document.createElement("span");
  badge.className = "activity-badge";
  badge.style.display = "none";
  gitBtn.appendChild(badge);

  function updateBadge() {
    const count = appState.activeWs?.changedFiles.length ?? 0;
    if (count > 0) {
      badge.textContent = count > 99 ? "99+" : String(count);
      badge.style.display = "";
    } else {
      badge.style.display = "none";
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
  updateBadge();
}
