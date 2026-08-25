// Settings ▸ Shortcuts — the registry (`shortcuts.ts`) grouped by category
// in the Help dialog's order, a filter box (label / key / category) and two
// kinds of flag per row: a conflict (the same combo bound elsewhere —
// `findConflicts`, pure) and a demotion (a `terminalCapture` def rebound to
// a chord the terminal owns, so it only fires outside it). Rebinding is the
// press-keys button from before, on the primitives.

import { icon } from "../icons";
import {
  CATEGORY_ORDER,
  eventToCombo,
  findConflict,
  formatShortcut,
  getReservedCombos,
  getShortcuts,
  isDemotedShortcut,
  isOutsideOnly,
  isTerminalSafeCombo,
  resetAllShortcuts,
  updateShortcut,
  type ShortcutDef,
} from "../../shortcuts";
import { findConflicts, groupByCategory, matchesShortcutQuery } from "../../shortcut-table";
import { toast } from "../toast";
import { escapeHtml, type SettingsSection } from "./settings-controls";

export const OUTSIDE_ONLY_NOTE = "Fires only when focus is outside a terminal or editor";

export function buildShortcutsSettingsSection(): SettingsSection {
  const el = document.createElement("div");
  el.className = "settings-tab-shortcuts";

  // ── Toolbar: filter + per-tab reset ──
  const toolbar = document.createElement("div");
  toolbar.className = "settings-toolbar";
  const filter = document.createElement("input");
  filter.type = "search";
  filter.className = "settings-filter ui-input";
  filter.dataset.size = "sm";
  filter.placeholder = "Filter by action, key or category";
  filter.setAttribute("aria-label", "Filter shortcuts");
  toolbar.appendChild(filter);
  const resetBtn = document.createElement("button");
  resetBtn.type = "button";
  resetBtn.className = "ui-btn";
  resetBtn.dataset.variant = "ghost";
  resetBtn.dataset.size = "sm";
  resetBtn.textContent = "Reset shortcuts";
  resetBtn.title = "Every shortcut back to its default key";
  toolbar.appendChild(resetBtn);
  el.appendChild(toolbar);

  const table = document.createElement("div");
  table.className = "settings-shortcuts-table";
  el.appendChild(table);

  const legend = document.createElement("p");
  legend.className = "settings-legend";
  el.appendChild(legend);

  const render = () => {
    const defs = getShortcuts();
    const conflicts = findConflicts(defs, getReservedCombos());
    const query = filter.value;
    const visible = defs.filter((d) => matchesShortcutQuery(d, query, formatShortcut(d.key)));
    const groups = groupByCategory(visible, CATEGORY_ORDER);

    table.replaceChildren();
    let anyOutside = false;
    let anyConflict = false;
    let anyDemoted = false;

    if (groups.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ui-empty";
      empty.textContent = `No shortcut matches "${query.trim()}"`;
      table.appendChild(empty);
    }

    for (const group of groups) {
      const g = document.createElement("div");
      g.className = "settings-shortcut-group";
      const title = document.createElement("div");
      title.className = "settings-shortcut-group-title";
      title.textContent = group.category;
      g.appendChild(title);

      const head = document.createElement("div");
      head.className = "settings-shortcut-row settings-shortcut-header";
      head.innerHTML = `
        <span class="settings-col-action">Action</span>
        <span class="settings-col-default">Default</span>
        <span class="settings-col-current">Current</span>`;
      g.appendChild(head);

      for (const def of group.items) {
        const outside = isOutsideOnly(def);
        const demoted = isDemotedShortcut(def);
        const others = conflicts.get(def.id);
        anyOutside ||= outside;
        anyDemoted ||= demoted;
        anyConflict ||= !!others;
        g.appendChild(row(def, { outside, demoted, others }));
      }
      table.appendChild(g);
    }

    const notes: string[] = [];
    if (anyOutside) notes.push(`° ${OUTSIDE_ONLY_NOTE} — the terminal keeps every key it can use.`);
    if (anyConflict) notes.push("The same key is bound to another action; only the first one in this list fires.");
    if (anyDemoted) notes.push("! Bound to a key the terminal owns (not Alt+… / Ctrl+Shift+…), so it fires outside the terminal only.");
    legend.textContent = notes.join(" ");
    legend.hidden = notes.length === 0;
  };

  const row = (def: ShortcutDef, flags: { outside: boolean; demoted: boolean; others?: string[] }) => {
    const r = document.createElement("div");
    r.className = "settings-shortcut-row";
    r.dataset.id = def.id;

    const actionCol = document.createElement("span");
    actionCol.className = "settings-col-action";
    let html = escapeHtml(def.label);
    if (flags.outside) html += `<span class="shortcut-row-note" title="${OUTSIDE_ONLY_NOTE}">°</span>`;
    if (flags.others) {
      const who = flags.others.join(", ");
      html += `<span class="settings-flag" data-kind="conflict" role="img" aria-label="Conflict: also bound to ${escapeHtml(who)}" title="${escapeHtml(`Also bound to: ${who}`)}">${icon("warning")}</span>`;
    }
    if (flags.demoted) {
      const msg = `"${formatShortcut(def.key)}" belongs to the terminal — "${def.label}" will only fire when focus is outside it`;
      html += `<span class="settings-flag" data-kind="demoted" role="img" aria-label="${escapeHtml(msg)}" title="${escapeHtml(msg)}">!</span>`;
    }
    actionCol.innerHTML = html;

    const defaultCol = document.createElement("span");
    defaultCol.className = "settings-col-default";
    defaultCol.innerHTML = `<kbd>${escapeHtml(formatShortcut(def.defaultKey))}</kbd>`;

    const currentCol = document.createElement("span");
    currentCol.className = "settings-col-current";
    const keyBtn = document.createElement("button");
    keyBtn.type = "button";
    keyBtn.className = "settings-key-btn ui-btn";
    keyBtn.dataset.variant = "secondary";
    keyBtn.dataset.size = "sm";
    keyBtn.textContent = formatShortcut(def.key);
    keyBtn.title = `Change the key for "${def.label}" (Esc cancels)`;
    if (def.key !== def.defaultKey) keyBtn.classList.add("modified");
    if (flags.others) keyBtn.setAttribute("aria-invalid", "true");
    keyBtn.addEventListener("click", () => record(def, keyBtn));
    currentCol.appendChild(keyBtn);

    r.appendChild(actionCol);
    r.appendChild(defaultCol);
    r.appendChild(currentCol);
    return r;
  };

  const record = (def: ShortcutDef, keyBtn: HTMLButtonElement) => {
    keyBtn.textContent = "Press keys…";
    keyBtn.classList.add("recording");

    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const combo = eventToCombo(e);
      if (!combo) return; // modifier-only press

      const done = () => document.removeEventListener("keydown", handler, true);
      if (e.key === "Escape") {
        keyBtn.textContent = formatShortcut(def.key);
        keyBtn.classList.remove("recording");
        done();
        return;
      }

      const conflict = findConflict(def.id, combo);
      if (conflict) {
        toast(`"${formatShortcut(combo)}" already used by "${conflict.label}"`, "error");
        return;
      }

      if (def.terminalCapture && !isTerminalSafeCombo(combo)) {
        toast(`"${formatShortcut(combo)}" belongs to the terminal — "${def.label}" will only fire when focus is outside it`, "info");
      }
      updateShortcut(def.id, combo);
      done();
      render();
      table.querySelector<HTMLButtonElement>(`.settings-shortcut-row[data-id="${def.id}"] .settings-key-btn`)?.focus();
    };

    document.addEventListener("keydown", handler, true);
  };

  filter.addEventListener("input", render);
  resetBtn.addEventListener("click", () => {
    resetAllShortcuts();
    render();
    toast("Shortcuts restored to defaults", "success");
  });

  render();

  return {
    el,
    reset() {
      resetAllShortcuts();
      filter.value = "";
      render();
    },
    focus() {
      filter.focus();
    },
  };
}
