// Keeps the desktop shortcut table in docs/technical.md honest — the mirror
// of the TUI's `docs_parity` test over its prefix table. The table between the
// BEGIN/END markers must list every rebindable shortcut's *default* key and
// label and every fixed (non-rebindable) row, and it may not list a
// `Ctrl+…` / `Alt+…` key that nothing in the registry defines (that is how
// `Alt+S` survived in the docs after Settings moved to `Ctrl+,`).
//
// The check is on keys and labels, not on the prose after them: a row may
// add an explanation after the label.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { getFixedShortcuts, getReservedCombos, getShortcuts } from "./shortcuts";

const DOC = decodeURIComponent(new URL("../../../../docs/technical.md", import.meta.url).pathname);
const BEGIN = "<!-- BEGIN:desktop-shortcuts -->";
const END = "<!-- END:desktop-shortcuts -->";

function table(): string {
  const text = readFileSync(DOC, "utf8");
  const a = text.indexOf(BEGIN);
  const b = text.indexOf(END);
  if (a < 0 || b < 0 || b < a) throw new Error(`docs/technical.md: missing ${BEGIN} … ${END} markers`);
  return text.slice(a + BEGIN.length, b);
}

/** Every inline-code span in the table. Handles the `` `` `x` `` ``
 *  double-backtick form a backtick key needs (`` ``Ctrl+Shift+` `` ``), as
 *  well as plain `` `x` `` — a port of the TUI's `code_spans`. */
function codeTokens(md: string): string[] {
  const chars = [...md];
  const spans: string[] = [];
  let i = 0;
  while (i < chars.length) {
    if (chars[i] !== "`") {
      i++;
      continue;
    }
    const fence = chars[i + 1] === "`" ? 2 : 1;
    const open = i + fence;
    let j = open;
    let close = -1;
    while (j < chars.length) {
      let run = 0;
      while (chars[j + run] === "`") run++;
      if (run === fence) {
        close = j;
        break;
      }
      j += run > 0 ? run : 1;
    }
    if (close < 0) break;
    spans.push(chars.slice(open, close).join("").trim());
    i = close + fence;
  }
  return spans;
}

describe("docs/technical.md desktop shortcut table", () => {
  const md = table();
  const tokens = new Set(codeTokens(md));

  it("lists every rebindable shortcut's default key and label", () => {
    const missing: string[] = [];
    for (const def of getShortcuts()) {
      if (!tokens.has(def.defaultKey)) missing.push(`key \`${def.defaultKey}\` (${def.id})`);
      if (!md.includes(def.label)) missing.push(`label "${def.label}" (${def.id})`);
    }
    expect(missing, `add these rows to the desktop shortcut table:\n${missing.join("\n")}`).toEqual([]);
  });

  it("lists every fixed (non-rebindable) key and label", () => {
    const missing: string[] = [];
    for (const f of getFixedShortcuts()) {
      if (!tokens.has(f.key)) missing.push(`key \`${f.key}\``);
      if (!md.includes(f.label)) missing.push(`label "${f.label}"`);
    }
    expect(missing, `add these fixed rows to the desktop shortcut table:\n${missing.join("\n")}`).toEqual([]);
  });

  it("does not list a Ctrl+… / Alt+… key the registry no longer defines", () => {
    const known = new Set<string>([
      ...getShortcuts().map((d) => d.defaultKey),
      ...getFixedShortcuts().map((f) => f.key),
      ...getReservedCombos().map((r) => r.key),
    ]);
    const stale = [...tokens].filter((t) => /^(Ctrl|Alt)\+/.test(t) && !known.has(t));
    expect(stale, `these keys are in the docs but nothing defines them:\n${stale.join("\n")}`).toEqual([]);
  });
});
