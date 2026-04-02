import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";
import type { AgentInfo } from "../ipc";

const PROVIDERS = ["Claude Code", "Gemini", "OpenCode", "Kilo", "Codex"];

let agents: AgentInfo[] = [];

export function renderAgentsPanel(container: HTMLElement) {
  async function loadAndRender() {
    const wsIdx = appState.activeWorkspace;
    try {
      agents = await ipc.listAgents(wsIdx);
    } catch {
      agents = [];
    }
    render();
  }

  function render() {
    container.innerHTML = "";

    // Header
    const header = document.createElement("div");
    header.className = "sidebar-header sc-header";
    header.innerHTML = `
      <span>AGENTS</span>
      <span class="sc-header-actions">
        <button class="sc-header-btn" id="ag-import-btn" title="Import from repo">↓</button>
        <button class="sc-header-btn" id="ag-new-btn" title="New Agent">+</button>
      </span>
    `;
    container.appendChild(header);

    // Dispatch button
    const dispatchBtn = document.createElement("div");
    dispatchBtn.style.cssText = "padding:8px 12px;border-bottom:1px solid var(--border-primary);";
    dispatchBtn.innerHTML = `
      <button class="sc-commit-btn" id="ag-dispatch-btn">
        <span class="sc-commit-icon">▶</span> Dispatch Agent
      </button>
    `;
    container.appendChild(dispatchBtn);

    // Agent list
    if (agents.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-message";
      empty.innerHTML = "No agents configured.<br>Click <strong>+</strong> to create one or <strong>↓</strong> to import from repo.";
      container.appendChild(empty);
    } else {
      for (const agent of agents) {
        const item = document.createElement("div");
        item.className = "agent-panel-item";

        item.innerHTML = `
          <div class="agent-panel-header">
            <span class="agent-panel-name">${esc(agent.name)}</span>
            <span class="agent-panel-provider">${esc(agent.provider)}</span>
          </div>
          <div class="agent-panel-role">${esc(agent.role.slice(0, 120))}${agent.role.length > 120 ? "..." : ""}</div>
          <div class="agent-panel-actions">
            <button class="file-action-btn ag-act-dispatch" title="Dispatch this agent">▶</button>
            <button class="file-action-btn ag-act-edit" title="Edit">✎</button>
            <button class="file-action-btn ws-action-delete ag-act-delete" title="Delete">×</button>
          </div>
        `;

        // Dispatch this agent
        item.querySelector(".ag-act-dispatch")!.addEventListener("click", () => {
          quickDispatch(agent);
        });

        // Edit
        item.querySelector(".ag-act-edit")!.addEventListener("click", () => {
          showEditForm(agent, loadAndRender);
        });

        // Delete
        item.querySelector(".ag-act-delete")!.addEventListener("click", async () => {
          if (!agent.id) return;
          if (!confirm(`Delete agent "${agent.name}"?`)) return;
          try {
            await ipc.deleteAgent(agent.id);
            toast(`Agent "${agent.name}" deleted`, "info");
            loadAndRender();
          } catch (err) {
            toast(`Delete failed: ${err}`, "error");
          }
        });

        container.appendChild(item);
      }
    }

    // Wire header buttons
    container.querySelector("#ag-new-btn")!.addEventListener("click", () => {
      showEditForm(null, loadAndRender);
    });

    container.querySelector("#ag-import-btn")!.addEventListener("click", async () => {
      await handleImport(loadAndRender);
    });

    container.querySelector("#ag-dispatch-btn")!.addEventListener("click", () => {
      showDispatchDialog();
    });
  }

  appState.on("active-workspace-changed", loadAndRender);
  appState.on("view-changed", () => {
    if (appState.activeView === "agents") loadAndRender();
  });

  loadAndRender();
}

function showEditForm(existing: AgentInfo | null, onSaved: () => void) {
  document.querySelector(".agent-form-backdrop")?.remove();

  const isEdit = existing !== null;
  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop agent-form-backdrop";

  backdrop.innerHTML = `
    <div class="dialog" style="max-width:560px">
      <div class="dialog-header">
        <span class="dialog-title">${isEdit ? "Edit Agent" : "New Agent"}</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Name</label>
          <input class="dialog-input" id="af-name" value="${esc(existing?.name ?? "")}" ${isEdit ? 'readonly style="opacity:0.6"' : ""} />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Provider</label>
          <select class="dialog-select" id="af-provider">
            ${PROVIDERS.map(p => `<option value="${p}"${existing?.provider === p ? " selected" : ""}>${p}</option>`).join("")}
          </select>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Role / Instructions</label>
          <textarea class="dialog-textarea" id="af-role" rows="12" style="min-height:200px;font-family:'JetBrainsMono NF Mono',monospace;font-size:12px">${esc(existing?.role ?? "")}</textarea>
        </div>
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-secondary" id="af-cancel">Cancel</button>
        <button class="dialog-btn dialog-btn-primary" id="af-save">${isEdit ? "Save" : "Create"}</button>
      </div>
    </div>
  `;

  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#af-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });

  backdrop.querySelector("#af-save")!.addEventListener("click", async () => {
    const name = (backdrop.querySelector("#af-name") as HTMLInputElement).value.trim();
    const provider = (backdrop.querySelector("#af-provider") as HTMLSelectElement).value;
    const role = (backdrop.querySelector("#af-role") as HTMLTextAreaElement).value.trim();
    if (!name || !role) { toast("Name and role are required", "error"); return; }

    try {
      await ipc.saveAgent(appState.activeWorkspace, name, provider, role, existing?.id);
      toast(`Agent "${name}" ${isEdit ? "updated" : "created"}`, "success");
      close();
      onSaved();
    } catch (err) {
      toast(`Save failed: ${err}`, "error");
    }
  });

  document.body.appendChild(backdrop);
  (backdrop.querySelector(isEdit ? "#af-role" : "#af-name") as HTMLElement).focus();
}

async function quickDispatch(agent: AgentInfo) {
  const wsIdx = appState.activeWorkspace;
  try {
    const tabId = await ipc.dispatchAgent(wsIdx, agent.provider, agent.role, false);
    const providerMap: Record<string, import("../types").AIProvider> = {
      "Claude Code": "Claude", "Gemini": "Gemini", "OpenCode": "OpenCode", "Kilo": "Kilo", "Codex": "Codex",
    };
    appState.addTab(wsIdx, { id: tabId, provider: providerMap[agent.provider] || "Claude", alive: true });
    toast(`Dispatched "${agent.name}"`, "success");
  } catch (err) {
    toast(`Dispatch failed: ${err}`, "error");
  }
}

async function handleImport(onDone: () => void) {
  const wsIdx = appState.activeWorkspace;
  let scanned: ipc.ScannedAgent[];
  try {
    scanned = await ipc.scanRepoAgents(wsIdx);
  } catch (err) {
    toast(`Scan failed: ${err}`, "error");
    return;
  }

  if (scanned.length === 0) {
    toast("No agent files found in repo", "info");
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop agent-form-backdrop";
  const selected = new Set(scanned.map((_, i) => i).filter(i => !scanned[i].exists));

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "500px";
  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title">Import Agents</span>
      <button class="dialog-close">×</button>
    </div>
    <div class="dialog-body">
      ${scanned.map((a, i) => `
        <label style="display:flex;align-items:center;gap:8px;padding:4px 0;cursor:pointer">
          <input type="checkbox" class="ag-check" data-idx="${i}" ${selected.has(i) ? "checked" : ""} />
          <strong>${esc(a.name)}</strong>
          <span style="color:var(--text-muted);font-size:11px">${esc(a.provider)}</span>
          ${a.exists ? '<span style="color:var(--git-modified);font-size:10px">(exists)</span>' : '<span style="color:var(--git-added);font-size:10px">(new)</span>'}
        </label>
      `).join("")}
    </div>
    <div class="dialog-footer">
      <button class="dialog-btn dialog-btn-secondary" id="ai-cancel">Cancel</button>
      <button class="dialog-btn dialog-btn-primary" id="ai-import">Import (${selected.size})</button>
    </div>
  `;

  dialog.querySelectorAll<HTMLInputElement>(".ag-check").forEach(cb => {
    cb.addEventListener("change", () => {
      const idx = parseInt(cb.dataset.idx!, 10);
      if (cb.checked) selected.add(idx); else selected.delete(idx);
      dialog.querySelector<HTMLButtonElement>("#ai-import")!.textContent = `Import (${selected.size})`;
    });
  });

  const close = () => backdrop.remove();
  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  dialog.querySelector("#ai-cancel")!.addEventListener("click", close);
  dialog.querySelector("#ai-import")!.addEventListener("click", async () => {
    const toImport = [...selected].map(i => scanned[i]);
    try {
      const count = await ipc.importAgents(wsIdx, toImport);
      toast(`Imported ${count} agent(s)`, "success");
      close();
      onDone();
    } catch (err) {
      toast(`Import failed: ${err}`, "error");
    }
  });

  backdrop.appendChild(dialog);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  document.body.appendChild(backdrop);
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
