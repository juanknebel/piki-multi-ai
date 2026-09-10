// UI zoom — applies a level from zoom.ts to the document and persists it.
// `--ui-zoom` on <html> drives `--font-size-base`, and every rem token in
// variables.css (type scale, spacing, bar heights, activity bar) follows; the
// xterm font size follows through `themeEngine.applyTerminalZoom`, and a
// terminal created later reads `--ui-zoom` itself (terminal-panel.ts).
// Bound to the zoom-in / zoom-out / zoom-reset shortcuts (and their
// terminal-safe Ctrl+Shift twins) in main.ts; the View menu and the palette
// call the same three functions.

import { settingsStore } from "./settings";
import { themeEngine } from "./theme";
import { toast } from "./components/toast";
import { ZOOM_DEFAULT, clampZoom, formatZoom, stepZoom } from "./zoom";

const SETTINGS_KEY = "uiZoom";

let current = ZOOM_DEFAULT;

export function getUiZoom(): number {
  return current;
}

/** Apply `zoom` to the chrome and the terminals; persist unless told not to. */
export function applyUiZoom(zoom: number, persist = true): void {
  current = clampZoom(zoom);
  document.documentElement.style.setProperty("--ui-zoom", String(current));
  themeEngine.applyTerminalZoom(current);
  if (persist) settingsStore.patch(SETTINGS_KEY, current === ZOOM_DEFAULT ? undefined : current);
}

/** Restore the persisted level at startup (after `settingsStore.load()`). */
export function initUiZoom(): void {
  applyUiZoom(settingsStore.get<number>(SETTINGS_KEY) ?? ZOOM_DEFAULT, false);
}

function announce(): void {
  toast(`Zoom ${formatZoom(current)}`, "info");
}

export function zoomIn(): void {
  applyUiZoom(stepZoom(current, 1));
  announce();
}

export function zoomOut(): void {
  applyUiZoom(stepZoom(current, -1));
  announce();
}

export function resetZoom(): void {
  applyUiZoom(ZOOM_DEFAULT);
  announce();
}
