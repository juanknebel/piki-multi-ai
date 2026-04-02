import { appState } from "../state";
import { renderWorkspaceList } from "./workspace-list";
import { renderSourceControl } from "./source-control";
import { renderAgentsPanel } from "./agents-panel";

export function initSidebar() {
  const explorerView = document.getElementById("explorer-view")!;
  const workspaceList = document.getElementById("workspace-list")!;
  const scView = document.getElementById("source-control-view")!;
  const agentsView = document.getElementById("agents-view")!;

  renderWorkspaceList(workspaceList);
  renderSourceControl(scView);
  renderAgentsPanel(agentsView);

  function updateView() {
    const view = appState.activeView;
    explorerView.style.display = view === "explorer" ? "flex" : "none";
    scView.style.display = view === "git" ? "flex" : "none";
    agentsView.style.display = view === "agents" ? "flex" : "none";
  }

  appState.on("view-changed", updateView);
  updateView();
}
