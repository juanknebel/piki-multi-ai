import { describe, expect, it } from "vitest";
import { findConflicts, groupByCategory, matchesShortcutQuery, normalizeCombo } from "./shortcut-table";

const defs = [
  { id: "palette", label: "Command Palette", category: "General", key: "Ctrl+P" },
  { id: "new-ws", label: "New Workspace", category: "General", key: "Ctrl+N" },
  { id: "sidebar", label: "Toggle Sidebar", category: "View & Panels", key: "Ctrl+B" },
  { id: "log", label: "Git Log", category: "Git", key: "Alt+L" },
];

describe("findConflicts", () => {
  it("is empty when every key is unique", () => {
    expect(findConflicts(defs).size).toBe(0);
  });

  it("flags BOTH rows that share a combo, naming the other action", () => {
    const dup = [...defs, { id: "dash", label: "Dashboard", category: "General", key: "ctrl+p" }];
    const c = findConflicts(dup);
    expect(c.get("palette")).toEqual(["Dashboard"]);
    expect(c.get("dash")).toEqual(["Command Palette"]);
    expect(c.has("new-ws")).toBe(false);
  });

  it("lists every other action when three collide", () => {
    const tri = [
      ...defs,
      { id: "a", label: "A", category: "Git", key: "Alt+L" },
      { id: "b", label: "B", category: "Git", key: "Alt+L" },
    ];
    const c = findConflicts(tri);
    expect(c.get("log")).toEqual(["A", "B"]);
    expect(c.get("a")).toEqual(["Git Log", "B"]);
  });

  it("flags a shortcut sitting on a reserved widget key", () => {
    const c = findConflicts(defs, [{ key: "Ctrl+B", label: "Bold (editor)" }]);
    expect(c.get("sidebar")).toEqual(["Bold (editor)"]);
    expect(c.size).toBe(1);
  });

  it("ignores empty keys", () => {
    const c = findConflicts([...defs, { id: "x", label: "X", category: "Git", key: "" }, { id: "y", label: "Y", category: "Git", key: " " }]);
    expect(c.size).toBe(0);
  });
});

describe("matchesShortcutQuery", () => {
  const def = defs[0];
  it("empty query matches everything", () => {
    expect(matchesShortcutQuery(def, "")).toBe(true);
    expect(matchesShortcutQuery(def, "   ")).toBe(true);
  });
  it("matches label, key, formatted key and category, case-insensitively", () => {
    expect(matchesShortcutQuery(def, "palette")).toBe(true);
    expect(matchesShortcutQuery(def, "CTRL+P")).toBe(true);
    expect(matchesShortcutQuery(def, "⌘+P", "⌘+P")).toBe(true);
    expect(matchesShortcutQuery(def, "general")).toBe(true);
    expect(matchesShortcutQuery(def, "git")).toBe(false);
  });
  it("every word must match", () => {
    expect(matchesShortcutQuery(def, "general palette")).toBe(true);
    expect(matchesShortcutQuery(def, "general sidebar")).toBe(false);
  });
});

describe("groupByCategory", () => {
  it("keeps the given order, drops empty groups, appends unknown categories", () => {
    const order = ["Git", "General", "Search", "View & Panels"];
    const groups = groupByCategory([...defs, { id: "odd", label: "Odd", category: "Misc", key: "F9" }], order);
    expect(groups.map((g) => g.category)).toEqual(["Git", "General", "View & Panels", "Misc"]);
    expect(groups[1].items.map((d) => d.id)).toEqual(["palette", "new-ws"]);
  });
});

describe("normalizeCombo", () => {
  it("lower-cases and trims", () => {
    expect(normalizeCombo(" Ctrl+Shift+A ")).toBe("ctrl+shift+a");
  });
});
