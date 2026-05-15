import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";
import { createDropdown } from "../dropdown";
import { attachPathPicker } from "../path-picker";
import type { WorkspaceInfo } from "../../types";

type Mode = "create" | "edit" | "clone";

export interface WorkspacePrefill {
  /** Pre-fill the folder field (used when source=local). */
  dir?: string;
}

interface DialogOptions {
  mode: Mode;
  /** Index of workspace being edited (edit mode) */
  editIndex?: number;
  /** Workspace to clone from (clone mode) */
  cloneFrom?: WorkspaceInfo;
  /** Optional prefill for create mode (e.g. when launching from a Project sub-dir) */
  prefill?: WorkspacePrefill;
}

export function showWorkspaceDialog(opts: DialogOptions) {
  document.querySelector(".dialog-backdrop")?.remove();

  const { mode, editIndex, cloneFrom, prefill: createPrefill } = opts;
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

  const showSourceAndDir = mode !== "edit";
  const showName = mode !== "edit";

  backdrop.innerHTML = `
    <div class="dialog">
      <div class="dialog-header">
        <span class="dialog-title">${title}</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        ${
          showSourceAndDir
            ? `
        <div class="dialog-field">
          <label class="dialog-label">Source</label>
          <span id="ws-source-slot"></span>
        </div>
        <div class="dialog-field" id="ws-folder-field">
          <label class="dialog-label">Folder</label>
          <input class="dialog-input" id="ws-dir" placeholder="/path/to/folder" value="${escapeAttr(createPrefill?.dir ?? prefill?.source_repo ?? prefill?.path ?? "")}" />
        </div>
        <div class="dialog-field" id="ws-url-field" style="display:none">
          <label class="dialog-label">GitHub URL</label>
          <input class="dialog-input" id="ws-url" placeholder="https://github.com/owner/repo[.git]" value="" />
        </div>
        `
            : ""
        }
        ${
          showName
            ? `
        <div class="dialog-field" id="ws-name-field">
          <label class="dialog-label">Name <span style="opacity:0.6;font-weight:normal">(optional)</span></label>
          <input class="dialog-input" id="ws-name" placeholder="auto-derived if empty" value="${escapeAttr(mode === "clone" ? "" : prefill?.name ?? "")}" />
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

  // Attach native folder pickers to path inputs
  const dirInput = backdrop.querySelector<HTMLInputElement>("#ws-dir");
  if (dirInput) attachPathPicker(dirInput, { title: "Select workspace directory" });
  const kanbanInput = backdrop.querySelector<HTMLInputElement>("#ws-kanban");
  if (kanbanInput) attachPathPicker(kanbanInput, { title: "Select kanban directory" });

  // Mount source dropdown (replaces the old workspace-type dropdown)
  let sourceDropdown: ReturnType<typeof createDropdown> | null = null;
  const sourceSlot = backdrop.querySelector("#ws-source-slot");
  if (sourceSlot) {
    sourceDropdown = createDropdown(
      [
        { value: "local", label: "Local folder" },
        { value: "github", label: "GitHub URL" },
      ],
      "local",
    );
    sourceSlot.replaceWith(sourceDropdown.container);
  }

  // Toggle Folder/URL field visibility on source change
  const folderField = backdrop.querySelector<HTMLElement>("#ws-folder-field");
  const urlField = backdrop.querySelector<HTMLElement>("#ws-url-field");
  if (sourceDropdown && folderField && urlField) {
    const updateSourceFields = () => {
      const isGithub = sourceDropdown!.value === "github";
      folderField.style.display = isGithub ? "none" : "";
      urlField.style.display = isGithub ? "" : "none";
    };
    sourceDropdown.container.addEventListener("change", updateSourceFields);
    updateSourceFields();
  }

  // Auto-focus first input
  if (sourceDropdown) {
    sourceDropdown.container.querySelector("button")?.focus();
  } else {
    backdrop.querySelector<HTMLInputElement>("#ws-desc")?.focus();
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
      await submitCreate(backdrop, sourceDropdown?.value ?? "local");
    }
  });
}

async function submitCreate(backdrop: HTMLElement, source: string) {
  const name =
    backdrop.querySelector<HTMLInputElement>("#ws-name")?.value.trim() ?? "";
  const desc =
    backdrop.querySelector<HTMLInputElement>("#ws-desc")?.value.trim() ?? "";
  const group =
    backdrop.querySelector<HTMLInputElement>("#ws-group")?.value.trim() ?? "";
  const kanban =
    backdrop.querySelector<HTMLInputElement>("#ws-kanban")?.value.trim() ?? "";
  const prompt =
    backdrop.querySelector<HTMLTextAreaElement>("#ws-prompt")?.value.trim() ??
    "";

  const btn = backdrop.querySelector<HTMLButtonElement>("#ws-submit")!;

  if (source === "github") {
    const url =
      backdrop.querySelector<HTMLInputElement>("#ws-url")?.value.trim() ?? "";
    if (!url) {
      toast("GitHub URL is required", "error");
      return;
    }
    const finalName = name || parseGithubRepoNameFromUrl(url);
    if (!finalName) {
      toast("Could not parse repo name from URL", "error");
      return;
    }
    btn.disabled = true;
    btn.textContent = "Cloning...";
    try {
      const info = await ipc.createGithubWorkspace(
        finalName,
        desc,
        prompt,
        url,
        group || null,
        kanban || null,
      );
      appState.addWorkspace(info);
      toast(`Workspace "${info.name}" cloned`, "success");
      backdrop.remove();
    } catch (err) {
      toast(`Failed to clone: ${err}`, "error");
      btn.disabled = false;
      btn.textContent = "Create";
    }
    return;
  }

  // source === "local"
  const dir =
    backdrop.querySelector<HTMLInputElement>("#ws-dir")?.value.trim() ?? "";
  if (!dir) {
    toast("Folder is required", "error");
    return;
  }
  const finalName = name || basenameFromPath(dir);
  if (!finalName) {
    toast("Could not derive workspace name from folder", "error");
    return;
  }
  btn.disabled = true;
  btn.textContent = "Creating...";

  try {
    const info = await ipc.createWorkspace(
      finalName,
      desc,
      prompt,
      dir,
      "Simple",
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

/** Mirrors `piki_core::workspace::manager::parse_github_repo_name`. Extracts
 *  the trailing segment of a clone-style URL, stripping `.git` if present.
 *  Returns "" when the input is empty or trims to nothing usable. */
function parseGithubRepoNameFromUrl(url: string): string {
  const trimmed = url.trim().replace(/\/+$/, "").split(/[?#]/)[0] ?? "";
  const last = trimmed.split(/[/:]/).pop() ?? "";
  return last.replace(/\.git$/, "");
}

function basenameFromPath(path: string): string {
  return path.replace(/\/+$/, "").split("/").pop() ?? "";
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
