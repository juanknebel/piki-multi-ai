import { describe, expect, it } from "vitest";
import { findMatchOffsets } from "./markdown-viewer";

describe("findMatchOffsets", () => {
  it("finds every non-overlapping hit case-insensitively", () => {
    expect(findMatchOffsets("Foo foo FOO", "foo")).toEqual([0, 4, 8]);
  });

  it("advances past each hit so repeats do not overlap", () => {
    expect(findMatchOffsets("aaa", "aa")).toEqual([0]);
  });

  it("returns no hits for an empty query or no match", () => {
    expect(findMatchOffsets("hello", "")).toEqual([]);
    expect(findMatchOffsets("hello", "z")).toEqual([]);
  });
});
