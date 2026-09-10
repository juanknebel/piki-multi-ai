import { describe, expect, it } from "vitest";
import {
  computeDerived,
  hexToRgba,
  kanbanPalette,
  mixHex,
  onColorFor,
  relativeLuminance,
  rgbToHex,
  themeTone,
} from "./theme-derive";

describe("relativeLuminance / onColorFor", () => {
  it("spans 0..1 from black to white", () => {
    expect(relativeLuminance("#000000")).toBe(0);
    expect(relativeLuminance("#ffffff")).toBeCloseTo(1, 5);
  });

  it("picks black text on a light accent and white on a dark one", () => {
    expect(onColorFor("#39bae6")).toBe("#000"); // Obsidian cyan
    expect(onColorFor("#88c0d0")).toBe("#000"); // Nord frost
    expect(onColorFor("#1a6599")).toBe("#fff"); // Solarized Light accent
    expect(onColorFor("#000000")).toBe("#fff");
    expect(onColorFor("#ffffff")).toBe("#000");
  });
});

describe("colour helpers", () => {
  it("mixes linearly and clamps to 8-bit hex", () => {
    expect(mixHex("#000000", "#ffffff", 0)).toBe("#000000");
    expect(mixHex("#000000", "#ffffff", 1)).toBe("#ffffff");
    expect(mixHex("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(rgbToHex(300, -5, 12.4)).toBe("#ff000c");
  });

  it("formats rgba from hex", () => {
    expect(hexToRgba("#39bae6", 0.25)).toBe("rgba(57, 186, 230, 0.25)");
  });
});

describe("computeDerived", () => {
  const base = {
    "accent-primary": "#1a6599",
    "accent-warm": "#b58900",
    "terminal-cursor": "#1a6599",
    "activity-bar-badge": "#fdf6e3",
    "xterm-red": "#dc322f",
    "xterm-yellow": "#b58900",
    "xterm-blue": "#268bd2",
    "xterm-bright-blue": "#839496",
    "xterm-bright-yellow": "#657b83",
    "xterm-cyan": "#2aa198",
    "xterm-green": "#859900",
    "xterm-magenta": "#d33682",
    "text-secondary": "#546b73",
    "text-muted": "#6e7e7e",
    "sidebar-header-fg": "#546b73",
  };

  it("derives on-accent, badge text and selection from the palette", () => {
    const d = computeDerived(base, false);
    expect(d["on-accent"]).toBe("#fff");
    expect(d["activity-bar-badge-fg"]).toBe("#000");
    expect(d["activity-bar-badge-glow"]).toBe("0 0 10px rgba(253, 246, 227, 0.3)");
    expect(d["selection-bg"]).toBe("rgba(26, 101, 153, 0.25)");
    expect(d["border-subtle"]).toBe("rgba(0, 0, 0, 0.06)");
  });

  it("maps every file-icon token onto a palette colour", () => {
    const d = computeDerived(base, true);
    for (const k of ["rust", "ts", "js", "py", "go", "web", "data", "doc", "asset", "muted", "default", "folder"]) {
      expect(d[`icon-${k}`], k).toMatch(/^#[0-9a-f]{6}$/);
    }
    expect(d["icon-ts"]).toBe("#268bd2");
    expect(d["icon-folder"]).toBe("#6e7e7e");
    expect(d["icon-web"]).toBe(mixHex("#dc322f", "#b58900", 0.5));
  });

  it("falls back to Obsidian defaults when a key is missing", () => {
    const d = computeDerived({}, true);
    expect(d["on-accent"]).toBe("#000");
    expect(d["accent-muted"]).toBe("rgba(57, 186, 230, 0.12)");
    expect(d["icon-rust"]).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe("themeTone", () => {
  it("reads the tone off the main background luminance", () => {
    expect(themeTone({ "bg-primary": "#fdf6e3" })).toBe("light"); // Solarized Light
    expect(themeTone({ "bg-primary": "#0b0f14" })).toBe("dark"); // Obsidian
    expect(themeTone({ "bg-primary": "#2e3440" })).toBe("dark"); // Nord
    expect(themeTone({ "bg-primary": "#1e1e2e" })).toBe("dark"); // Catppuccin Mocha
    expect(themeTone({ "bg-primary": "#1a1b26" })).toBe("dark"); // Tokyo Night
    expect(themeTone({})).toBe("dark");
  });
});

describe("kanbanPalette", () => {
  it("yields sixteen distinct lowercase swatches from the palette", () => {
    const p = kanbanPalette({
      "accent-primary": "#39BAE6",
      "xterm-blue": "#39bae6", // duplicate of the accent — must be skipped
      "xterm-cyan": "#39bae6",
      "accent-warm": "#e6a730",
    });
    expect(p).toHaveLength(16);
    expect(new Set(p).size).toBe(16);
    expect(p[0]).toBe("#39bae6");
    expect(p[1]).toBe("#e6a730");
    for (const hex of p) expect(hex).toMatch(/^#[0-9a-f]{6}$/);
  });

  it("falls back to the Obsidian swatches when the palette is empty", () => {
    expect(kanbanPalette({})[2]).toBe("#7b61ff");
  });
});

describe("computeDerived — phase 14 tokens", () => {
  it("derives on-error, accent-alt and the kanban tokens", () => {
    const d = computeDerived({ "git-deleted": "#f38ba8", "xterm-magenta": "#cba6f7", "accent-primary": "#89b4fa" }, true);
    expect(d["on-error"]).toBe("#000"); // Catppuccin pink is light → black text
    expect(computeDerived({ "git-deleted": "#b82523" }, false)["on-error"]).toBe("#fff");
    expect(d["accent-alt"]).toBe("#cba6f7");
    expect(d["kanban-col-todo"]).toBe("#89b4fa");
    expect(d["kanban-col-in-review"]).toBe("#cba6f7");
    for (let i = 1; i <= 16; i++) expect(d[`kanban-swatch-${i}`], `swatch ${i}`).toMatch(/^#[0-9a-f]{6}$/);
    expect(d["kanban-swatch-17"]).toBeUndefined();
  });
});
