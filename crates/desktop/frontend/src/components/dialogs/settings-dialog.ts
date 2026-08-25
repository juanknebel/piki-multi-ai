// Settings dialog (Ctrl+, / Edit ▸ Settings / palette): a left tab rail —
// General / Appearance / Terminal / Shortcuts — and one panel. Each tab is a
// `SettingsSection` (settings-controls.ts) built by its own module; this
// file only mounts them, remembers the last tab (`settingsTab` in the
// settings store), routes the footer's "Reset this tab" / "Restore Defaults"
// and owns the keyboard: Esc closes (not while a key is being recorded),
// ↑/↓/Home/End move along the rail, initial focus lands on the active
// tab's first control, focus is restored on close.
//
// What "Restore Defaults" does NOT touch, on purpose: the terminal shell
// command (a machine-specific path) and the provider binaries
// (providers.toml, Manage Providers). The confirm says so.

import { getShellSetting, setShellSetting } from "../../shortcuts";
import { settingsStore } from "../../settings";
import { showConfirm } from "../confirm";
import { toast } from "../toast";
import { attachPathPicker } from "../path-picker";
import { buildTerminalSettingsSection } from "./terminal-settings-section";
import { buildGeneralSettingsSection } from "./general-settings-section";
import { buildAppearanceSettingsSection } from "./appearance-settings-section";
import { buildShortcutsSettingsSection } from "./shortcuts-settings-section";
import { settingsHint, settingsSection, type SettingsSection } from "./settings-controls";

export type SettingsTabId = "general" | "appearance" | "terminal" | "shortcuts";

const TABS: { id: SettingsTabId; label: string }[] = [
  { id: "general", label: "General" },
  { id: "appearance", label: "Appearance" },
  { id: "terminal", label: "Terminal" },
  { id: "shortcuts", label: "Shortcuts" },
];

const LAST_TAB_KEY = "settingsTab";

function isTabId(v: unknown): v is SettingsTabId {
  return typeof v === "string" && TABS.some((t) => t.id === v);
}

export async function showSettingsDialog(initialTab?: SettingsTabId) {
  document.querySelector(".settings-backdrop")?.remove();
  const prevFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop settings-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog ui-surface settings-dialog";
  dialog.setAttribute("role", "dialog");
  dialog.setAttribute("aria-label", "Settings");

  const header = document.createElement("div");
  header.className = "ui-header";
  header.innerHTML = `
    <span class="ui-header-title">Settings</span>
    <div class="ui-header-actions">
      <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">&times;</button>
    </div>`;

  // ── Body: rail + panel ──
  const body = document.createElement("div");
  body.className = "settings-body";

  const rail = document.createElement("nav");
  rail.className = "settings-rail";
  rail.setAttribute("role", "tablist");
  rail.setAttribute("aria-label", "Settings sections");
  rail.setAttribute("aria-orientation", "vertical");

  const panel = document.createElement("div");
  panel.className = "settings-panel";
  panel.setAttribute("role", "tabpanel");

  body.appendChild(rail);
  body.appendChild(panel);

  const sections: Record<SettingsTabId, SettingsSection> = {
    general: buildGeneralSettingsSection(),
    appearance: buildAppearanceSettingsSection(),
    terminal: buildTerminalTab(),
    shortcuts: buildShortcutsSettingsSection(),
  };

  const tabButtons = new Map<SettingsTabId, HTMLButtonElement>();
  for (const t of TABS) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "settings-rail-tab";
    b.setAttribute("role", "tab");
    b.id = `settings-tab-${t.id}`;
    b.textContent = t.label;
    b.addEventListener("click", () => select(t.id, true));
    rail.appendChild(b);
    tabButtons.set(t.id, b);
    sections[t.id].el.hidden = true;
    panel.appendChild(sections[t.id].el);
  }

  let active: SettingsTabId = initialTab ?? (isTabId(settingsStore.get(LAST_TAB_KEY)) ? settingsStore.get<SettingsTabId>(LAST_TAB_KEY)! : "general");

  const select = (id: SettingsTabId, focusControl: boolean) => {
    active = id;
    for (const t of TABS) {
      const on = t.id === id;
      const b = tabButtons.get(t.id)!;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
      sections[t.id].el.hidden = !on;
    }
    panel.setAttribute("aria-labelledby", `settings-tab-${id}`);
    panel.scrollTop = 0;
    resetTabBtn.textContent = `Reset ${TABS.find((t) => t.id === id)!.label.toLowerCase()}`;
    settingsStore.patch(LAST_TAB_KEY, id === "general" ? undefined : id);
    if (focusControl) {
      sections[id].focus();
      // A tab still loading has nothing to focus: keep focus inside the
      // dialog (Esc, Tab order) on its rail button rather than on <body>.
      if (!dialog.contains(document.activeElement)) tabButtons.get(id)!.focus();
    }
  };

  rail.addEventListener("keydown", (e) => {
    const idx = TABS.findIndex((t) => t.id === active);
    let next = -1;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") next = (idx + 1) % TABS.length;
    else if (e.key === "ArrowUp" || e.key === "ArrowLeft") next = (idx - 1 + TABS.length) % TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = TABS.length - 1;
    if (next < 0) return;
    e.preventDefault();
    select(TABS[next].id, false);
    tabButtons.get(TABS[next].id)!.focus();
  });

  // ── Footer ──
  const footer = document.createElement("div");
  footer.className = "dialog-footer settings-footer";
  const resetTabBtn = document.createElement("button");
  resetTabBtn.type = "button";
  resetTabBtn.className = "ui-btn";
  resetTabBtn.dataset.variant = "ghost";
  resetTabBtn.title = "Defaults for the current tab only";
  resetTabBtn.addEventListener("click", () => void resetTab(active));
  const spacer = document.createElement("span");
  spacer.className = "settings-footer-spacer";
  const restoreBtn = document.createElement("button");
  restoreBtn.type = "button";
  restoreBtn.className = "ui-btn";
  restoreBtn.dataset.variant = "danger";
  restoreBtn.textContent = "Restore Defaults";
  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "ui-btn";
  closeBtn.dataset.variant = "secondary";
  closeBtn.textContent = "Close";
  footer.append(resetTabBtn, spacer, restoreBtn, closeBtn);

  dialog.append(header, body, footer);
  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const close = () => {
    backdrop.remove();
    prevFocus?.focus();
  };
  header.querySelector(".dialog-close")!.addEventListener("click", close);
  closeBtn.addEventListener("click", close);

  const resetTab = async (id: SettingsTabId) => {
    await sections[id].reset();
    if (id !== "general") toast(`${TABS.find((t) => t.id === id)!.label} settings restored to defaults`, "success");
  };

  restoreBtn.addEventListener("click", () => {
    showConfirm({
      bodyHtml: `
        <p>Restore default settings?</p>
        <p class="ws-delete-hint">Resets every keyboard shortcut, the terminal look (font, size, line height, scrollback, cursor, copy on select), UI zoom and density, and the persistent-sessions / notification choices made here — config.toml applies to those again.</p>
        <p class="ws-delete-hint">Kept: the terminal shell command and the provider binaries (Manage Providers).</p>`,
      actions: [
        { label: "Cancel", kind: "secondary" },
        {
          label: "Restore defaults",
          kind: "danger",
          onSelect: () => {
            void (async () => {
              for (const t of TABS) await sections[t.id].reset();
              toast("Settings restored to defaults", "success");
              sections[active].focus();
            })();
          },
        },
      ],
    });
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    // Only close on Escape if not recording a shortcut
    if (e.key === "Escape" && !document.querySelector(".settings-key-btn.recording")) {
      e.stopPropagation();
      close();
    }
  });

  select(active, true);
}

/** Terminal tab: the shell command (kept by every reset — it is a path on
 *  this machine, not a preference) above the Settings ▸ Terminal section
 *  from terminal-settings-section.ts. */
function buildTerminalTab(): SettingsSection {
  const el = document.createElement("div");
  el.className = "settings-tab-terminal";

  const shell = settingsSection("Shell");
  const row = document.createElement("div");
  row.className = "settings-shell-row";
  const label = document.createElement("label");
  label.className = "settings-label";
  label.htmlFor = "settings-shell";
  label.textContent = "Terminal shell command";
  const shellInput = document.createElement("input");
  shellInput.id = "settings-shell";
  shellInput.className = "settings-shell-input ui-input";
  shellInput.type = "text";
  shellInput.value = getShellSetting();
  shellInput.placeholder = "Default: $SHELL";
  row.append(label, shellInput);
  shell.appendChild(row);
  attachPathPicker(shellInput, { directory: false, title: "Select shell binary" });
  let shellTimer: ReturnType<typeof setTimeout> | null = null;
  shellInput.addEventListener("input", () => {
    if (shellTimer) clearTimeout(shellTimer);
    shellTimer = setTimeout(() => setShellSetting(shellInput.value.trim()), 500);
  });
  shell.appendChild(settingsHint("Leave empty to use the system default ($SHELL). Applies to new Shell tabs. Never reset by Restore Defaults."));
  el.appendChild(shell);

  const terminal = buildTerminalSettingsSection();
  el.appendChild(terminal.el);

  return {
    el,
    reset() {
      terminal.reset();
    },
    focus() {
      shellInput.focus();
    },
  };
}
