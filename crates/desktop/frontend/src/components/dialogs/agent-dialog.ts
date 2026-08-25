import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";
import { showConfirm } from "../confirm";
import { createDropdown } from "../dropdown";
import { icon } from "../icons";
import type { AgentInfo } from "../../ipc";
import { showNeedsWorkspace } from "./needs-workspace";

async function loadProviderNames(): Promise<string[]> {
  try {
    const list = await ipc.listProviders();
    return list.map((p) => p.name);
  } catch {
    return [];
  }
}

/** Agent profiles of one workspace's repo — the active one, or
 *  `workspaceIdx` when opened from that workspace's row (⚙). */
export async function showAgentManager(workspaceIdx?: number) {
  document.querySelector(".agent-manager-backdrop")?.remove();

  if (appState.workspaces.length === 0) {
    showNeedsWorkspace("Agent profiles are stored in a workspace's repository — create one to manage them.");
    return;
  }
  const wsIdx = workspaceIdx ?? appState.activeWorkspace;
  const wsName = appState.workspaces[wsIdx]?.info.name ?? "";
  let agents: AgentInfo[];
  try {
    agents = await ipc.listAgents(wsIdx);
  } catch (err) {
    toast(`Failed to load agents: ${err}`, "error");
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop agent-manager-backdrop";

  function render() {
    backdrop.querySelector(".dialog")?.remove();

    const dialog = document.createElement("div");
    dialog.className = "dialog ui-surface";
    dialog.style.maxWidth = "600px";
    dialog.style.maxHeight = "80vh";
    dialog.innerHTML = `
      <div class="ui-header">
        <span class="ui-header-title">Agent Profiles${wsName ? ` · ${esc(wsName)}` : ""}</span>
        <span style="display:flex;gap:6px;align-items:center">
          <button data-variant="secondary" data-size="sm" class="ui-btn" id="ag-import">Import from repo</button>
          <button data-variant="primary" data-size="sm" class="ui-btn" id="ag-new">+ New Agent</button>
          <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">×</button>
        </span>
      </div>
      <div class="dialog-body" style="max-height:60vh;overflow-y:auto">
        ${agents.length === 0 ? '<div class="ui-empty">No agent profiles configured for this project.</div>' : ""}
        ${agents.map((a) => `
          <div class="agent-manager-item" data-id="${a.id}">
            <div class="agent-manager-item-header">
              <span class="agent-manager-item-name">${esc(a.name)}</span>
              <span class="agent-manager-item-provider">${esc(a.provider)}</span>
              <span class="agent-manager-item-version">v${a.version}${a.last_synced_at ? ` ${icon("check", { label: "synced" })}` : ""}</span>
            </div>
            <div class="agent-manager-item-role">${esc(a.role.slice(0, 200))}${a.role.length > 200 ? "..." : ""}</div>
            <div class="agent-manager-item-actions">
              <button data-variant="secondary" data-size="sm" class="ui-btn ag-edit" data-id="${a.id}">Edit</button>
              <button data-variant="secondary" data-size="sm" class="ui-btn ag-sync" data-id="${a.id}">Sync</button>
              <button data-variant="danger" data-size="sm" class="ui-btn ag-delete" data-id="${a.id}">Delete</button>
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
      showImportDialog(wsIdx, () => reload());
    });

    // Edit buttons
    dialog.querySelectorAll<HTMLButtonElement>(".ag-edit").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = parseInt(btn.dataset.id!, 10);
        const agent = agents.find((a) => a.id === id);
        if (agent) showAgentForm(agent, () => reload());
      });
    });

    // Sync buttons
    dialog.querySelectorAll<HTMLButtonElement>(".ag-sync").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const id = parseInt(btn.dataset.id!, 10);
        try {
          await ipc.syncAgentToRepo(wsIdx, id);
          toast("Agent synced to repo", "success");
          await reload();
        } catch (err) {
          toast(`Sync failed: ${err}`, "error");
        }
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
  backdrop.setAttribute("tabindex", "0");

  document.body.appendChild(backdrop);
  render();
  backdrop.focus();
}

async function showAgentForm(existing: AgentInfo | null, onSaved: () => void) {
  document.querySelector(".agent-form-backdrop")?.remove();

  const providerNames = await loadProviderNames();
  if (providerNames.length === 0) {
    toast("No providers configured. Add one in the Providers dialog.", "error");
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop agent-form-backdrop";
  backdrop.style.zIndex = "var(--z-dialog)";

  const isEdit = existing !== null;
  backdrop.innerHTML = `
    <div class="dialog ui-surface" style="max-width:560px">
      <div class="ui-header">
        <span class="ui-header-title">${isEdit ? "Edit Agent" : "New Agent"}</span>
        <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Name</label>
          <input class="ui-input" id="af-name" value="${esc(existing?.name ?? "")}" ${isEdit ? "readonly" : ""} />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Provider</label>
          <span id="af-provider-slot"></span>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Role / Instructions</label>
          <textarea class="ui-input" id="af-role" rows="12" style="min-height:200px;font-family:var(--font-mono);font-size:12px">${esc(existing?.role ?? "")}</textarea>
        </div>
      </div>
      <div class="dialog-footer">
        <button data-variant="secondary" class="ui-btn" id="af-cancel">Cancel</button>
        <button data-variant="primary" class="ui-btn" id="af-save">${isEdit ? "Save" : "Create"}</button>
      </div>
    </div>
  `;

  const providerDropdown = createDropdown(
    providerNames.map((p) => ({ value: p, label: p })),
    existing?.provider ?? providerNames[0],
  );
  backdrop.querySelector("#af-provider-slot")!.replaceWith(providerDropdown.container);

  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#af-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });

  backdrop.querySelector("#af-save")!.addEventListener("click", async () => {
    const name = (backdrop.querySelector("#af-name") as HTMLInputElement).value.trim();
    const provider = providerDropdown.value;
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

async function showImportDialog(wsIdx: number, onImported: () => void) {
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
  backdrop.style.zIndex = "var(--z-dialog)";

  const selected = new Set(
    scanned.map((a, i) => (a.exists ? -1 : i)).filter((i) => i >= 0),
  );
  let filterText = "";

  const close = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });

  const dialog = document.createElement("div");
  dialog.className = "dialog ui-surface";
  dialog.style.maxWidth = "500px";
  dialog.innerHTML = `
    <div class="ui-header">
      <span class="ui-header-title">Import Agents from Repo</span>
      <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">×</button>
    </div>
    <div class="dialog-toolbar import-toolbar">
      <input type="text" class="ui-input import-filter" placeholder="Filter agents..." />
      <button data-variant="secondary" data-size="sm" class="ui-btn" id="ai-select-all">Select all</button>
      <button data-variant="secondary" data-size="sm" class="ui-btn" id="ai-select-none">Select none</button>
      <span class="import-count"></span>
    </div>
    <div class="dialog-body" id="ai-list"></div>
    <div class="dialog-footer">
      <button data-variant="secondary" class="ui-btn" id="ai-cancel">Cancel</button>
      <button data-variant="primary" class="ui-btn" id="ai-import">Import (${selected.size})</button>
    </div>
  `;
  backdrop.appendChild(dialog);

  const listEl = dialog.querySelector<HTMLDivElement>("#ai-list")!;
  const importBtn = dialog.querySelector<HTMLButtonElement>("#ai-import")!;
  const countEl = dialog.querySelector<HTMLSpanElement>(".import-count")!;
  const filterEl = dialog.querySelector<HTMLInputElement>(".import-filter")!;

  function visibleIndices(): number[] {
    const q = filterText.trim().toLowerCase();
    if (!q) return scanned.map((_, i) => i);
    return scanned
      .map((a, i) => (a.name.toLowerCase().includes(q) || a.provider.toLowerCase().includes(q) ? i : -1))
      .filter((i) => i >= 0);
  }

  function renderList() {
    const vis = visibleIndices();
    listEl.innerHTML = vis.length === 0
      ? '<div class="ui-empty">No agents match the filter.</div>'
      : vis.map((i) => {
          const a = scanned[i];
          return `
            <label class="import-check-item">
              <input type="checkbox" class="ag-import-check" data-idx="${i}" ${selected.has(i) ? "checked" : ""} />
              <span class="import-check-name">${esc(a.name)}</span>
              <span class="import-check-provider">${esc(a.provider)}</span>
              ${a.exists ? '<span class="import-check-badge exists">(exists)</span>' : '<span class="import-check-badge new">(new)</span>'}
            </label>
          `;
        }).join("");

    listEl.querySelectorAll<HTMLInputElement>(".ag-import-check").forEach((cb) => {
      cb.addEventListener("change", () => {
        const idx = parseInt(cb.dataset.idx!, 10);
        if (cb.checked) selected.add(idx); else selected.delete(idx);
        updateCounts();
      });
    });
    updateCounts();
  }

  function updateCounts() {
    importBtn.textContent = `Import (${selected.size})`;
    countEl.textContent = `${selected.size}/${scanned.length} selected`;
  }

  filterEl.addEventListener("input", () => {
    filterText = filterEl.value;
    renderList();
  });

  dialog.querySelector("#ai-select-all")!.addEventListener("click", () => {
    for (const i of visibleIndices()) selected.add(i);
    renderList();
  });
  dialog.querySelector("#ai-select-none")!.addEventListener("click", () => {
    for (const i of visibleIndices()) selected.delete(i);
    renderList();
  });

  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  dialog.querySelector("#ai-cancel")!.addEventListener("click", close);
  importBtn.addEventListener("click", async () => {
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

  document.body.appendChild(backdrop);
  renderList();
  filterEl.focus();
}

function showDeleteConfirm(name: string, onConfirm: () => void) {
  showConfirm({
    bodyHtml: `
      <p>Delete <strong>${esc(name)}</strong>?</p>
      <p class="ws-delete-hint">This cannot be undone.</p>
    `,
    actions: [
      { label: "Delete", kind: "danger", isDefault: true, onSelect: () => onConfirm() },
      { label: "Cancel", kind: "secondary" },
    ],
  });
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
