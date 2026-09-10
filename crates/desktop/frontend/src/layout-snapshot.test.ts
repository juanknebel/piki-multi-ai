import { describe, expect, it } from "vitest";
import {
  describeContent,
  missingSavedContents,
  parseSavedLayouts,
  remapContentIds,
  snapshotContents,
  type SavedWsLayout,
} from "./layout-snapshot";
import { deserialize, newLeaf, splitPane, treeContentIds, type PaneNode } from "./pane-tree";
import type { TabInfo } from "./types";

const tabs: TabInfo[] = [
  { id: "claude", provider: { Custom: "claude" }, alive: true },
  { id: "web", provider: "WebPreview", alive: true },
  { id: "ed", provider: "CodeEditor", alive: true, custom_title: "notes" },
  { id: "md", provider: "Markdown", alive: true },
  { id: "kb", provider: "Kanban", alive: true },
  { id: "api", provider: "Api", alive: true },
  { id: "lost", provider: "CodeEditor", alive: true },
];
const pathOf = (id: string) => ({ ed: "src/a.ts", md: "README.md" })[id] ?? null;
const urlOf = (id: string) => (id === "web" ? "http://localhost:3000" : null);

function claudeLeftWebRight(): PaneNode {
  const left = newLeaf("claude");
  const { root, newPaneId } = splitPane(left, left.id, "right");
  return setLeaf(root, newPaneId, "web");
}
function setLeaf(root: PaneNode, id: string, contentId: string): PaneNode {
  if (root.kind === "leaf") return root.id === id ? { ...root, contentId } : root;
  return { ...root, first: setLeaf(root.first, id, contentId), second: setLeaf(root.second, id, contentId) };
}

describe("layout snapshot", () => {
  it("describes every non-PTY content and skips PTYs / unknown paths", () => {
    const saved = snapshotContents(tabs, (t) => describeContent(t, pathOf, urlOf));
    expect(saved.map((c) => c.id)).toEqual(["web", "ed", "md", "kb", "api"]);
    expect(saved.find((c) => c.id === "web")).toEqual({ id: "web", kind: "WebPreview", url: "http://localhost:3000" });
    expect(saved.find((c) => c.id === "ed")).toEqual({ id: "ed", kind: "CodeEditor", path: "src/a.ts", title: "notes" });
    expect(saved.find((c) => c.id === "kb")).toEqual({ id: "kb", kind: "Kanban" });
  });

  it("round-trips claude-left + web-right through JSON and finds what to restore", () => {
    const tree = claudeLeftWebRight();
    const entry: SavedWsLayout = {
      tabs: [{ tree, activePaneId: tree.id }],
      activeWsTab: 0,
      contents: snapshotContents(tabs, (t) => describeContent(t, pathOf, urlOf)),
    };
    const parsed = parseSavedLayouts(JSON.parse(JSON.stringify({ "/ws": entry })));
    const back = parsed["/ws"];
    expect(back.contents?.length).toBe(5);
    const restored = deserialize(back.tabs[0].tree)!;
    expect(treeContentIds(restored)).toEqual(["claude", "web"]);
    // After a restart the daemon brings the PTY back; the preview must be re-created.
    const missing = missingSavedContents(back, [restored], new Set(["claude"]));
    expect(missing).toEqual([{ id: "web", kind: "WebPreview", url: "http://localhost:3000" }]);
  });

  it("remaps respawned ids and leaves the rest alone", () => {
    const tree = claudeLeftWebRight();
    const out = remapContentIds(tree, new Map([["web", "web2"]]));
    expect(treeContentIds(out)).toEqual(["claude", "web2"]);
    expect(remapContentIds(tree, new Map())).toEqual(tree);
  });

  it("drops malformed entries and legacy snapshots keep working without contents", () => {
    const parsed = parseSavedLayouts({
      "/a": { tabs: [{ tree: newLeaf("x"), activePaneId: "p" }], activeWsTab: 1 },
      "/b": { tabs: "nope" },
      "/c": null,
      "/d": { tabs: [], activeWsTab: 0, contents: [{ id: 1 }, { id: "ok", kind: "Api" }, { id: "bad", kind: "Shell" }] },
    });
    expect(Object.keys(parsed)).toEqual(["/a", "/d"]);
    expect(parsed["/a"].contents).toBeUndefined();
    expect(parsed["/a"].activeWsTab).toBe(1);
    expect(parsed["/d"].contents).toEqual([{ id: "ok", kind: "Api" }]);
    expect(parseSavedLayouts("junk")).toEqual({});
  });

  it("ignores saved contents the trees no longer reference", () => {
    const entry: SavedWsLayout = {
      tabs: [{ tree: newLeaf("ed"), activePaneId: "" }],
      activeWsTab: 0,
      contents: [{ id: "ed", kind: "CodeEditor", path: "a" }, { id: "gone", kind: "Api" }],
    };
    const missing = missingSavedContents(entry, [newLeaf("ed")], new Set());
    expect(missing.map((c) => c.id)).toEqual(["ed"]);
  });
});
