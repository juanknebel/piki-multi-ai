import { describe, expect, it } from "vitest";
import {
  CONTEXT_MAX_LINES,
  TRUNCATED_MARKER,
  appendToDraft,
  contextChoices,
  contextHeader,
  diffLinesToText,
  fenceBlock,
  fenceFor,
  fenceLang,
  formatDurationMs,
  parseToolMessage,
  prettyJson,
  truncateLines,
} from "./chat-context";

describe("contextHeader", () => {
  it("formats every kind", () => {
    expect(contextHeader("terminal", { name: "Shell" })).toBe('Terminal selection (tab "Shell")');
    expect(contextHeader("file", { name: "src/a.rs" })).toBe("File: src/a.rs");
    expect(contextHeader("file", { name: "src/a.rs", lines: { from: 3, to: 9 } })).toBe("File: src/a.rs (lines 3–9)");
    expect(contextHeader("diff", { name: "src/a.rs" })).toBe("Diff: src/a.rs");
    expect(contextHeader("editor-selection", { name: "a.ts", lines: { from: 1, to: 1 } })).toBe(
      "Selected text in editor: a.ts (lines 1–1)",
    );
  });
});

describe("fenceBlock", () => {
  it("wraps text in a fence with a header and language hint", () => {
    expect(fenceBlock("file", { name: "src/main.rs" }, "fn main() {}\n")).toBe(
      "File: src/main.rs\n```rs\nfn main() {}\n```\n",
    );
  });

  it("a terminal selection is a text fence", () => {
    const block = fenceBlock("terminal", { name: "Shell" }, "panic at foo.rs:12");
    expect(block.startsWith('Terminal selection (tab "Shell")\n```text\n')).toBe(true);
  });

  it("returns an empty string for whitespace-only input", () => {
    expect(fenceBlock("terminal", { name: "Shell" }, "  \n\n")).toBe("");
  });

  it("grows the fence past backtick runs in the body", () => {
    expect(fenceFor("a ``` b")).toBe("````");
    expect(fenceFor("plain")).toBe("```");
    const block = fenceBlock("file", { name: "README.md" }, "```sh\nls\n```");
    expect(block).toContain("\n````md\n");
    expect(block.endsWith("\n````\n")).toBe(true);
  });

  it("truncates long bodies at the cap with a marker", () => {
    const lines = Array.from({ length: CONTEXT_MAX_LINES + 50 }, (_, i) => `line ${i}`);
    const block = fenceBlock("file", { name: "big.txt" }, lines.join("\n"));
    expect(block).toContain(TRUNCATED_MARKER);
    expect(block).toContain(`line ${CONTEXT_MAX_LINES - 1}`);
    expect(block).not.toContain(`line ${CONTEXT_MAX_LINES}\n`);
  });
});

describe("truncateLines", () => {
  it("leaves short text alone", () => {
    expect(truncateLines("a\nb", 5)).toEqual({ text: "a\nb", truncated: false });
  });
  it("cuts at max lines", () => {
    expect(truncateLines("a\nb\nc", 2)).toEqual({ text: `a\nb\n${TRUNCATED_MARKER}`, truncated: true });
  });
});

describe("fenceLang", () => {
  it("derives the hint from the extension", () => {
    expect(fenceLang("file", "x/y.ts")).toBe("ts");
    expect(fenceLang("file", "Makefile")).toBe("");
    expect(fenceLang("diff", "a.rs")).toBe("diff");
    expect(fenceLang("terminal", "Shell")).toBe("text");
  });
});

describe("appendToDraft", () => {
  it("separates blocks with a blank line", () => {
    expect(appendToDraft("", "B\n")).toBe("B\n");
    expect(appendToDraft("why?\n", "B\n")).toBe("why?\n\nB\n");
    expect(appendToDraft("keep", "")).toBe("keep");
  });
});

describe("diffLinesToText", () => {
  it("re-prefixes add/del/context and keeps headers", () => {
    expect(
      diffLinesToText([
        { content: "@@ -1 +1 @@", line_type: "hunk" },
        { content: "a", line_type: "context" },
        { content: "b", line_type: "del" },
        { content: "c", line_type: "add" },
      ]),
    ).toBe("@@ -1 +1 @@\n a\n-b\n+c");
  });
});

describe("parseToolMessage", () => {
  it("splits the stored `[name] [Error] text` shape", () => {
    expect(parseToolMessage("[read_file] hello")).toEqual({ name: "read_file", result: "hello", isError: false });
    expect(parseToolMessage("[shell] [Error] boom")).toEqual({ name: "shell", result: "boom", isError: true });
    expect(parseToolMessage("free text")).toEqual({ name: "tool", result: "free text", isError: false });
  });
});

describe("prettyJson / formatDurationMs / contextChoices", () => {
  it("pretty-prints JSON strings and objects, passes junk through", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson({ a: 1 })).toBe('{\n  "a": 1\n}');
    expect(prettyJson("not json")).toBe("not json");
  });
  it("formats durations", () => {
    expect(formatDurationMs(12)).toBe("12 ms");
    expect(formatDurationMs(1500)).toBe("1.5 s");
    expect(formatDurationMs(61_000)).toBe("1 m 1 s");
  });
  it("disables chooser rows that have nothing behind them", () => {
    const rows = contextChoices({ terminalSelection: false, activeFile: true, editorSelection: false });
    expect(rows.map((r) => r.disabled)).toEqual([true, false, false, true]);
  });
});
