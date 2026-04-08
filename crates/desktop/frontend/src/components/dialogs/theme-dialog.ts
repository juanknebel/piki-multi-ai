import {
  themeEngine,
  COLOR_GROUPS,
  keyToLabel,
  isValidHex,
  type ThemeColorKey,
} from "../../theme";
import { toast } from "../toast";

export function showThemeDialog() {
  document.querySelector(".theme-dialog-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop theme-dialog-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "560px";
  dialog.style.maxHeight = "85vh";

  // Build header
  const header = document.createElement("div");
  header.className = "dialog-header";
  header.innerHTML = `
    <span class="dialog-title">Theme Settings</span>
    <button class="dialog-close">×</button>
  `;

  // Build body
  const body = document.createElement("div");
  body.className = "dialog-body";
  body.style.overflowY = "auto";
  body.style.maxHeight = "62vh";

  // Preset selector (custom dropdown — native <select> renders transparent in WebKit)
  const presetRow = document.createElement("div");
  presetRow.className = "theme-preset-row";
  const presets = themeEngine.getPresets();

  const presetLabel = document.createElement("span");
  presetLabel.className = "theme-preset-label";
  presetLabel.textContent = "Preset";

  const presetDropdown = document.createElement("div");
  presetDropdown.className = "theme-preset-dropdown";

  const presetTrigger = document.createElement("button");
  presetTrigger.className = "theme-preset-trigger";
  presetTrigger.dataset.value = themeEngine.getActivePresetId();
  const activePreset = presets.find((p) => p.id === themeEngine.getActivePresetId());
  presetTrigger.innerHTML = `<span class="theme-preset-trigger-text">${activePreset?.name ?? ""}</span><span class="theme-preset-trigger-arrow">▾</span>`;

  presetDropdown.appendChild(presetTrigger);
  presetRow.appendChild(presetLabel);
  presetRow.appendChild(presetDropdown);
  body.appendChild(presetRow);

  // Color groups
  const colorRows = new Map<ThemeColorKey, { picker: HTMLInputElement; hex: HTMLInputElement; row: HTMLElement }>();

  for (const group of COLOR_GROUPS) {
    const section = document.createElement("div");
    section.className = "theme-color-group";

    const groupHeader = document.createElement("div");
    groupHeader.className = "theme-group-header";
    groupHeader.innerHTML = `
      <svg class="theme-group-chevron" viewBox="0 0 16 16">
        <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/>
      </svg>
      ${group.label} (${group.keys.length})
    `;

    const groupBody = document.createElement("div");
    groupBody.className = "theme-group-body";

    // Collapse by default except first group
    const startCollapsed = group.label !== "Backgrounds";
    if (startCollapsed) {
      groupBody.classList.add("collapsed");
      groupHeader.querySelector(".theme-group-chevron")!.classList.add("collapsed");
    }

    groupHeader.addEventListener("click", () => {
      groupBody.classList.toggle("collapsed");
      groupHeader.querySelector(".theme-group-chevron")!.classList.toggle("collapsed");
    });

    for (const key of group.keys) {
      const value = themeEngine.getEffectiveColor(key);
      const isOverridden = themeEngine.hasOverride(key);

      const row = document.createElement("div");
      row.className = `theme-color-row${isOverridden ? " overridden" : ""}`;
      row.innerHTML = `
        <span class="theme-color-label">${keyToLabel(key)}</span>
        <input type="color" class="theme-color-picker" value="${value}" />
        <input type="text" class="theme-color-hex" value="${value}" maxlength="7" spellcheck="false" />
        <button class="theme-color-reset" title="Reset to preset">↺</button>
      `;

      const picker = row.querySelector<HTMLInputElement>(".theme-color-picker")!;
      const hex = row.querySelector<HTMLInputElement>(".theme-color-hex")!;
      const resetBtn = row.querySelector<HTMLButtonElement>(".theme-color-reset")!;

      // Color picker → apply
      picker.addEventListener("input", () => {
        hex.value = picker.value;
        themeEngine.setColorOverride(key, picker.value);
        row.classList.add("overridden");
      });

      // Hex input → apply
      hex.addEventListener("input", () => {
        let v = hex.value;
        if (!v.startsWith("#")) v = "#" + v;
        if (isValidHex(v)) {
          picker.value = v;
          themeEngine.setColorOverride(key, v);
          row.classList.add("overridden");
        }
      });

      // Reset single color
      resetBtn.addEventListener("click", () => {
        themeEngine.clearSingleOverride(key);
        const newVal = themeEngine.getEffectiveColor(key);
        picker.value = newVal;
        hex.value = newVal;
        row.classList.remove("overridden");
      });

      colorRows.set(key, { picker, hex, row });
      groupBody.appendChild(row);
    }

    section.appendChild(groupHeader);
    section.appendChild(groupBody);
    body.appendChild(section);
  }

  // Preset dropdown logic
  function applyPreset(presetId: string) {
    themeEngine.setPreset(presetId);
    const p = presets.find((pr) => pr.id === presetId);
    presetTrigger.dataset.value = presetId;
    presetTrigger.querySelector(".theme-preset-trigger-text")!.textContent = p?.name ?? presetId;
    for (const [key, { picker, hex, row }] of colorRows) {
      const val = themeEngine.getEffectiveColor(key);
      picker.value = val;
      hex.value = val;
      row.classList.remove("overridden");
    }
  }

  presetTrigger.addEventListener("click", (e) => {
    e.stopPropagation();
    const existing = presetDropdown.querySelector(".theme-preset-list");
    if (existing) { existing.remove(); return; }

    const list = document.createElement("div");
    list.className = "theme-preset-list";

    for (const p of presets) {
      const item = document.createElement("button");
      item.className = `theme-preset-item${p.id === presetTrigger.dataset.value ? " active" : ""}`;
      item.textContent = p.name;
      item.addEventListener("click", (ev) => {
        ev.stopPropagation();
        applyPreset(p.id);
        list.remove();
      });
      list.appendChild(item);
    }

    presetDropdown.appendChild(list);

    const closeList = (ev: MouseEvent) => {
      if (!list.contains(ev.target as Node) && ev.target !== presetTrigger) {
        list.remove();
        document.removeEventListener("click", closeList);
      }
    };
    setTimeout(() => document.addEventListener("click", closeList), 0);
  });

  // Footer
  const footer = document.createElement("div");
  footer.className = "dialog-footer";
  footer.innerHTML = `
    <span class="theme-footer-left">
      <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="theme-import">Import</button>
      <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="theme-export">Export</button>
    </span>
    <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="theme-reset-all">Reset All</button>
    <button class="dialog-btn dialog-btn-secondary" id="theme-close">Close</button>
  `;

  // Assemble dialog
  dialog.appendChild(header);
  dialog.appendChild(body);
  dialog.appendChild(footer);
  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  // ── Event handlers ─────────────────────────────

  const close = () => backdrop.remove();
  header.querySelector(".dialog-close")!.addEventListener("click", close);
  footer.querySelector("#theme-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();

  // Reset all overrides
  footer.querySelector("#theme-reset-all")!.addEventListener("click", () => {
    themeEngine.clearOverrides();
    for (const [key, { picker, hex, row }] of colorRows) {
      const val = themeEngine.getEffectiveColor(key);
      picker.value = val;
      hex.value = val;
      row.classList.remove("overridden");
    }
  });

  // Export
  footer.querySelector("#theme-export")!.addEventListener("click", () => {
    const json = themeEngine.exportTheme();
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `piki-theme-${themeEngine.getActivePresetId()}.json`;
    a.click();
    URL.revokeObjectURL(url);
    toast("Theme exported", "success");
  });

  // Import
  footer.querySelector("#theme-import")!.addEventListener("click", () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = () => {
        themeEngine.importTheme(reader.result as string);
        // Update UI to reflect imported theme
        presetTrigger.dataset.value = themeEngine.getActivePresetId();
        const importedPreset = presets.find((pr) => pr.id === themeEngine.getActivePresetId());
        presetTrigger.querySelector(".theme-preset-trigger-text")!.textContent = importedPreset?.name ?? "";
        const overrides = themeEngine.getOverrides();
        for (const [key, { picker, hex, row }] of colorRows) {
          const val = themeEngine.getEffectiveColor(key);
          picker.value = val;
          hex.value = val;
          row.classList.toggle("overridden", key in overrides);
        }
        toast("Theme imported", "success");
      };
      reader.readAsText(file);
    });
    input.click();
  });
}
