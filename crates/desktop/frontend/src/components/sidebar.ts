import { appState } from "../state";
import { renderWorkspaceList } from "./workspace-list";
import { renderSourceControl } from "./source-control";

export function initSidebar() {
  const explorerView = document.getElementById("explorer-view")!;
  const workspaceList = document.getElementById("workspace-list")!;
  const scView = document.getElementById("source-control-view")!;

  renderWorkspaceList(workspaceList);
  renderSourceControl(scView);

  function updateView() {
    const view = appState.activeView;
    explorerView.style.display = view === "explorer" ? "flex" : "none";
    scView.style.display = view === "git" ? "flex" : "none";
  }

  appState.on("view-changed", updateView);
  updateView();
}
