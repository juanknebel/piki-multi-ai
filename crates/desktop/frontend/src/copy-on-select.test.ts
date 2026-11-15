import { describe, expect, it } from "vitest";
import { createSelectionCopier, emptyCopyHint } from "./copy-on-select";

describe("createSelectionCopier", () => {
  it("selecting 40 rows is one clipboard write", () => {
    const c = createSelectionCopier();
    for (let i = 0; i < 40; i++) c.selectionChanged();
    expect(c.mouseUp()).toBe(true);
    // xterm's own document-level mouseup fires one more change before the flush
    c.selectionChanged();
    expect(c.flush("forty rows")).toBe("forty rows");
    // nothing left over: a later click-to-focus copies nothing
    expect(c.mouseUp()).toBe(false);
    expect(c.flush("forty rows")).toBeNull();
  });

  it("a click that only clears the selection copies nothing", () => {
    const c = createSelectionCopier();
    c.selectionChanged();
    expect(c.mouseUp()).toBe(true);
    expect(c.flush("")).toBeNull();
    expect(c.dirty).toBe(false);
  });

  it("a mouseup without any selection change is not a gesture", () => {
    const c = createSelectionCopier();
    expect(c.mouseUp()).toBe(false);
    expect(c.flush("stale")).toBeNull();
  });

  it("two gestures are two writes", () => {
    const c = createSelectionCopier();
    c.selectionChanged();
    c.mouseUp();
    expect(c.flush("one")).toBe("one");
    c.selectionChanged();
    c.selectionChanged();
    c.mouseUp();
    expect(c.flush("two")).toBe("two");
  });

  it("a double mouseup before the flush schedules only one flush", () => {
    const c = createSelectionCopier();
    c.selectionChanged();
    expect(c.mouseUp()).toBe(true);
    expect(c.mouseUp()).toBe(false);
    expect(c.flush("x")).toBe("x");
  });
});

describe("emptyCopyHint", () => {
  it("tells a plain empty copy to select first", () => {
    expect(emptyCopyHint(false, false)).toBe("Nothing selected — drag to select text first");
    expect(emptyCopyHint(false, true)).toBe("Nothing selected — drag to select text first");
  });

  it("points at Shift+drag when the app tracks the mouse", () => {
    expect(emptyCopyHint(true, false)).toBe(
      "This app tracks the mouse — hold Shift while dragging to select, then copy again",
    );
  });

  it("points at Select All on macOS, where Shift does not bypass mouse tracking", () => {
    expect(emptyCopyHint(true, true)).toBe(
      "This app tracks the mouse — use Select All from the right-click menu, then copy",
    );
  });
});
