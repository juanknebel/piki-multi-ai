// Builds a CodeMirror editor theme + syntax HighlightStyle from the app's
// ThemeEngine palette, so the editor follows the global theme (the analogue
// of ThemeEngine.buildXtermTheme for the terminal). ThemeColors has no
// syntax-specific keys, so token colors are derived from the ANSI / accent /
// text / git palette — cohesive with any preset or custom theme.

import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";
import type { ThemeColorKey } from "./theme";

function rgba(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const v =
    h.length === 3
      ? h
          .split("")
          .map((x) => x + x)
          .join("")
      : h;
  const n = parseInt(v.slice(0, 6), 16);
  if (Number.isNaN(n)) return hex;
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
}

export function buildCmTheme(
  c: (k: ThemeColorKey) => string,
  dark: boolean,
): Extension {
  const bg = c("bg-primary");
  const fg = c("text-primary");
  const muted = c("text-muted");
  const accent = c("accent-primary");
  const sel = rgba(accent, 0.22);

  const view = EditorView.theme(
    {
      "&": { color: fg, backgroundColor: bg, height: "100%" },
      ".cm-content": { caretColor: accent },
      ".cm-cursor, .cm-dropCursor": { borderLeftColor: accent },
      "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
        { backgroundColor: sel },
      ".cm-selectionMatch": { backgroundColor: rgba(accent, 0.14) },
      ".cm-activeLine": { backgroundColor: rgba(fg, 0.04) },
      ".cm-gutters": {
        backgroundColor: bg,
        color: muted,
        border: "none",
      },
      ".cm-activeLineGutter": {
        backgroundColor: rgba(fg, 0.04),
        color: c("text-secondary"),
      },
      ".cm-foldPlaceholder": {
        backgroundColor: "transparent",
        border: "none",
        color: muted,
      },
      ".cm-scroller": { overflow: "auto" },
      ".cm-tooltip": {
        backgroundColor: c("bg-dropdown"),
        border: `1px solid ${c("border-primary")}`,
        color: fg,
      },
      ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
        backgroundColor: c("bg-active"),
        color: c("text-bright"),
      },
      ".cm-panels": { backgroundColor: c("bg-secondary"), color: fg },
      ".cm-searchMatch": { backgroundColor: rgba(c("xterm-yellow"), 0.3) },
      ".cm-searchMatch.cm-searchMatch-selected": {
        backgroundColor: rgba(accent, 0.4),
      },
      ".cm-matchingBracket, .cm-nonmatchingBracket": {
        backgroundColor: rgba(accent, 0.2),
        outline: `1px solid ${rgba(accent, 0.5)}`,
      },
    },
    { dark },
  );

  const hl = HighlightStyle.define(
    [
      { tag: t.comment, color: muted, fontStyle: "italic" },
      {
        tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword],
        color: c("xterm-magenta"),
      },
      {
        tag: [t.string, t.special(t.string), t.regexp],
        color: c("xterm-green"),
      },
      { tag: [t.number, t.bool, t.null, t.atom], color: c("xterm-yellow") },
      {
        tag: [t.function(t.variableName), t.function(t.propertyName)],
        color: c("xterm-blue"),
      },
      {
        tag: [t.typeName, t.className, t.namespace, t.definition(t.typeName)],
        color: c("xterm-cyan"),
      },
      { tag: [t.propertyName, t.attributeName], color: c("xterm-cyan") },
      { tag: [t.variableName, t.labelName], color: fg },
      {
        tag: [t.definition(t.variableName), t.macroName],
        color: c("text-bright"),
      },
      { tag: [t.operator, t.punctuation, t.separator], color: c("text-secondary") },
      { tag: [t.tagName, t.angleBracket], color: c("xterm-red") },
      { tag: [t.meta, t.documentMeta, t.annotation], color: muted },
      { tag: [t.attributeValue], color: c("xterm-green") },
      { tag: t.invalid, color: c("git-deleted") },
      { tag: [t.link, t.url], color: c("xterm-cyan"), textDecoration: "underline" },
      { tag: t.heading, color: c("xterm-blue"), fontWeight: "bold" },
      { tag: t.strong, fontWeight: "bold" },
      { tag: t.emphasis, fontStyle: "italic" },
      { tag: t.strikethrough, textDecoration: "line-through" },
      { tag: [t.processingInstruction, t.inserted], color: c("xterm-green") },
      { tag: t.deleted, color: c("git-deleted") },
    ],
    { themeType: dark ? "dark" : "light" },
  );

  return [view, syntaxHighlighting(hl)];
}
