// The icon set and the "no emoji chrome" rule (phase 15).
// `icons.ts` is the one place chrome glyphs come from; this test keeps every
// icon well-formed (16px grid, currentColor only) and scans components/ +
// types.ts so the dingbats it replaced cannot creep back into markup.
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { ICONS, ICON_NAMES, icon } from "./icons";

const here = decodeURIComponent(new URL("./", import.meta.url).pathname);
const srcDir = decodeURIComponent(new URL("../", import.meta.url).pathname);

/** Glyphs that used to be pasted into chrome as text. Arrows (↑ ↓ ← →), the
 *  `×` close glyph, `…`, `·`, `—` and `−` are typography, not icons, and are
 *  deliberately NOT here. */
const REMOVED_GLYPHS = "✓✔⚠📁📂⚙👁✕✖✏✎⟲⟳⋯●○▸▾▷▶↺↻⎇⏳⌖◎⇥⤓◄►＋️";

/** Files (basename) allowed to contain some of those glyphs, and why. */
const ALLOW: Record<string, { glyphs: string; why: string }> = {
  // Nerd Font PUA code points for the file tree — a font, not a dingbat.
  "file-icons.ts": { glyphs: "*", why: "Nerd Font PUA glyph table" },
};

function stripComments(ts: string): string {
  return ts.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:'"`])\/\/[^\n]*/g, "$1");
}

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = dir + name;
    if (statSync(p).isDirectory()) walk(p + "/", out);
    else if (name.endsWith(".ts") && !name.endsWith(".test.ts") && !name.endsWith(".d.ts")) out.push(p);
  }
  return out;
}

describe("icons.ts", () => {
  it("has a non-empty, currentColor-only drawing for every name", () => {
    expect(ICON_NAMES.length).toBeGreaterThan(20);
    for (const name of ICON_NAMES) {
      const body = ICONS[name];
      expect(body.length, name).toBeGreaterThan(10);
      // Only currentColor / none — a baked colour would ignore the theme.
      for (const m of body.matchAll(/(fill|stroke)="([^"]*)"/g)) {
        expect(["currentColor", "none"], `${name}: ${m[0]}`).toContain(m[2]);
      }
      expect(body, name).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(/);
    }
  });

  it("renders a 16px-grid svg that is aria-hidden unless labelled", () => {
    const plain = icon("refresh");
    expect(plain).toMatch(/^<svg class="icon icon-refresh" viewBox="0 0 16 16" aria-hidden="true" focusable="false">/);
    expect(plain.endsWith("</svg>")).toBe(true);
    const labelled = icon("check", { label: 'done "now"', class: "x y", size: "12" });
    expect(labelled).toContain('class="icon icon-check x y"');
    expect(labelled).toContain('width="12" height="12"');
    expect(labelled).toContain('role="img" aria-label="done &quot;now&quot;"');
    expect(labelled).not.toContain("aria-hidden");
  });

  it("keeps every stroke at the shared weight", () => {
    for (const name of ICON_NAMES) {
      for (const m of ICONS[name].matchAll(/stroke-width="([^"]*)"/g)) {
        expect(m[1], name).toBe("1.5");
      }
    }
  });
});

describe("chrome uses icons.ts, not emoji / dingbat glyphs", () => {
  const files = [...walk(here), srcDir + "types.ts"];

  it("scans the component sources", () => {
    expect(files.length).toBeGreaterThan(40);
    expect(files.some((f) => f.endsWith("/types.ts"))).toBe(true);
  });

  for (const file of files) {
    const base = file.slice(file.lastIndexOf("/") + 1);
    it(`${base} has no removed glyph in code`, () => {
      const allow = ALLOW[base];
      if (allow?.glyphs === "*") return;
      const code = stripComments(readFileSync(file, "utf8"));
      const hits = [...new Set([...code].filter((ch) => REMOVED_GLYPHS.includes(ch)))].filter(
        (ch) => !(allow && allow.glyphs.includes(ch)),
      );
      expect(hits, `${base}: use icon("…") from components/icons.ts instead of ${hits.join(" ")}`).toEqual([]);
    });
  }
});
