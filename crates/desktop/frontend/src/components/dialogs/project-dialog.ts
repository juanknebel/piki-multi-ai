/** Create / edit a project: name, one of the 10 palette swatches, and the
 *  member list — existing workspaces as checkboxes plus free directories
 *  added through the path picker. Members keep their saved order on edit;
 *  newly ticked workspaces and added directories append at the end. */
import * as ipc from "../../ipc";
import { appState } from "../../state";
import type { Project } from "../../types";
import { icon } from "../icons";
import { branchLabel } from "../../labels";
import { attachPathPicker } from "../path-picker";
import { reportError, toast } from "../toast";

const PALETTE_LEN = 10;

export function showProjectDialog(project: Project | null, onSaved: () => void | Promise<void>) {
  document.querySelector(".projects-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop projects-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog ui-surface project-dialog";

  let color = project ? project.color % PALETTE_LEN : 0;
  /** Directory members: every saved member path without a registered
   *  workspace, plus whatever gets added through the picker. */
  const dirs: string[] = (project?.members ?? [])
    .map((m) => m.path)
    .filter((p) => !appState.workspaces.some((w) => w.info.path === p));

  const workspaceRows = appState.workspaces.map((w) => ({
    path: w.info.path,
    name: w.info.name,
    sub: `${w.info.workspace_type.toLowerCase()}${w.branch ? ` · ${branchLabel(w.branch)}` : ""}`,
    title: w.branch ? `${w.info.path} · ${w.branch}` : w.info.path,
    checked: (project?.members ?? []).some((m) => m.path === w.info.path),
  }));

  dialog.innerHTML = `
    <div class="ui-header">
      <span class="ui-header-title">${project ? "Edit Project" : "New Project"}</span>
      <button data-variant="ghost" data-icon class="dialog-close ui-btn" aria-label="Close">×</button>
    </div>
    <div class="dialog-body">
      <div class="dialog-field">
        <label class="dialog-label" for="project-name">Name</label>
        <input class="ui-input" id="project-name" type="text" placeholder="e.g. Billing" />
      </div>
      <div class="dialog-field">
        <span class="dialog-label" id="project-color-label">Color</span>
        <div class="project-swatch-row" role="radiogroup" aria-labelledby="project-color-label"></div>
      </div>
      <div class="dialog-field">
        <span class="dialog-label">Members</span>
        <div class="project-member-picker" id="project-ws-rows"></div>
        <div class="project-dir-rows" id="project-dir-rows"></div>
        <input class="ui-input" id="project-dir-input" type="text" placeholder="Add a directory…" />
      </div>
    </div>
    <div class="dialog-footer">
      <button data-variant="secondary" class="ui-btn" id="project-cancel">Cancel</button>
      <button data-variant="primary" class="ui-btn" id="project-save">${project ? "Save" : "Create"}</button>
    </div>
  `;
  backdrop.appendChild(dialog);

  const nameInput = dialog.querySelector<HTMLInputElement>("#project-name")!;
  nameInput.value = project?.name ?? "";

  // ── Palette: 10 swatches as a radio group, ←/→ move the selection ──
  const swatchRow = dialog.querySelector<HTMLElement>(".project-swatch-row")!;
  function renderSwatches() {
    swatchRow.innerHTML = "";
    for (let i = 0; i < PALETTE_LEN; i++) {
      const sw = document.createElement("button");
      sw.type = "button";
      sw.className = `project-swatch-btn${i === color ? " active" : ""}`;
      sw.style.background = `var(--project-swatch-${i + 1})`;
      sw.setAttribute("role", "radio");
      sw.setAttribute("aria-checked", String(i === color));
      sw.setAttribute("aria-label", `Color ${i + 1}`);
      sw.tabIndex = i === color ? 0 : -1;
      sw.addEventListener("click", () => {
        color = i;
        renderSwatches();
      });
      sw.addEventListener("keydown", (e) => {
        if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
        e.preventDefault();
        color = (color + (e.key === "ArrowRight" ? 1 : PALETTE_LEN - 1)) % PALETTE_LEN;
        renderSwatches();
        swatchRow.querySelector<HTMLElement>(".project-swatch-btn.active")?.focus();
      });
      swatchRow.appendChild(sw);
    }
  }
  renderSwatches();

  // ── Workspace checkboxes ──
  const wsRowsEl = dialog.querySelector<HTMLElement>("#project-ws-rows")!;
  if (workspaceRows.length === 0) {
    wsRowsEl.innerHTML = `<span class="project-picker-hint">No workspaces yet — add directories below.</span>`;
  }
  for (const row of workspaceRows) {
    const label = document.createElement("label");
    label.className = "project-member-check";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = row.checked;
    cb.addEventListener("change", () => (row.checked = cb.checked));
    label.appendChild(cb);
    const name = document.createElement("span");
    name.className = "project-member-check-name";
    name.textContent = row.name;
    label.appendChild(name);
    const sub = document.createElement("span");
    sub.className = "project-member-check-sub";
    sub.textContent = row.sub;
    label.appendChild(sub);
    label.title = row.title;
    wsRowsEl.appendChild(label);
  }

  // ── Directory rows ──
  const dirRowsEl = dialog.querySelector<HTMLElement>("#project-dir-rows")!;
  function renderDirs() {
    dirRowsEl.innerHTML = "";
    for (const dir of dirs) {
      const row = document.createElement("div");
      row.className = "project-dir-row";
      const name = document.createElement("span");
      name.className = "project-dir-path";
      name.textContent = dir;
      name.title = dir;
      row.appendChild(name);
      const rm = document.createElement("button");
      rm.className = "ui-btn";
      rm.dataset.variant = "ghost";
      rm.dataset.icon = "";
      rm.setAttribute("aria-label", `Remove ${dir}`);
      rm.innerHTML = icon("close");
      rm.addEventListener("click", () => {
        dirs.splice(dirs.indexOf(dir), 1);
        renderDirs();
      });
      row.appendChild(rm);
      dirRowsEl.appendChild(row);
    }
  }
  renderDirs();

  const dirInput = dialog.querySelector<HTMLInputElement>("#project-dir-input")!;
  function addDir() {
    const value = dirInput.value.trim();
    if (!value) return;
    if (!dirs.includes(value)) dirs.push(value);
    dirInput.value = "";
    renderDirs();
  }
  dirInput.addEventListener("change", addDir);
  dirInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.stopPropagation();
      addDir();
    }
  });

  // ── Save / close ──
  const close = () => backdrop.remove();

  async function save() {
    const name = nameInput.value.trim();
    if (!name) {
      nameInput.setAttribute("aria-invalid", "true");
      nameInput.focus();
      return;
    }
    addDir();
    // Saved order first (still-selected members keep their position), new
    // ticks and directories append at the end.
    const selected = new Set<string>([
      ...workspaceRows.filter((r) => r.checked).map((r) => r.path),
      ...dirs,
    ]);
    const members: { path: string }[] = [];
    for (const m of project?.members ?? []) {
      if (selected.delete(m.path)) members.push({ path: m.path });
    }
    for (const path of selected) members.push({ path });

    const payload: Project = {
      id: project?.id ?? null,
      name,
      color,
      order: project?.order ?? 0, // backend assigns max+1 for new projects
      members,
    };
    try {
      await ipc.saveProject(payload);
      toast(project ? `Project "${name}" saved` : `Project "${name}" created`, "success");
      close();
      await onSaved();
    } catch (err) {
      reportError("Failed to save project", err);
    }
  }

  dialog.querySelector("#project-save")!.addEventListener("click", () => void save());
  dialog.querySelector("#project-cancel")!.addEventListener("click", close);
  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  nameInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void save();
  });
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  backdrop.setAttribute("tabindex", "0");

  document.body.appendChild(backdrop);
  attachPathPicker(dirInput, { directory: true, title: "Add directory to project" });
  nameInput.focus();
}
