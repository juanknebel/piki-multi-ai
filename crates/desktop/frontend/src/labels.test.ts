import { describe, expect, it } from "vitest";
import {
  BRANCH_LABEL_MAX,
  branchLabel,
  homeRelative,
  pathLabel,
  truncateMiddle,
} from "./labels";

describe("truncateMiddle", () => {
  it("passes short strings through", () => {
    expect(truncateMiddle("main", 10)).toBe("main");
    expect(truncateMiddle("", 10)).toBe("");
  });

  it("keeps the head and the tail, never exceeding max", () => {
    const out = truncateMiddle("feat/really-long-branch-name-here", 12);
    expect(out).toBe("feat/r…-here");
    expect(Array.from(out).length).toBe(12);
    expect(out.startsWith("feat/")).toBe(true);
  });

  it("counts code points, not UTF-16 units", () => {
    expect(truncateMiddle("ééééééééé", 5)).toBe("éé…éé");
  });
});

describe("branchLabel", () => {
  it("renders an em dash when the branch is unknown", () => {
    expect(branchLabel(null)).toBe("—");
    expect(branchLabel(undefined)).toBe("—");
    expect(branchLabel("")).toBe("—");
  });

  it("applies the shared cap", () => {
    const long = "release/2026-08-25-persistent-sessions-desktop";
    expect(Array.from(branchLabel(long)).length).toBe(BRANCH_LABEL_MAX);
    expect(branchLabel("nightly")).toBe("nightly");
  });
});

describe("homeRelative", () => {
  it("rewrites a path under the home directory", () => {
    expect(homeRelative("/home/zero/git/piki/piki-vt100", "/home/zero")).toBe("~/git/piki/piki-vt100");
    expect(homeRelative("/home/zero", "/home/zero")).toBe("~");
    expect(homeRelative("/home/zero/git", "/home/zero/")).toBe("~/git");
  });

  it("leaves anything else alone", () => {
    expect(homeRelative("/srv/repos/x", "/home/zero")).toBe("/srv/repos/x");
    // A sibling whose name merely starts with the home path is NOT under it.
    expect(homeRelative("/home/zerox/git", "/home/zero")).toBe("/home/zerox/git");
    expect(homeRelative("/home/zero/git", null)).toBe("/home/zero/git");
    expect(homeRelative("/home/zero/git", "")).toBe("/home/zero/git");
  });
});

describe("pathLabel", () => {
  it("abbreviates then middle-truncates", () => {
    expect(pathLabel("/home/zero/git/piki/piki-vt100", "/home/zero")).toBe("~/git/piki/piki-vt100");
    const long = "/home/zero/.local/share/piki-multi/worktrees/agent-multi/nightly";
    const label = pathLabel(long, "/home/zero", 30);
    expect(Array.from(label).length).toBe(30);
    expect(label.startsWith("~/.local")).toBe(true);
    expect(label.endsWith("nightly")).toBe(true);
  });
});
