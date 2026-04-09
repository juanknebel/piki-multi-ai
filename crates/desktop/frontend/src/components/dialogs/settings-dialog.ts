import {
  getShortcuts,
  updateShortcut,
  resetAllShortcuts,
  findConflict,
  eventToCombo,
  formatShortcut,
  getShellSetting,
  setShellSetting,
} from "../../shortcuts";
import { toast } from "../toast";

export async function showSettingsDialog() {
  document.querySelector(".settings-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop settings-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "640px";
  dialog.style.maxHeight = "80vh";

  // Header
  const header = document.createElement("div");
  header.className = "dialog-header";
  header.innerHTML = `
    <span class="dialog-title">Settings</span>
    <button class="dialog-close">&times;</button>
  `;

  // Body
  const body = document.createElement("div");
  body.className = "dialog-body";
  body.style.overflowY = "auto";
  body.style.padding = "0";

  // ── Shell section ──
  const shellSection = document.createElement("div");
  shellSection.className = "settings-section";

  const currentShell = await getShellSetting();
  const envShell = "$SHELL";

  shellSection.innerHTML = `
    <div class="settings-section-title">Shell</div>
    <div class="settings-shell-row">
      <label class="settings-label">Terminal shell command</label>
      <input class="settings-shell-input" type="text" value="${escAttr(currentShell)}" placeholder="Default: ${envShell}" />
    </div>
    <div class="settings-hint">Leave empty to use system default ($SHELL). Changes apply to new Shell tabs.</div>
  `;

  const shellInput = shellSection.querySelector<HTMLInputElement>(".settings-shell-input")!;
  let shellTimer: ReturnType<typeof setTimeout> | null = null;
  shellInput.addEventListener("input", () => {
    if (shellTimer) clearTimeout(shellTimer);
    shellTimer = setTimeout(() => {
      setShellSetting(shellInput.value.trim());
    }, 500);
  });

  body.appendChild(shellSection);

  // ── Shortcuts section ──
  const shortcutsSection = document.createElement("div");
  shortcutsSection.className = "settings-section";
  shortcutsSection.innerHTML = `<div class="settings-section-title">Keyboard Shortcuts</div>`;

  const table = document.createElement("div");
  table.className = "settings-shortcuts-table";

  // Header row
  const headerRow = document.createElement("div");
  headerRow.className = "settings-shortcut-row settings-shortcut-header";
  headerRow.innerHTML = `
    <span class="settings-col-action">Action</span>
    <span class="settings-col-default">Default</span>
    <span class="settings-col-current">Current</span>
  `;
  table.appendChild(headerRow);

  const shortcuts = getShortcuts();

  for (const def of shortcuts) {
    const row = document.createElement("div");
    row.className = "settings-shortcut-row";

    const actionCol = document.createElement("span");
    actionCol.className = "settings-col-action";
    actionCol.textContent = def.label;

    const defaultCol = document.createElement("span");
    defaultCol.className = "settings-col-default";
    defaultCol.innerHTML = `<kbd>${esc(formatShortcut(def.defaultKey))}</kbd>`;

    const currentCol = document.createElement("span");
    currentCol.className = "settings-col-current";
    const keyBtn = document.createElement("button");
    keyBtn.className = "settings-key-btn";
    keyBtn.textContent = formatShortcut(def.key);
    if (def.key !== def.defaultKey) keyBtn.classList.add("modified");

    keyBtn.addEventListener("click", () => {
      keyBtn.textContent = "Press keys...";
      keyBtn.classList.add("recording");

      const handler = (e: KeyboardEvent) => {
        e.preventDefault();
        e.stopPropagation();

        const combo = eventToCombo(e);
        if (!combo) return; // modifier-only press

        if (e.key === "Escape") {
          keyBtn.textContent = formatShortcut(def.key);
          keyBtn.classList.remove("recording");
          document.removeEventListener("keydown", handler, true);
          return;
        }

        const conflict = findConflict(def.id, combo);
        if (conflict) {
          toast(`"${combo}" already used by "${conflict.label}"`, "error");
          return;
        }

        updateShortcut(def.id, combo);
        keyBtn.textContent = formatShortcut(combo);
        keyBtn.classList.remove("recording");
        keyBtn.classList.toggle("modified", combo !== def.defaultKey);
        document.removeEventListener("keydown", handler, true);
      };

      document.addEventListener("keydown", handler, true);
    });

    currentCol.appendChild(keyBtn);

    row.appendChild(actionCol);
    row.appendChild(defaultCol);
    row.appendChild(currentCol);
    table.appendChild(row);
  }

  shortcutsSection.appendChild(table);
  body.appendChild(shortcutsSection);

  // Footer
  const footer = document.createElement("div");
  footer.className = "dialog-footer";
  footer.innerHTML = `
    <button class="dialog-btn dialog-btn-danger" id="settings-reset">Restore Defaults</button>
    <button class="dialog-btn dialog-btn-secondary" id="settings-close">Close</button>
  `;

  dialog.appendChild(header);
  dialog.appendChild(body);
  dialog.appendChild(footer);
  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();

  header.querySelector(".dialog-close")!.addEventListener("click", close);
  footer.querySelector("#settings-close")!.addEventListener("click", close);

  footer.querySelector("#settings-reset")!.addEventListener("click", () => {
    resetAllShortcuts();
    shellInput.value = "";
    setShellSetting("");
    // Re-render shortcut keys
    const btns = table.querySelectorAll<HTMLButtonElement>(".settings-key-btn");
    shortcuts.forEach((def, i) => {
      btns[i].textContent = formatShortcut(def.key);
      btns[i].classList.remove("modified");
    });
    toast("Settings restored to defaults", "success");
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    // Only close on Escape if not recording a shortcut
    if (e.key === "Escape" && !document.querySelector(".settings-key-btn.recording")) {
      close();
    }
  });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

function esc(s: string): string {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}

function escAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
