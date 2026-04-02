import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";
import type { AgentInfo } from "../../ipc";

const PROVIDERS = ["Claude Code", "Gemini", "OpenCode", "Kilo", "Codex"];

export async function showAgentManager() {
  document.querySelector(".dialog-backdrop")?.remove();

  const wsIdx = appState.activeWorkspace;
  let agents: AgentInfo[];
  try {
    agents = await ipc.listAgents(wsIdx);
  } catch (err) {
    toast(`Failed to load agents: ${err}`, "error");
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";

  function render() {
    backdrop.querySelector(".dialog")?.remove();

    const dialog = document.createElement("div");
    dialog.className = "dialog";
    dialog.style.maxWidth = "600px";
    dialog.style.maxHeight = "80vh";
    dialog.innerHTML = `
      <div class="dialog-header">
        <span class="dialog-title">Agent Profiles</span>
        <span style="display:flex;gap:6px;align-items:center">
          <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="ag-import">Import from repo</button>
          <button class="dialog-btn dialog-btn-primary dialog-btn-sm" id="ag-new">+ New Agent</button>
          <button class="dialog-close">×</button>
        </span>
      </div>
      <div class="dialog-body" style="max-height:60vh;overflow-y:auto">
        ${agents.length === 0 ? '<div class="empty-message">No agent profiles configured for this project.</div>' : ""}
        ${agents.map((a) => `
          <div class="agent-manager-item" data-id="${a.id}">
            <div class="agent-manager-item-header">
              <span class="agent-manager-item-name">${esc(a.name)}</span>
              <span class="agent-manager-item-provider">${esc(a.provider)}</span>
              <span class="agent-manager-item-version">v${a.version}${a.last_synced_at ? " ✓" : ""}</span>
            </div>
            <div class="agent-manager-item-role">${esc(a.role.slice(0, 200))}${a.role.length > 200 ? "..." : ""}</div>
            <div class="agent-manager-item-actions">
              <button class="dialog-btn dialog-btn-secondary dialog-btn-sm ag-edit" data-id="${a.id}">Edit</button>
              <button class="dialog-btn dialog-btn-danger dialog-btn-sm ag-delete" data-id="${a.id}">Delete</button>
            </div>
          </div>
        `).join("")}
      </div>
    `;

    // New agent
    dialog.querySelector("#ag-new")!.addEventListener("click", () => {
      showAgentForm(null, () => reload());
    });

    // Import
    dialog.querySelector("#ag-import")!.addEventListener("click", () => {
      showImportDialog(() => reload());
    });

    // Edit buttons
    dialog.querySelectorAll<HTMLButtonElement>(".ag-edit").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = parseInt(btn.dataset.id!, 10);
        const agent = agents.find((a) => a.id === id);
        if (agent) showAgentForm(agent, () => reload());
      });
    });

    // Delete buttons
    dialog.querySelectorAll<HTMLButtonElement>(".ag-delete").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = parseInt(btn.dataset.id!, 10);
        const agent = agents.find((a) => a.id === id);
        showDeleteConfirm(agent?.name ?? "this agent", async () => {
          try {
            await ipc.deleteAgent(id);
            toast("Agent deleted", "info");
            await reload();
          } catch (err) {
            toast(`Delete failed: ${err}`, "error");
          }
        });
      });
    });

    dialog.querySelector(".dialog-close")!.addEventListener("click", close);
    backdrop.appendChild(dialog);
  }

  async function reload() {
    try {
      agents = await ipc.listAgents(wsIdx);
      render();
    } catch (err) {
      toast(`Failed to reload agents: ${err}`, "error");
    }
  }

  const close = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });

  document.body.appendChild(backdrop);
  render();
}

function showAgentForm(existing: AgentInfo | null, onSaved: () => void) {
  document.querySelector(".agent-form-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop agent-form-backdrop";
  backdrop.style.zIndex = "110";

  const isEdit = existing !== null;
  backdrop.innerHTML = `
    <div class="dialog" style="max-width:560px">
      <div class="dialog-header">
        <span class="dialog-title">${isEdit ? "Edit Agent" : "New Agent"}</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Name</label>
          <input class="dialog-input" id="af-name" value="${esc(existing?.name ?? "")}" ${isEdit ? "readonly style='opacity:0.6'" : ""} />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Provider</label>
          <select class="dialog-select" id="af-provider">
            ${PROVIDERS.map((p) => `<option value="${p}"${existing?.provider === p ? " selected" : ""}>${p}</option>`).join("")}
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

    if (!name) { toast("Name is required", "error"); return; }
    if (!role) { toast("Role is required", "error"); return; }

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

async function showImportDialog(onImported: () => void) {
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
  backdrop.style.zIndex = "110";

  const selected = new Set(scanned.filter((a) => !a.exists).map((_, i) => i));

  function render() {
    backdrop.querySelector(".dialog")?.remove();
    const dialog = document.createElement("div");
    dialog.className = "dialog";
    dialog.style.maxWidth = "500px";
    dialog.innerHTML = `
      <div class="dialog-header">
        <span class="dialog-title">Import Agents from Repo</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        ${scanned.map((a, i) => `
          <label class="import-check-item">
            <input type="checkbox" class="ag-import-check" data-idx="${i}" ${selected.has(i) ? "checked" : ""} />
            <span class="import-check-name">${esc(a.name)}</span>
            <span class="import-check-provider">${esc(a.provider)}</span>
            ${a.exists ? '<span class="import-check-badge exists">(exists)</span>' : '<span class="import-check-badge new">(new)</span>'}
          </label>
        `).join("")}
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-secondary" id="ai-cancel">Cancel</button>
        <button class="dialog-btn dialog-btn-primary" id="ai-import">Import (${selected.size})</button>
      </div>
    `;

    dialog.querySelectorAll<HTMLInputElement>(".ag-import-check").forEach((cb) => {
      cb.addEventListener("change", () => {
        const idx = parseInt(cb.dataset.idx!, 10);
        if (cb.checked) selected.add(idx); else selected.delete(idx);
        dialog.querySelector<HTMLButtonElement>("#ai-import")!.textContent = `Import (${selected.size})`;
      });
    });

    dialog.querySelector(".dialog-close")!.addEventListener("click", close);
    dialog.querySelector("#ai-cancel")!.addEventListener("click", close);
    dialog.querySelector("#ai-import")!.addEventListener("click", async () => {
      const toImport = [...selected].map((i) => scanned[i]);
      try {
        const count = await ipc.importAgents(wsIdx, toImport);
        toast(`Imported ${count} agent(s)`, "success");
        close();
        onImported();
      } catch (err) {
        toast(`Import failed: ${err}`, "error");
      }
    });

    backdrop.appendChild(dialog);
  }

  const close = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });

  document.body.appendChild(backdrop);
  render();
}

function showDeleteConfirm(name: string, onConfirm: () => void) {
  document.querySelector(".ws-delete-confirm")?.remove();
  const overlay = document.createElement("div");
  overlay.className = "ws-delete-confirm";
  overlay.innerHTML = `
    <div class="ws-delete-dialog">
      <p>Delete <strong>${esc(name)}</strong>?</p>
      <p class="ws-delete-hint">This cannot be undone.</p>
      <div class="ws-delete-buttons">
        <button class="dialog-btn dialog-btn-danger ws-confirm-yes">Delete</button>
        <button class="dialog-btn dialog-btn-secondary ws-confirm-no">Cancel</button>
      </div>
    </div>
  `;
  overlay.querySelector(".ws-confirm-yes")!.addEventListener("click", () => { overlay.remove(); onConfirm(); });
  overlay.querySelector(".ws-confirm-no")!.addEventListener("click", () => overlay.remove());
  overlay.addEventListener("click", (e) => { if (e.target === overlay) overlay.remove(); });
  document.body.appendChild(overlay);
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
