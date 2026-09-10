import { describe, expect, it } from "vitest";
import {
  DIALOG_MIN_H,
  DIALOG_MIN_W,
  clampDialogSize,
  storedDialogSize,
} from "./dialog-resize";

describe("clampDialogSize", () => {
  it("passes through a size inside the bounds", () => {
    expect(clampDialogSize(800, 500, 1200, 900)).toEqual({ w: 800, h: 500 });
  });

  it("enforces the minimum floor", () => {
    expect(clampDialogSize(10, 10, 1200, 900)).toEqual({
      w: DIALOG_MIN_W,
      h: DIALOG_MIN_H,
    });
  });

  it("caps at the available viewport space", () => {
    expect(clampDialogSize(5000, 5000, 1200, 700)).toEqual({ w: 1200, h: 700 });
  });

  it("keeps the floor even when the viewport is smaller than it", () => {
    // A tiny window must not produce a negative/zero box.
    expect(clampDialogSize(500, 500, 200, 100)).toEqual({
      w: DIALOG_MIN_W,
      h: DIALOG_MIN_H,
    });
  });

  it("rounds to whole pixels", () => {
    expect(clampDialogSize(400.6, 300.4, 1200, 900)).toEqual({ w: 401, h: 300 });
  });
});

describe("storedDialogSize", () => {
  it("returns the entry for the id", () => {
    expect(storedDialogSize({ logs: { w: 700, h: 400 } }, "logs")).toEqual({
      w: 700,
      h: 400,
    });
  });

  it("returns null for a missing id or record", () => {
    expect(storedDialogSize(undefined, "logs")).toBeNull();
    expect(storedDialogSize({}, "logs")).toBeNull();
  });

  it("rejects malformed entries — settings are user-editable state", () => {
    expect(storedDialogSize({ logs: { w: "700", h: 400 } }, "logs")).toBeNull();
    expect(storedDialogSize({ logs: null }, "logs")).toBeNull();
    expect(storedDialogSize({ logs: { w: Infinity, h: 400 } }, "logs")).toBeNull();
    expect(storedDialogSize("nonsense", "logs")).toBeNull();
  });
});
