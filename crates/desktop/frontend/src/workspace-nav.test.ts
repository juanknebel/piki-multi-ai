import { describe, expect, it } from "vitest";
import type { SidebarRow } from "./ipc";
import { pickLastWorkspace, stepWorkspace, visibleWorkspaceIndices } from "./workspace-nav";

const ws = (index: number, kind: "standalone" | "parent" | "child" = "standalone"): SidebarRow => ({
  type: "workspace",
  index,
  kind,
  family_key: kind === "standalone" ? null : "/repo",
  collapsed: kind === "parent" ? false : null,
});

describe("visibleWorkspaceIndices", () => {
  it("keeps sidebar order and drops group headers", () => {
    const rows: SidebarRow[] = [
      { type: "prReviewHeader", collapsed: false, family_key: "pr-review" },
      ws(3),
      ws(0, "parent"),
      ws(1, "child"),
      ws(2),
    ];
    expect(visibleWorkspaceIndices(rows)).toEqual([3, 0, 1, 2]);
  });

  it("skips a collapsed family's children — they are not rows", () => {
    // What `sidebar_rows` emits for a collapsed parent: the parent only.
    const rows: SidebarRow[] = [
      { type: "workspace", index: 0, kind: "parent", family_key: "/repo", collapsed: true },
      ws(3),
    ];
    expect(visibleWorkspaceIndices(rows)).toEqual([0, 3]);
  });

  it("is empty for no rows", () => {
    expect(visibleWorkspaceIndices([])).toEqual([]);
  });
});

describe("stepWorkspace", () => {
  const visible = [3, 0, 1, 2];

  it("walks forward and backward in visual order", () => {
    expect(stepWorkspace(visible, 3, 1)).toBe(0);
    expect(stepWorkspace(visible, 1, 1)).toBe(2);
    expect(stepWorkspace(visible, 0, -1)).toBe(3);
  });

  it("wraps at both ends", () => {
    expect(stepWorkspace(visible, 2, 1)).toBe(3);
    expect(stepWorkspace(visible, 3, -1)).toBe(2);
  });

  it("starts at the first row when the active workspace is not visible", () => {
    // Its family was collapsed under it: walk from the top instead of nowhere.
    expect(stepWorkspace(visible, 7, 1)).toBe(0);
    expect(stepWorkspace(visible, 7, -1)).toBe(2);
  });

  it("returns null when there is nowhere to go", () => {
    expect(stepWorkspace([], 0, 1)).toBeNull();
    expect(stepWorkspace([4], 4, 1)).toBeNull();
    expect(stepWorkspace([4], 4, -1)).toBeNull();
  });
});

describe("pickLastWorkspace", () => {
  const paths = ["/w/main", "/w/auth", "/w/docs"];

  it("picks the most recent workspace that is not the active one", () => {
    expect(pickLastWorkspace(["/w/auth", "/w/docs", "/w/main"], paths, 1)).toBe(2);
    expect(pickLastWorkspace(["/w/auth", "/w/docs", "/w/main"], paths, 2)).toBe(1);
  });

  it("walks past entries that are no longer loaded", () => {
    const mru = ["/w/auth", "/w/deleted", "/w/docs"];
    expect(pickLastWorkspace(mru, paths, 1)).toBe(2);
  });

  it("returns null when nothing in the MRU is loaded any more", () => {
    expect(pickLastWorkspace([], paths, 0)).toBeNull();
    expect(pickLastWorkspace(["/w/gone"], paths, 0)).toBeNull();
    expect(pickLastWorkspace(["/w/main"], paths, 0)).toBeNull();
  });
});
