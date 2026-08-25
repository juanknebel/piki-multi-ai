import { describe, expect, it } from "vitest";
import { BRANCH_LABEL_MAX, branchLabel, truncateMiddle } from "./labels";

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
