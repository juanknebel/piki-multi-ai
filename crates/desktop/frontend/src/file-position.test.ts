import { describe, expect, it, beforeEach } from "vitest";
import {
  POSITION_CAP,
  clampLine,
  clampPosition,
  forgetFilePositions,
  positionKey,
  recallFilePosition,
  rememberFilePosition,
} from "./file-position";

const pos = (top: number, anchor = 0, head = anchor) => ({ top, anchor, head });

beforeEach(() => forgetFilePositions());

describe("positionKey", () => {
  it("separates the same path in two workspaces", () => {
    expect(positionKey(0, "src/main.rs")).not.toBe(positionKey(1, "src/main.rs"));
  });
});

describe("clampPosition", () => {
  it("is null when the file was never opened", () => {
    expect(clampPosition(undefined, 100)).toBeNull();
  });

  it("keeps a position that still fits", () => {
    expect(clampPosition(pos(240, 10, 20), 100)).toEqual({ top: 240, anchor: 10, head: 20 });
  });

  it("pulls offsets inside a file that shrank", () => {
    // CodeMirror throws on a selection past the end of the doc, so a stale
    // offset from before an edit must never reach it.
    expect(clampPosition(pos(900, 5000, 5010), 42)).toEqual({ top: 900, anchor: 42, head: 42 });
  });

  it("survives an empty file and negative junk", () => {
    expect(clampPosition(pos(-5, -3, -1), 0)).toEqual({ top: 0, anchor: 0, head: 0 });
  });

  it("rounds fractional offsets", () => {
    expect(clampPosition(pos(12.4, 7.6, 8.2), 100)).toEqual({ top: 12, anchor: 8, head: 8 });
  });
});

describe("clampLine", () => {
  it("keeps a line inside the file", () => {
    expect(clampLine(37, 100)).toBe(37);
  });

  it("is 1-based at both ends", () => {
    expect(clampLine(0, 100)).toBe(1);
    expect(clampLine(-4, 100)).toBe(1);
    expect(clampLine(500, 100)).toBe(100);
  });

  it("holds up for an empty file and for junk", () => {
    expect(clampLine(3, 0)).toBe(1);
    expect(clampLine(Number.NaN, 50)).toBe(1);
    expect(clampLine(Number.POSITIVE_INFINITY, 50)).toBe(1);
  });
});

describe("the session cache", () => {
  it("gives back what was stored", () => {
    rememberFilePosition("0:a.rs", pos(120, 30, 34));
    expect(recallFilePosition("0:a.rs")).toEqual({ top: 120, anchor: 30, head: 34 });
  });

  it("does not answer for a file nobody opened", () => {
    expect(recallFilePosition("0:never.rs")).toBeUndefined();
  });

  it("overwrites the previous position of the same file", () => {
    rememberFilePosition("0:a.rs", pos(120));
    rememberFilePosition("0:a.rs", pos(999));
    expect(recallFilePosition("0:a.rs")).toEqual({ top: 999, anchor: 0, head: 0 });
  });

  it("drops the least recently stored file past the cap", () => {
    rememberFilePosition("keep", pos(1), 2);
    rememberFilePosition("drop-me", pos(2), 2);
    rememberFilePosition("keep", pos(3), 2); // re-store moves it back to recent
    rememberFilePosition("newest", pos(4), 2);
    expect(recallFilePosition("drop-me")).toBeUndefined();
    expect(recallFilePosition("keep")).toEqual({ top: 3, anchor: 0, head: 0 });
    expect(recallFilePosition("newest")).toEqual({ top: 4, anchor: 0, head: 0 });
  });

  it("holds the documented number of files", () => {
    for (let i = 0; i < POSITION_CAP + 5; i++) rememberFilePosition(`f${i}`, pos(i));
    expect(recallFilePosition("f0")).toBeUndefined();
    expect(recallFilePosition(`f${POSITION_CAP + 4}`)).toBeDefined();
    expect(recallFilePosition(`f5`)).toBeDefined();
  });
});
