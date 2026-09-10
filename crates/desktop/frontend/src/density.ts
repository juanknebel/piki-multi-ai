// UI density — Settings ▸ Appearance. Contract shared with the visual track
// (phase 15): the value is persisted under `appearance.density` and applied
// as `data-density="compact|comfortable"` on <html> (the attribute is
// REMOVED for `normal`); the CSS side (`:root[data-density="…"] { --density }`)
// lives in variables.css and is not this module's business.

import { settingsStore } from "./settings";

export const DENSITIES = ["compact", "normal", "comfortable"] as const;
export type Density = (typeof DENSITIES)[number];
export const DENSITY_DEFAULT: Density = "normal";
export const DENSITY_SETTINGS_KEY = "appearance.density";

export const DENSITY_LABELS: Record<Density, string> = {
  compact: "Compact",
  normal: "Normal",
  comfortable: "Comfortable",
};

/** A persisted / user-supplied value → a valid density (default for junk). */
export function normalizeDensity(value: unknown): Density {
  return typeof value === "string" && (DENSITIES as readonly string[]).includes(value)
    ? (value as Density)
    : DENSITY_DEFAULT;
}

let current: Density = DENSITY_DEFAULT;

export function getDensity(): Density {
  return current;
}

/** Stamp the attribute and persist (only non-default values are stored). */
export function applyDensity(value: unknown, persist = true): Density {
  current = normalizeDensity(value);
  if (current === DENSITY_DEFAULT) delete document.documentElement.dataset.density;
  else document.documentElement.dataset.density = current;
  if (persist) settingsStore.patch(DENSITY_SETTINGS_KEY, current === DENSITY_DEFAULT ? undefined : current);
  return current;
}

/** Restore the persisted density at startup (after `settingsStore.load()`). */
export function initDensity(): void {
  applyDensity(settingsStore.get(DENSITY_SETTINGS_KEY), false);
}
