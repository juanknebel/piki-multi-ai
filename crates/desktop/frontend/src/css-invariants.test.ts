// Stylesheet invariants (crates/desktop/CLAUDE.md "CSS tokens" / phase 13):
// read every sheet in styles/ as text — no DOM — and fail the build
// when a future change reintroduces what the token layer and the focus-ring
// work removed.
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";

const dir = decodeURIComponent(new URL("./styles/", import.meta.url).pathname);
const sheets = readdirSync(dir)
  .filter((f) => f.endsWith(".css"))
  .sort();

function read(name: string): string {
  return readFileSync(dir + name, "utf8");
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Flat `{ selector, body }` list. Nested at-rules keep the at-rule text in
 *  the selector, which is fine for "does the selector mention X" checks. */
function blocks(css: string): { selector: string; body: string }[] {
  const out: { selector: string; body: string }[] = [];
  for (const chunk of stripComments(css).split("}")) {
    const open = chunk.lastIndexOf("{");
    if (open < 0) continue;
    out.push({ selector: chunk.slice(0, open).trim(), body: chunk.slice(open + 1) });
  }
  return out;
}

describe("styles/ invariants", () => {
  it("finds the stylesheets", () => {
    expect(sheets).toContain("variables.css");
    expect(sheets).toContain("primitives.css");
    expect(sheets.length).toBeGreaterThan(20);
  });

  it("index.css imports every sheet exactly once, primitives right after reset", () => {
    const index = read("index.css");
    const imports = [...index.matchAll(/@import "\.\/([^"]+)";/g)].map((m) => m[1]);
    for (const name of sheets) {
      if (name === "index.css") continue;
      expect(imports.filter((i) => i === name), `${name} imported once`).toHaveLength(1);
    }
    for (const name of imports) expect(sheets, `${name} exists`).toContain(name);
    expect(imports.indexOf("primitives.css")).toBe(imports.indexOf("reset.css") + 1);
  });

  it("only removes the outline inside a :focus-visible rule that paints the replacement", () => {
    for (const name of sheets) {
      for (const { selector, body } of blocks(read(name))) {
        if (!/outline\s*:\s*none/.test(body)) continue;
        expect(selector, `${name}: "${selector}" sets outline: none`).toContain(":focus-visible");
        expect(
          /(box-shadow|border-color|border-bottom-color|background)\s*:/.test(body),
          `${name}: "${selector}" removes the outline without a visible replacement`,
        ).toBe(true);
      }
    }
  });

  it("uses a --z-* token for every z-index", () => {
    for (const name of sheets) {
      for (const m of stripComments(read(name)).matchAll(/z-index\s*:\s*([^;]+);/g)) {
        expect(m[1].trim(), `${name}: z-index ${m[1]}`).toMatch(/^(calc\()?var\(--z-/);
      }
    }
  });

  it("keeps every colour literal in variables.css", () => {
    const literal = /rgba?\(|#[0-9a-fA-F]{3,8}\b/;
    for (const name of sheets) {
      if (name === "variables.css") continue;
      const hit = read(name)
        .split("\n")
        .find((line) => literal.test(line));
      expect(hit, `${name}: ${hit}`).toBeUndefined();
    }
  });

  it("never transitions `all`", () => {
    for (const name of sheets) {
      expect(stripComments(read(name)), name).not.toMatch(/transition\s*:\s*all\b/);
    }
  });

  it("keeps the font name and the control heights in variables.css only", () => {
    for (const name of sheets) {
      if (name === "variables.css") continue;
      expect(read(name), name).not.toContain("JetBrainsMono");
    }
    const vars = read("variables.css");
    for (const token of ["--control-height", "--control-height-sm", "--header-height", "--focus-ring"]) {
      expect(vars, token).toContain(`${token}:`);
    }
  });
});
