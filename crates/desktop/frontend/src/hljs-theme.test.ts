import { describe, expect, it } from "vitest";
import { HLJS_PALETTE_KEYS, buildHljsCss } from "./hljs-theme";

const PALETTE: Record<string, string> = {
  "text-primary": "#3b4f56",
  "text-muted": "#6e7e7e",
  "xterm-magenta": "#d33682",
  "xterm-red": "#dc322f",
  "xterm-yellow": "#b58900",
  "xterm-green": "#859900",
  "xterm-cyan": "#2aa198",
  "xterm-blue": "#268bd2",
};

describe("buildHljsCss", () => {
  const css = buildHljsCss((k) => PALETTE[k] ?? `MISSING(${k})`);

  it("only reads keys the palette provides", () => {
    for (const k of HLJS_PALETTE_KEYS) expect(PALETTE[k], k).toBeDefined();
    expect(css).not.toContain("MISSING(");
  });

  it("covers every token class the shipped atom-one-dark sheet styled", () => {
    for (const cls of [
      ".hljs-comment", ".hljs-keyword", ".hljs-section", ".hljs-literal", ".hljs-string",
      ".hljs-attr", ".hljs-number", ".hljs-symbol", ".hljs-built_in", ".hljs-title.class_",
      ".hljs-emphasis", ".hljs-strong", ".hljs-link", ".hljs-meta .hljs-string",
    ]) {
      expect(css, cls).toContain(cls);
    }
  });

  it("contains no colour literal that is not from the palette", () => {
    const hexes = css.match(/#[0-9a-fA-F]{3,8}\b/g) ?? [];
    const allowed = new Set(Object.values(PALETTE));
    for (const h of hexes) expect(allowed.has(h), h).toBe(true);
    expect(css).not.toMatch(/rgba?\(/);
  });

  it("gives comments the muted colour, italic", () => {
    expect(css).toMatch(/\.hljs-comment,\n\.hljs-quote \{\n {2}color: #6e7e7e;\n {2}font-style: italic;/);
  });
});
