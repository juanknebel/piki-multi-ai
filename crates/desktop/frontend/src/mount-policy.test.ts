import { describe, it, expect } from "vitest";
import { HiddenOutputBuffer, shouldFocusOnMount, shouldResync } from "./mount-policy";

describe("shouldResync", () => {
  it("is true only for a content that never mounted", () => {
    const seen = new Set<string>();
    expect(shouldResync("a", seen)).toBe(true);
    seen.add("a");
    expect(shouldResync("a", seen)).toBe(false);
    expect(shouldResync("b", seen)).toBe(true);
  });

  it("a click on a pane does not resync the others", () => {
    // Four panes, all mounted once; clicking pane 2 re-runs the mount path
    // for every leaf — none of them is due a resync.
    const seen = new Set(["p1", "p2", "p3", "p4"]);
    const due = ["p1", "p2", "p3", "p4"].filter((id) => shouldResync(id, seen));
    expect(due).toEqual([]);
  });
});

describe("shouldFocusOnMount", () => {
  it("focuses only the active pane's content", () => {
    expect(shouldFocusOnMount("a", "a")).toBe(true);
    expect(shouldFocusOnMount("b", "a")).toBe(false);
    expect(shouldFocusOnMount("b", null)).toBe(false);
    expect(shouldFocusOnMount("b", undefined)).toBe(false);
  });
});

describe("HiddenOutputBuffer", () => {
  const bytes = (n: number, fill = 1) => new Uint8Array(n).fill(fill);

  it("queues chunks and drains them in order", () => {
    const b = new HiddenOutputBuffer(100);
    expect(b.push(bytes(10, 1))).toEqual([]);
    expect(b.push(bytes(10, 2))).toEqual([]);
    expect(b.size).toBe(20);
    const out = b.drain();
    expect(out.map((c) => c[0])).toEqual([1, 2]);
    expect(b.size).toBe(0);
    expect(b.drain()).toEqual([]);
  });

  it("hands everything back once the cap overflows, never dropping", () => {
    const b = new HiddenOutputBuffer(25);
    expect(b.push(bytes(20, 1))).toEqual([]);
    const flushed = b.push(bytes(10, 2));
    expect(flushed.map((c) => c.length)).toEqual([20, 10]);
    expect(b.size).toBe(0);
  });
});
