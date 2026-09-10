// Builds the highlight.js `.hljs-*` stylesheet from the app's ThemeEngine
// palette — the markdown-fence analogue of cm-theme.ts, so code blocks in the
// markdown viewer and the chat follow the active preset instead of a shipped
// "atom-one-dark" that stayed dark on Solarized Light. Pure (no DOM): theme.ts
// injects the result into a <style id="hljs-theme"> on every theme apply.
//
// Token → colour mapping mirrors cm-theme.ts so a fence and an editor tab
// showing the same code look alike.

import type { ThemeColorKey } from "./theme";

export const HLJS_STYLE_ID = "hljs-theme";

interface Rule {
  selectors: string[];
  color?: ThemeColorKey;
  extra?: string;
}

const RULES: Rule[] = [
  { selectors: [".hljs"], color: "text-primary" },
  { selectors: [".hljs-comment", ".hljs-quote"], color: "text-muted", extra: "font-style: italic;" },
  { selectors: [".hljs-doctag", ".hljs-keyword", ".hljs-formula"], color: "xterm-magenta" },
  {
    selectors: [".hljs-section", ".hljs-name", ".hljs-selector-tag", ".hljs-deletion", ".hljs-subst"],
    color: "xterm-red",
  },
  { selectors: [".hljs-literal", ".hljs-number"], color: "xterm-yellow" },
  {
    selectors: [".hljs-string", ".hljs-regexp", ".hljs-addition", ".hljs-attribute", ".hljs-meta .hljs-string"],
    color: "xterm-green",
  },
  {
    selectors: [
      ".hljs-attr",
      ".hljs-type",
      ".hljs-built_in",
      ".hljs-title.class_",
      ".hljs-class .hljs-title",
      ".hljs-selector-class",
      ".hljs-selector-attr",
      ".hljs-selector-pseudo",
    ],
    color: "xterm-cyan",
  },
  { selectors: [".hljs-variable", ".hljs-template-variable"], color: "text-primary" },
  { selectors: [".hljs-symbol", ".hljs-bullet", ".hljs-link", ".hljs-selector-id", ".hljs-title"], color: "xterm-blue" },
  { selectors: [".hljs-meta"], color: "text-muted" },
  { selectors: [".hljs-emphasis"], extra: "font-style: italic;" },
  { selectors: [".hljs-strong"], extra: "font-weight: bold;" },
  { selectors: [".hljs-link"], extra: "text-decoration: underline;" },
];

/** Every palette key the stylesheet reads — handy for tests and audits. */
export const HLJS_PALETTE_KEYS: ThemeColorKey[] = Array.from(
  new Set(RULES.flatMap((r) => (r.color ? [r.color] : []))),
);

/** CSS text for the `.hljs-*` classes, colours taken from `c(key)`. */
export function buildHljsCss(c: (k: ThemeColorKey) => string): string {
  return RULES.map((r) => {
    const decls: string[] = [];
    if (r.color) decls.push(`color: ${c(r.color)};`);
    if (r.extra) decls.push(r.extra);
    return `${r.selectors.join(",\n")} {\n  ${decls.join("\n  ")}\n}`;
  }).join("\n");
}
