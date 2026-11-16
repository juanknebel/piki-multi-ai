import { describe, expect, it } from "vitest";
import type { ProjectTreeRow } from "./ipc";
import { pickLastWorkspace, stepWorkspace, visibleWorkspaceIndices } from "./workspace-nav";

const checkout = (index: number, depth = 1): ProjectTreeRow => ({
  type: "checkout",
  index,
  kind: "worktree",
  depth,
  hoisted: true,
  is_parent: false,
});

const project = (key: string, collapsed = false): ProjectTreeRow => ({
  type: "project",
  key,
  collapsed,
  hidden: [],
  project_id: 1,
  name: "proj",
  color: 0,
  bucket: "project",
});

const repo = (root = "/repo"): ProjectTreeRow => ({
  type: "repo",
  root,
  display: "repo",
  depth: 1,
});

describe("visibleWorkspaceIndices", () => {
  it("keeps sidebar order and drops every header row", () => {
    const rows: ProjectTreeRow[] = [
      project("project:1"),
      repo(),
      checkout(3),
      checkout(0),
      { type: "dir", path: "/notes" },
      checkout(1),
    ];
    expect(visibleWorkspaceIndices(rows)).toEqual([3, 0, 1]);
  });

  it("skips what a collapsed project hides — those are not rows at all", () => {
    // What `project_tree` emits for a collapsed project: the header only.
    const rows: ProjectTreeRow[] = [
      project("project:1", true),
      project("project:2"),
      checkout(3),
    ];
    expect(visibleWorkspaceIndices(rows)).toEqual([3]);
  });

  it("visits a workspace in two projects only once", () => {
    const rows: ProjectTreeRow[] = [
      project("project:1"),
      checkout(0),
      project("project:2"),
      checkout(0),
      checkout(1),
    ];
    expect(visibleWorkspaceIndices(rows)).toEqual([0, 1]);
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
