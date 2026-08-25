// Settings ▸ Terminal — the shape of the user's terminal preferences, their
// defaults, the normalisation of whatever the settings document holds, and
// the xterm option set they produce for a UI-zoom level. Pure except for the
// small store glue at the bottom (same arrangement as shortcuts.ts).
//
// Persistence: ONE object under the `terminal` key of the settings document,
// holding only the values that differ from the defaults (the key is deleted
// when everything is default), so a future default change reaches users who
// never touched that field.
//
// Zoom composes with the font size: `fontSize` is the base at zoom 1 and the
// effective xterm size is `terminalFontSizeFor(zoom, fontSize)` — the same
// function phase 14's `applyTerminalZoom` uses, so Ctrl+= keeps scaling a
// terminal whose base size the user changed.

import { settingsStore } from "./settings";
import { TERMINAL_BASE_FONT_SIZE, terminalFontSizeFor } from "./zoom";

export type CursorStyle = "block" | "underline" | "bar";

export interface TerminalSettings {
  /** CSS font-family list; empty = the theme's `--font-mono`. */
  fontFamily: string;
  /** Base size in px at zoom 1; UI zoom multiplies it. */
  fontSize: number;
  lineHeight: number;
  scrollback: number;
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
  /** Selecting text with the mouse writes it to the clipboard (one write
   *  per gesture — see copy-on-select.ts). */
  copyOnSelect: boolean;
}

export const DEFAULT_TERMINAL_SETTINGS: Readonly<TerminalSettings> = Object.freeze({
  fontFamily: "",
  fontSize: TERMINAL_BASE_FONT_SIZE,
  lineHeight: 1.25,
  scrollback: 5000,
  cursorStyle: "block",
  cursorBlink: true,
  copyOnSelect: true,
});

export const FONT_SIZE_MIN = 8;
export const FONT_SIZE_MAX = 40;
export const LINE_HEIGHT_CHOICES: readonly number[] = [1, 1.1, 1.2, 1.25, 1.3, 1.4, 1.5];
export const SCROLLBACK_CHOICES: readonly number[] = [1000, 5000, 10000, 20000, 50000, 100000];
export const SCROLLBACK_MAX = 200000;
export const CURSOR_STYLES: readonly CursorStyle[] = ["block", "underline", "bar"];

const SETTINGS_KEY = "terminal";

function num(v: unknown): number | undefined {
  const n = typeof v === "number" ? v : typeof v === "string" && v.trim() !== "" ? Number(v) : NaN;
  return Number.isFinite(n) ? n : undefined;
}

/** Whatever the settings document holds → a complete, sane settings object.
 *  Junk, missing or out-of-range fields fall back to the default. */
export function normalizeTerminalSettings(raw: unknown): TerminalSettings {
  const d = DEFAULT_TERMINAL_SETTINGS;
  const r = (raw && typeof raw === "object" && !Array.isArray(raw) ? raw : {}) as Record<string, unknown>;

  const fontFamily = typeof r.fontFamily === "string" ? r.fontFamily.trim().slice(0, 200) : d.fontFamily;

  const fs = num(r.fontSize);
  const fontSize = fs === undefined ? d.fontSize : Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, Math.round(fs)));

  const lh = num(r.lineHeight);
  const lineHeight = lh === undefined || lh < 1 || lh > 2 ? d.lineHeight : Math.round(lh * 100) / 100;

  const sb = num(r.scrollback);
  const scrollback = sb === undefined || sb < 0 ? d.scrollback : Math.min(SCROLLBACK_MAX, Math.round(sb));

  const cursorStyle = CURSOR_STYLES.includes(r.cursorStyle as CursorStyle) ? (r.cursorStyle as CursorStyle) : d.cursorStyle;
  const cursorBlink = typeof r.cursorBlink === "boolean" ? r.cursorBlink : d.cursorBlink;
  const copyOnSelect = typeof r.copyOnSelect === "boolean" ? r.copyOnSelect : d.copyOnSelect;

  return { fontFamily, fontSize, lineHeight, scrollback, cursorStyle, cursorBlink, copyOnSelect };
}

/** The fields of `s` that differ from the defaults — what gets persisted. */
export function terminalSettingsDiff(s: TerminalSettings): Partial<TerminalSettings> {
  const out: Partial<TerminalSettings> = {};
  for (const key of Object.keys(DEFAULT_TERMINAL_SETTINGS) as (keyof TerminalSettings)[]) {
    if (s[key] !== DEFAULT_TERMINAL_SETTINGS[key]) (out as Record<string, unknown>)[key] = s[key];
  }
  return out;
}

/** Effective xterm font size: the base from the settings × the UI zoom. */
export function effectiveTerminalFontSize(s: TerminalSettings, zoom: number): number {
  return terminalFontSizeFor(zoom, s.fontSize);
}

/** The xterm `ITerminalOptions` subset the settings control. `fallbackFont`
 *  is the theme's `--font-mono` (read via `cssToken` by the caller). */
export interface TerminalXtermOptions {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  scrollback: number;
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
}

export function xtermOptionsFor(s: TerminalSettings, zoom: number, fallbackFont: string): TerminalXtermOptions {
  return {
    fontFamily: s.fontFamily || fallbackFont,
    fontSize: effectiveTerminalFontSize(s, zoom),
    lineHeight: s.lineHeight,
    scrollback: s.scrollback,
    cursorStyle: s.cursorStyle,
    cursorBlink: s.cursorBlink,
  };
}

/** "50k" / "1k" / "500" — for the scrollback dropdown. */
export function formatScrollback(n: number): string {
  if (n >= 1000 && n % 1000 === 0) return `${n / 1000}k lines`;
  return `${n} lines`;
}

// ── Store glue ─────────────────────────────────

/** The current settings (defaults filled in). Sync — the store is loaded. */
export function getTerminalSettings(): TerminalSettings {
  return normalizeTerminalSettings(settingsStore.get(SETTINGS_KEY));
}

/** Merge `patch` in, persist the diff from the defaults, return the result.
 *  The caller applies it to the live terminals (`themeEngine.updateAllTerminals()`). */
export function setTerminalSettings(patch: Partial<TerminalSettings>): TerminalSettings {
  const next = normalizeTerminalSettings({ ...getTerminalSettings(), ...patch });
  const diff = terminalSettingsDiff(next);
  settingsStore.patch(SETTINGS_KEY, Object.keys(diff).length > 0 ? diff : undefined);
  return next;
}

export function resetTerminalSettings(): TerminalSettings {
  settingsStore.patch(SETTINGS_KEY, undefined);
  return { ...DEFAULT_TERMINAL_SETTINGS };
}
