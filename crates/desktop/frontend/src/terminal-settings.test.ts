import { describe, expect, it } from "vitest";
import {
  DEFAULT_TERMINAL_SETTINGS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  SCROLLBACK_MAX,
  effectiveTerminalFontSize,
  formatScrollback,
  normalizeTerminalSettings,
  terminalSettingsDiff,
  xtermOptionsFor,
} from "./terminal-settings";
import { terminalFontSizeFor } from "./zoom";

describe("normalizeTerminalSettings", () => {
  it("junk or missing → defaults", () => {
    expect(normalizeTerminalSettings(undefined)).toEqual(DEFAULT_TERMINAL_SETTINGS);
    expect(normalizeTerminalSettings("nope")).toEqual(DEFAULT_TERMINAL_SETTINGS);
    expect(normalizeTerminalSettings([1, 2])).toEqual(DEFAULT_TERMINAL_SETTINGS);
    expect(normalizeTerminalSettings({ fontSize: "big", cursorStyle: "blob", scrollback: -3 })).toEqual(
      DEFAULT_TERMINAL_SETTINGS,
    );
  });

  it("keeps valid values and clamps the ranges", () => {
    const s = normalizeTerminalSettings({
      fontFamily: "  Fira Code, monospace ",
      fontSize: 13.4,
      lineHeight: 1.4,
      scrollback: 50000,
      cursorStyle: "bar",
      cursorBlink: false,
      copyOnSelect: false,
    });
    expect(s).toEqual({
      fontFamily: "Fira Code, monospace",
      fontSize: 13,
      lineHeight: 1.4,
      scrollback: 50000,
      cursorStyle: "bar",
      cursorBlink: false,
      copyOnSelect: false,
    });
    expect(normalizeTerminalSettings({ fontSize: 2 }).fontSize).toBe(FONT_SIZE_MIN);
    expect(normalizeTerminalSettings({ fontSize: 200 }).fontSize).toBe(FONT_SIZE_MAX);
    expect(normalizeTerminalSettings({ scrollback: 10_000_000 }).scrollback).toBe(SCROLLBACK_MAX);
    expect(normalizeTerminalSettings({ lineHeight: 5 }).lineHeight).toBe(DEFAULT_TERMINAL_SETTINGS.lineHeight);
    // numeric strings from an older hand-edited document are accepted
    expect(normalizeTerminalSettings({ fontSize: "16" }).fontSize).toBe(16);
  });
});

describe("terminalSettingsDiff", () => {
  it("is empty for the defaults and lists only what changed", () => {
    expect(terminalSettingsDiff({ ...DEFAULT_TERMINAL_SETTINGS })).toEqual({});
    expect(terminalSettingsDiff({ ...DEFAULT_TERMINAL_SETTINGS, scrollback: 50000, cursorBlink: false })).toEqual({
      scrollback: 50000,
      cursorBlink: false,
    });
  });
});

describe("zoom composition", () => {
  it("effective size = setting × zoom, and the default base matches phase 14", () => {
    const s = normalizeTerminalSettings({ fontSize: 16 });
    expect(effectiveTerminalFontSize(s, 1)).toBe(16);
    expect(effectiveTerminalFontSize(s, 1.5)).toBe(24);
    expect(effectiveTerminalFontSize(s, 0.7)).toBe(11);
    expect(effectiveTerminalFontSize(DEFAULT_TERMINAL_SETTINGS, 1.25)).toBe(terminalFontSizeFor(1.25));
  });

  it("xtermOptionsFor falls back to the theme font when the family is empty", () => {
    const opts = xtermOptionsFor(DEFAULT_TERMINAL_SETTINGS, 1, "Theme Mono");
    expect(opts.fontFamily).toBe("Theme Mono");
    expect(opts.fontSize).toBe(DEFAULT_TERMINAL_SETTINGS.fontSize);
    expect(opts.scrollback).toBe(5000);
    const custom = xtermOptionsFor(normalizeTerminalSettings({ fontFamily: "Iosevka" }), 2, "Theme Mono");
    expect(custom.fontFamily).toBe("Iosevka");
    expect(custom.fontSize).toBe(28);
  });
});

describe("formatScrollback", () => {
  it("rounds thousands", () => {
    expect(formatScrollback(50000)).toBe("50k lines");
    expect(formatScrollback(1000)).toBe("1k lines");
    expect(formatScrollback(1500)).toBe("1500 lines");
  });
});
