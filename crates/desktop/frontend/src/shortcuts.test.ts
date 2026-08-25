import { describe, expect, it } from "vitest";
import {
  CATEGORY_ORDER,
  getShortcuts,
  helpSections,
  isTerminalSafeCombo,
  parseCombo,
} from "./shortcuts";

describe("parseCombo", () => {
  it("splits modifiers from the key", () => {
    expect(parseCombo("Ctrl+Shift+F")).toEqual({ ctrl: true, shift: true, alt: false, key: "F" });
    expect(parseCombo("Alt+1")).toEqual({ ctrl: false, shift: false, alt: true, key: "1" });
    expect(parseCombo("?")).toEqual({ ctrl: false, shift: false, alt: false, key: "?" });
    expect(parseCombo("Ctrl+Space")).toEqual({ ctrl: true, shift: false, alt: false, key: "Space" });
  });

  it("treats ⌘ (recorded on macOS) as Ctrl so combos stay portable", () => {
    expect(parseCombo("⌘+P").ctrl).toBe(true);
  });
});

describe("isTerminalSafeCombo", () => {
  it("rejects every chord xterm.js turns into a control byte", () => {
    for (const combo of ["Ctrl+B", "Ctrl+P", "Ctrl+N", "Ctrl+F", "Ctrl+M", "Ctrl+Space", "Ctrl+\\", "?", "Ctrl+Z"]) {
      expect(isTerminalSafeCombo(combo), combo).toBe(false);
    }
  });

  it("accepts Alt-, Ctrl+Shift- and Ctrl+Alt- chords", () => {
    for (const combo of ["Alt+D", "Alt+Shift+W", "Ctrl+Shift+F", "Ctrl+Alt+X", "Alt+1"]) {
      expect(isTerminalSafeCombo(combo), combo).toBe(true);
    }
  });
});

describe("shortcut registry", () => {
  it("never lets a plain Ctrl+letter / Ctrl+Space default capture inside the terminal", () => {
    // The type-level guard (`TerminalSafeCombo`) already fails `tsc` for
    // this; the runtime check keeps it honest if the types are ever loosened.
    for (const def of getShortcuts()) {
      if (def.terminalCapture) {
        expect(isTerminalSafeCombo(def.defaultKey), `${def.id} (${def.defaultKey})`).toBe(true);
      }
    }
  });

  it("has unique ids and unique default keys", () => {
    const ids = getShortcuts().map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
    const keys = getShortcuts().map((d) => d.defaultKey);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("builds help sections in CATEGORY_ORDER with no unknown category", () => {
    const sections = helpSections();
    const order = sections.map((s) => s.category);
    expect(order).toEqual(CATEGORY_ORDER.filter((c) => order.includes(c)));
    for (const s of sections) expect(s.items.length).toBeGreaterThan(0);
  });

  it("lists copy/paste as Ctrl+Shift+C/V off macOS (plain Ctrl+C is SIGINT)", () => {
    const terminal = helpSections().find((s) => s.category === "Terminal")!;
    const keys = terminal.items.map((i) => i.key);
    expect(keys).toContain("Ctrl+Shift+C");
    expect(keys).toContain("Ctrl+Shift+V");
    expect(keys).not.toContain("Ctrl+C");
  });

  it("marks outside-only rows and only those", () => {
    const all = helpSections().flatMap((s) => s.items);
    const byLabel = Object.fromEntries(all.map((i) => [i.label, i.outsideOnly]));
    expect(byLabel["Command Palette"]).toBe(true);
    expect(byLabel["Toggle Sidebar"]).toBe(true);
    expect(byLabel["Dashboard"]).toBe(false);
    expect(byLabel["Search in Project"]).toBe(false);
    expect(byLabel["Copy Selection"]).toBe(false);
  });
});
