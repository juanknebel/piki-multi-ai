/** Create / edit a project: name, one of the 10 palette swatches, and the
 *  member list.
 *
 *  Members are edited as two lists, not as a wall of checkboxes: what is
 *  already in the project (in order, each with a ✕) on top, and everything
 *  that could join it — every workspace not yet a member, filtered by a
 *  search box, plus any directory picked from disk — below. Both lists
 *  print the member's directory under its name, in FULL (wrapped if it
 *  has to be): several workspaces of the same repo are told apart by
 *  their path and nothing else, so the path can neither live in a tooltip
 *  alone nor be cut short. */
import * as ipc from "../../ipc";
import { appState } from "../../state";
import type { Project } from "../../types";
import { icon } from "../icons";
import { branchLabel, homeRelative } from "../../labels";
import { getHomeDir } from "../../home-dir";
import { pickPath } from "../path-picker";
import { attachDialogResize } from "../dialog-resize";
import { reportError, toast } from "../toast";

const PALETTE_LEN = 10;

/** Name / branch / directory for one member path, resolved against the live
 *  workspace list (a path with no workspace is a plain directory). */
function describeMember(path: string) {
  const ws = appState.workspaces.find((w) => w.info.path === path);
  const name = ws ? ws.info.name : path.replace(/\/+$/, "").split("/").pop() || path;
  const kind = ws ? ws.info.workspace_type.toLowerCase() : "directory";
  const branch = ws?.branch ? branchLabel(ws.branch) : null;
  return { ws, name, kind, branch };
}

export function showProjectDialog(project: Project | null, onSaved: () => void | Promise<void>) {
  document.querySelector(".projects-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop projects-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog ui-surface project-dialog";

  let color = project ? project.color % PALETTE_LEN : 0;
  /** The project's members, in order — the single source of truth for both
   *  lists and for what gets saved. */
  const members: string[] = (project?.members ?? []).map((m) => m.path);

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
      <div class="dialog-field project-members-field">
        <span class="dialog-label" id="project-members-label">Members <span class="project-count-badge" id="project-member-count"></span></span>
        <div class="project-member-list" id="project-members" role="list" aria-labelledby="project-members-label"></div>
      </div>
      <div class="dialog-field project-add-field">
        <span class="dialog-label" id="project-add-label">Add</span>
        <input class="ui-input" id="project-add-filter" type="text" placeholder="Filter workspaces, or type a path…" aria-labelledby="project-add-label" />
        <div class="project-candidate-list" id="project-candidates" role="list" aria-labelledby="project-add-label"></div>
        <button data-variant="secondary" data-size="sm" class="ui-btn project-browse-btn" id="project-browse">Browse for a directory…</button>
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

  // ── Member / candidate rows ──
  const membersEl = dialog.querySelector<HTMLElement>("#project-members")!;
  const countEl = dialog.querySelector<HTMLElement>("#project-member-count")!;
  const candidatesEl = dialog.querySelector<HTMLElement>("#project-candidates")!;
  const filterInput = dialog.querySelector<HTMLInputElement>("#project-add-filter")!;

  /** One row of either list: name · branch on top, the directory below,
   *  and a single-purpose button on the right (✕ removes, + adds). */
  function buildRow(path: string, action: "remove" | "add"): HTMLElement {
    const { ws, name, kind, branch } = describeMember(path);
    const row = document.createElement("div");
    row.className = `project-pick-row${ws ? "" : " directory"}`;
    row.setAttribute("role", "listitem");
    row.dataset.path = path;

    const text = document.createElement("div");
    text.className = "project-pick-text";
    const head = document.createElement("div");
    head.className = "project-pick-head";
    const nameEl = document.createElement("span");
    nameEl.className = "project-pick-name";
    nameEl.textContent = name;
    head.appendChild(nameEl);
    const meta = document.createElement("span");
    meta.className = "project-pick-meta";
    meta.textContent = branch ? `${kind} · ${branch}` : kind;
    head.appendChild(meta);
    text.appendChild(head);
    const pathEl = document.createElement("div");
    pathEl.className = "project-pick-path";
    // The FULL path (only `~`-shortened), never a character-capped label:
    // the row wraps it instead, so a deep worktree
    // (`~/.local/share/piki-multi/worktrees/…`) is readable to its last
    // segment — that tail is the only thing telling two checkouts apart,
    // and a fixed cap elided it even in a dialog with room to spare.
    pathEl.textContent = homeRelative(path, getHomeDir());
    text.appendChild(pathEl);
    row.appendChild(text);
    row.title = `${name}${branch ? ` · ${branch}` : ""}\n${path}`;

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "ui-btn";
    btn.dataset.variant = "ghost";
    btn.dataset.icon = "";
    const adding = action === "add";
    btn.title = adding ? `Add ${name}` : `Remove ${name}`;
    btn.setAttribute("aria-label", btn.title);
    btn.innerHTML = icon(adding ? "plus" : "close");
    btn.addEventListener("click", () => (adding ? addMember(path) : removeMember(path)));
    row.appendChild(btn);

    // The whole row is the target for adding: a candidate list is a menu of
    // things to pick, and hunting for a 22px button in it is the fiddly
    // part this dialog was losing people on.
    if (adding) {
      row.classList.add("clickable");
      row.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).closest("button")) return;
        addMember(path);
      });
    }
    return row;
  }

  function addMember(path: string) {
    const clean = path.replace(/\/+$/, "") || path;
    if (!members.includes(clean)) members.push(clean);
    filterInput.value = "";
    renderMembers();
    renderCandidates();
  }

  function removeMember(path: string) {
    const i = members.indexOf(path);
    if (i >= 0) members.splice(i, 1);
    renderMembers();
    renderCandidates();
  }

  function renderMembers() {
    membersEl.innerHTML = "";
    countEl.textContent = String(members.length);
    if (members.length === 0) {
      const hint = document.createElement("span");
      hint.className = "project-picker-hint";
      hint.textContent = "No members yet — add workspaces or a directory below.";
      membersEl.appendChild(hint);
      return;
    }
    for (const path of members) membersEl.appendChild(buildRow(path, "remove"));
  }

  /** Workspaces that are not members yet, matched against the filter by
   *  name, branch AND path — the path is how two checkouts of one repo are
   *  told apart, so it has to be searchable. */
  function candidatePaths(): string[] {
    const q = filterInput.value.trim().toLowerCase();
    return appState.workspaces
      .map((w) => w.info)
      .filter((info) => !members.includes(info.path))
      .filter((info) => {
        if (!q) return true;
        const ws = appState.workspaces.find((w) => w.info.path === info.path);
        return (
          info.name.toLowerCase().includes(q) ||
          info.path.toLowerCase().includes(q) ||
          (ws?.branch ?? "").toLowerCase().includes(q)
        );
      })
      .map((info) => info.path);
  }

  /** A typed absolute path (or `~/…`) that is not a known workspace can be
   *  added as a plain directory straight from the filter box. */
  function typedDirectory(): string | null {
    const raw = filterInput.value.trim();
    if (!raw.startsWith("/") && !raw.startsWith("~")) return null;
    const home = getHomeDir();
    const path = raw.startsWith("~") && home ? `${home}${raw.slice(1)}` : raw;
    const clean = path.replace(/\/+$/, "") || path;
    if (members.includes(clean)) return null;
    return clean;
  }

  function renderCandidates() {
    candidatesEl.innerHTML = "";
    const typed = typedDirectory();
    if (typed) candidatesEl.appendChild(buildRow(typed, "add"));
    const paths = candidatePaths();
    for (const path of paths) candidatesEl.appendChild(buildRow(path, "add"));
    if (!typed && paths.length === 0) {
      const hint = document.createElement("span");
      hint.className = "project-picker-hint";
      hint.textContent =
        appState.workspaces.length === 0
          ? "No workspaces yet — browse for a directory below."
          : filterInput.value.trim()
            ? "No workspace matches — type a full path to add a directory."
            : "Every workspace is already a member.";
      candidatesEl.appendChild(hint);
    }
  }

  renderMembers();
  renderCandidates();

  filterInput.addEventListener("input", renderCandidates);
  filterInput.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    // Enter adds the first row of the list the filter is pointing at, so a
    // repo can be added without touching the mouse.
    e.stopPropagation();
    e.preventDefault();
    const first = candidatesEl.querySelector<HTMLElement>(".project-pick-row")?.dataset.path;
    if (first) addMember(first);
  });

  dialog.querySelector("#project-browse")!.addEventListener("click", () => {
    void (async () => {
      const picked = await pickPath({ directory: true, title: "Add directory to project" });
      if (picked) addMember(picked);
    })();
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
    const payload: Project = {
      id: project?.id ?? null,
      name,
      color,
      order: project?.order ?? 0, // backend assigns max+1 for new projects
      members: members.map((path) => ({ path, kind: "Auto" as const })),
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
  attachDialogResize(dialog, "project");
  nameInput.focus();
}
