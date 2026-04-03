import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";
import type { WorkspaceInfo } from "../../types";

type Mode = "create" | "edit" | "clone";

interface DialogOptions {
  mode: Mode;
  /** Index of workspace being edited (edit mode) */
  editIndex?: number;
  /** Workspace to clone from (clone mode) */
  cloneFrom?: WorkspaceInfo;
}

export function showWorkspaceDialog(opts: DialogOptions) {
  document.querySelector(".dialog-backdrop")?.remove();

  const { mode, editIndex, cloneFrom } = opts;
  const editWs =
    mode === "edit" && editIndex !== undefined
      ? appState.workspaces[editIndex]?.info
      : undefined;

  const prefill = cloneFrom || editWs;
  const title =
    mode === "create"
      ? "New Workspace"
      : mode === "edit"
        ? "Edit Workspace"
        : "Clone Workspace";

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";

  const showTypeAndDir = mode !== "edit";
  const showName = mode !== "edit";

  backdrop.innerHTML = `
    <div class="dialog">
      <div class="dialog-header">
        <span class="dialog-title">${title}</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        ${
          showTypeAndDir
            ? `
        <div class="dialog-field">
          <label class="dialog-label">Type</label>
          <select class="dialog-select" id="ws-type">
            <option value="Simple"${prefill?.workspace_type === "Simple" ? " selected" : ""}>Simple (existing directory)</option>
            <option value="Worktree"${!prefill || prefill.workspace_type === "Worktree" ? " selected" : ""}>Worktree (git branch)</option>
            <option value="Project"${prefill?.workspace_type === "Project" ? " selected" : ""}>Project (monorepo root)</option>
          </select>
        </div>
        `
            : ""
        }
        ${
          showName
            ? `
        <div class="dialog-field" id="ws-name-field">
          <label class="dialog-label">Name</label>
          <input class="dialog-input" id="ws-name" placeholder="feature/my-feature" value="${escapeAttr(mode === "clone" ? "" : prefill?.name ?? "")}" />
        </div>
        `
            : ""
        }
        ${
          showTypeAndDir
            ? `
        <div class="dialog-field">
          <label class="dialog-label">Directory</label>
          <input class="dialog-input" id="ws-dir" placeholder="/path/to/repo" value="${escapeAttr(prefill?.source_repo ?? prefill?.path ?? "")}" />
        </div>
        `
            : ""
        }
        <div class="dialog-field">
          <label class="dialog-label">Description</label>
          <input class="dialog-input" id="ws-desc" placeholder="Brief description" value="${escapeAttr(prefill?.description ?? "")}" />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Group</label>
          <input class="dialog-input" id="ws-group" placeholder="Optional group name" value="${escapeAttr(prefill?.group ?? "")}" />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Kanban Path</label>
          <input class="dialog-input" id="ws-kanban" placeholder="Path to .board directory (optional)" value="${escapeAttr(prefill?.kanban_path ?? "")}" />
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Prompt</label>
          <textarea class="dialog-textarea" id="ws-prompt" placeholder="Initial prompt for AI tabs" rows="3">${escapeHtml(prefill?.prompt ?? "")}</textarea>
        </div>
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-secondary" id="ws-cancel">Cancel</button>
        <button class="dialog-btn dialog-btn-primary" id="ws-submit">${mode === "edit" ? "Save" : "Create"}</button>
      </div>
    </div>
  `;

  document.body.appendChild(backdrop);

  // Auto-focus first input
  const firstInput = backdrop.querySelector<HTMLInputElement | HTMLSelectElement>(
    showTypeAndDir ? "#ws-type" : "#ws-desc",
  );
  firstInput?.focus();

  // Toggle name field visibility based on type
  const typeSelect = backdrop.querySelector<HTMLSelectElement>("#ws-type");
  const nameField = backdrop.querySelector<HTMLElement>("#ws-name-field");
  if (typeSelect && nameField) {
    function updateNameVisibility() {
      const t = typeSelect!.value;
      nameField!.style.display = t === "Worktree" ? "" : "none";
    }
    typeSelect.addEventListener("change", updateNameVisibility);
    updateNameVisibility();
  }

  // Close
  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#ws-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  backdrop.setAttribute("tabindex", "0");

  // Submit
  backdrop.querySelector("#ws-submit")!.addEventListener("click", async () => {
    if (mode === "edit" && editIndex !== undefined) {
      await submitEdit(backdrop, editIndex);
    } else {
      await submitCreate(backdrop);
    }
  });
}

async function submitCreate(backdrop: HTMLElement) {
  const type =
    backdrop.querySelector<HTMLSelectElement>("#ws-type")?.value ?? "Simple";
  const name =
    backdrop.querySelector<HTMLInputElement>("#ws-name")?.value.trim() ?? "";
  const dir =
    backdrop.querySelector<HTMLInputElement>("#ws-dir")?.value.trim() ?? "";
  const desc =
    backdrop.querySelector<HTMLInputElement>("#ws-desc")?.value.trim() ?? "";
  const group =
    backdrop.querySelector<HTMLInputElement>("#ws-group")?.value.trim() ?? "";
  const kanban =
    backdrop.querySelector<HTMLInputElement>("#ws-kanban")?.value.trim() ?? "";
  const prompt =
    backdrop.querySelector<HTMLTextAreaElement>("#ws-prompt")?.value.trim() ??
    "";

  if (!dir) {
    toast("Directory is required", "error");
    return;
  }
  if (type === "Worktree" && !name) {
    toast("Name is required for worktree workspaces", "error");
    return;
  }

  const btn = backdrop.querySelector<HTMLButtonElement>("#ws-submit")!;
  btn.disabled = true;
  btn.textContent = "Creating...";

  try {
    const info = await ipc.createWorkspace(
      name,
      desc,
      prompt,
      dir,
      type,
      group || null,
      kanban || null,
    );
    appState.addWorkspace(info);
    toast(`Workspace "${info.name}" created`, "success");
    backdrop.remove();
  } catch (err) {
    toast(`Failed to create workspace: ${err}`, "error");
    btn.disabled = false;
    btn.textContent = "Create";
  }
}

async function submitEdit(backdrop: HTMLElement, index: number) {
  const desc =
    backdrop.querySelector<HTMLInputElement>("#ws-desc")?.value.trim();
  const group =
    backdrop.querySelector<HTMLInputElement>("#ws-group")?.value.trim();
  const kanban =
    backdrop.querySelector<HTMLInputElement>("#ws-kanban")?.value.trim();
  const prompt =
    backdrop.querySelector<HTMLTextAreaElement>("#ws-prompt")?.value.trim();

  const btn = backdrop.querySelector<HTMLButtonElement>("#ws-submit")!;
  btn.disabled = true;
  btn.textContent = "Saving...";

  try {
    await ipc.updateWorkspace(index, prompt, group, desc, kanban);
    // Update local state
    const ws = appState.workspaces[index];
    if (ws) {
      if (prompt !== undefined) ws.info.prompt = prompt;
      if (group !== undefined)
        ws.info.group = group === "" ? null : group;
      if (desc !== undefined) ws.info.description = desc;
      if (kanban !== undefined)
        ws.info.kanban_path = kanban === "" ? null : kanban;
    }
    toast("Workspace updated", "success");
    backdrop.remove();
  } catch (err) {
    toast(`Failed to update: ${err}`, "error");
    btn.disabled = false;
    btn.textContent = "Save";
  }
}

export function showWorkspaceInfo(index: number) {
  document.querySelector(".dialog-backdrop")?.remove();

  const ws = appState.workspaces[index];
  if (!ws) return;
  const info = ws.info;

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.innerHTML = `
    <div class="dialog" style="max-width:500px">
      <div class="dialog-header">
        <span class="dialog-title">Workspace Info</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        ${infoRow("Name", info.name)}
        ${infoRow("Type", info.workspace_type)}
        ${infoRow("Branch", info.branch)}
        ${infoRow("Path", String(info.path))}
        ${infoRow("Source Repo", info.source_repo_display)}
        ${infoRow("Group", info.group || "—")}
        ${infoRow("Kanban Path", info.kanban_path || "—")}
        ${infoRow("Description", info.description || "—")}
        ${infoRow("Prompt", info.prompt || "—")}
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-secondary" id="ws-info-close">Close</button>
      </div>
    </div>
  `;

  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#ws-info-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

function infoRow(label: string, value: string): string {
  return `
    <div class="info-row">
      <span class="info-row-label">${label}</span>
      <span class="info-row-value">${escapeHtml(value)}</span>
    </div>
  `;
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(text: string): string {
  return (text ?? "").replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}
