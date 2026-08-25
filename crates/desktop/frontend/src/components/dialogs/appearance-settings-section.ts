// Settings ▸ Appearance — a door to the Theme dialog (colours live there),
// the density (`density.ts`, `data-density` on <html>) and the UI zoom: ONE
// control over the same levels Ctrl+= / Ctrl+- step through (`ui-zoom.ts`),
// so the dropdown and the keys never disagree.

import { DENSITIES, DENSITY_DEFAULT, DENSITY_LABELS, applyDensity, getDensity } from "../../density";
import { getShortcutKey } from "../../shortcuts";
import { applyUiZoom, getUiZoom } from "../../ui-zoom";
import { ZOOM_DEFAULT, ZOOM_LEVELS, formatZoom } from "../../zoom";
import { createDropdown, type DropdownOption } from "../dropdown";
import { showThemeDialog } from "./theme-dialog";
import { settingsGrid, settingsHint, settingsSection, type SettingsSection } from "./settings-controls";

function zoomOptions(current: number): DropdownOption[] {
  const levels = ZOOM_LEVELS.includes(current) ? [...ZOOM_LEVELS] : [...ZOOM_LEVELS, current].sort((a, b) => a - b);
  return levels.map((z) => ({
    value: String(z),
    label: z === ZOOM_DEFAULT ? `${formatZoom(z)} (default)` : formatZoom(z),
  }));
}

export function buildAppearanceSettingsSection(): SettingsSection {
  const el = document.createElement("div");
  el.className = "settings-tab-appearance";

  // ── Theme ──
  const theme = settingsSection("Theme");
  const { row: tRow } = settingsGrid(theme);
  const openTheme = document.createElement("button");
  openTheme.className = "ui-btn";
  openTheme.dataset.variant = "secondary";
  openTheme.type = "button";
  openTheme.textContent = "Open theme editor…";
  openTheme.addEventListener("click", () => showThemeDialog());
  tRow("Colours", openTheme);
  theme.appendChild(
    settingsHint(`Presets, every colour and the terminal palette are edited in the Theme dialog (${getShortcutKey("theme")}).`),
  );
  el.appendChild(theme);

  // ── Layout ──
  const layout = settingsSection("Layout");
  const { row: lRow } = settingsGrid(layout);

  const density = createDropdown(
    DENSITIES.map((d) => ({ value: d, label: d === DENSITY_DEFAULT ? `${DENSITY_LABELS[d]} (default)` : DENSITY_LABELS[d] })),
    getDensity(),
  );
  density.container.addEventListener("change", () => applyDensity(density.value));
  lRow("Density", density.container);

  let zoom = createDropdown(zoomOptions(getUiZoom()), String(getUiZoom()));
  const zoomCell = lRow("UI zoom", zoom.container);
  const bindZoom = () => {
    zoom.container.addEventListener("change", () => applyUiZoom(Number(zoom.value)));
  };
  bindZoom();
  const syncZoom = () => {
    // Rebuild so a level reached with the keys (possibly off-list after a
    // persisted custom value) is selectable and shown.
    const fresh = createDropdown(zoomOptions(getUiZoom()), String(getUiZoom()));
    zoomCell.replaceChild(fresh.container, zoom.container);
    zoom = fresh;
    bindZoom();
  };

  layout.appendChild(
    settingsHint(
      `Zoom scales the whole chrome and the terminal font (base size in the Terminal tab); ${getShortcutKey("zoom-in")} / ${getShortcutKey("zoom-out")} / ${getShortcutKey("zoom-reset")} step through the same levels.`,
    ),
  );
  el.appendChild(layout);

  return {
    el,
    reset() {
      applyDensity(DENSITY_DEFAULT);
      density.value = DENSITY_DEFAULT;
      applyUiZoom(ZOOM_DEFAULT);
      syncZoom();
    },
    focus() {
      openTheme.focus();
    },
  };
}
