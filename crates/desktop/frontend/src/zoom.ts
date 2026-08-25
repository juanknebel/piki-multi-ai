// Pure UI-zoom arithmetic (no DOM) — unit-tested in zoom.test.ts; ui-zoom.ts
// applies it to the document and persists it. One root value, `--ui-zoom`,
// scales the rem-based type scale, spacing and bar heights in variables.css;
// the terminal font size is derived from the same level so Ctrl+= grows the
// chrome and the shell together.

/** Discrete levels Ctrl+= / Ctrl+- step through (browser-like). */
export const ZOOM_LEVELS: readonly number[] = [0.7, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];
export const ZOOM_DEFAULT = 1;
export const ZOOM_MIN = ZOOM_LEVELS[0];
export const ZOOM_MAX = ZOOM_LEVELS[ZOOM_LEVELS.length - 1];

/** xterm `fontSize` at zoom 1 (terminal-panel.ts reads it through `terminalFontSizeFor`). */
export const TERMINAL_BASE_FONT_SIZE = 14;

/** A persisted / user-supplied value → a sane zoom (default 1 for junk). */
export function clampZoom(value: unknown): number {
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n) || n <= 0) return ZOOM_DEFAULT;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(n * 100) / 100));
}

/** Next level above (+1) or below (-1) `current`; sticks at the ends. */
export function stepZoom(current: number, direction: 1 | -1): number {
  const cur = clampZoom(current);
  if (direction > 0) {
    const next = ZOOM_LEVELS.find((l) => l > cur + 1e-9);
    return next ?? ZOOM_MAX;
  }
  const prev = [...ZOOM_LEVELS].reverse().find((l) => l < cur - 1e-9);
  return prev ?? ZOOM_MIN;
}

/** Terminal font size for a zoom level (whole px, never below 8). */
export function terminalFontSizeFor(zoom: number): number {
  return Math.max(8, Math.round(TERMINAL_BASE_FONT_SIZE * clampZoom(zoom)));
}

/** "125%" — for toasts and the View menu. */
export function formatZoom(zoom: number): string {
  return `${Math.round(clampZoom(zoom) * 100)}%`;
}
