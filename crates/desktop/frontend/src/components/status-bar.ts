import { appState } from "../state";
import { getProviderLabel, type FileStatus } from "../types";
import { showAboutDialog } from "./dialogs/about-dialog";

const STAGED_STATUSES: FileStatus[] = ["Staged", "Added", "Renamed", "StagedModified"];

export function renderStatusBar(container: HTMLElement) {
  function render() {
    const ws = appState.activeWs;
    container.innerHTML = "";

    // App name (clickable → About)
    const appName = document.createElement("div");
    appName.className = "status-item clickable";
    appName.textContent = "Piki Desktop";
    appName.style.fontWeight = "600";
    appName.style.color = "var(--accent-primary)";
    appName.addEventListener("click", showAboutDialog);
    container.appendChild(appName);

    // Left side
    const branch = ws?.info.branch ?? "—";
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
        const label = getProviderLabel(tab.provider);
        const alive = tab.alive ? "" : " (exited)";
        addItem(container, `${label}${alive}`);
      }
    }

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
  render();
}

function addItem(container: HTMLElement, text: string, ...classes: string[]) {
  const item = document.createElement("div");
  item.className = ["status-item", ...classes].join(" ");
  item.textContent = text;
  container.appendChild(item);
}
