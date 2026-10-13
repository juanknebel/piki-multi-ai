// The hovered status-bar segment must survive re-renders. Hovering steals
// the node's `title` into `data-title` (tooltip.ts suppresses the native
// tooltip that way), and the morph comparison has to see through that —
// otherwise the node under the cursor reads as "changed" on EVERY render,
// so while an agent streams it is replaced several times a second: the
// icon visibly blinks and clicks land split across node generations and
// never fire. The comparator is pure (DOM-free shapes); `shapeOf` reads a
// shape off a live element.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { sameSegmentShape, type SegmentShape } from "./status-bar";

function shape(over: Partial<SegmentShape> = {}): SegmentShape {
  return {
    innerHTML: '<svg class="icon icon-terminal"></svg>',
    title: "Toggle the drop-down terminal",
    dataTitle: null,
    attrs: [
      ["class", "status-item clickable status-home-term"],
      ["data-seg", "drop-term"],
    ],
    ...over,
  };
}

describe("sameSegmentShape", () => {
  it("keeps an identical segment", () => {
    expect(sameSegmentShape(shape(), shape())).toBe(true);
  });

  it("sees through the tooltip's stolen title (hovered node survives)", () => {
    const hovered = shape({ title: null, dataTitle: "Toggle the drop-down terminal" });
    expect(sameSegmentShape(hovered, shape())).toBe(true);
  });

  it("replaces on a genuinely different title, hovered or not", () => {
    expect(sameSegmentShape(shape({ title: "old" }), shape({ title: "new" }))).toBe(false);
    expect(
      sameSegmentShape(shape({ title: null, dataTitle: "old" }), shape({ title: "new" })),
    ).toBe(false);
  });

  it("replaces when content or any other attribute changed", () => {
    expect(sameSegmentShape(shape({ innerHTML: "other" }), shape())).toBe(false);
    expect(
      sameSegmentShape(
        shape({
          attrs: [
            ["class", "status-item clickable status-home-term"],
            ["data-seg", "sessions"],
          ],
        }),
        shape(),
      ),
    ).toBe(false);
    expect(
      sameSegmentShape(
        shape({ attrs: [["class", "status-item clickable status-home-term"]] }),
        shape(),
      ),
    ).toBe(false);
  });
});

describe("status-bar morph wiring", () => {
  const src = readFileSync(
    decodeURIComponent(new URL("./status-bar.ts", import.meta.url).pathname),
    "utf8",
  );

  it("morphs by sameSegmentShape, never a raw outerHTML compare", () => {
    expect(src).toContain("sameSegmentShape(shapeOf(cur), shapeOf(n))");
    expect(src, "raw outerHTML compare reintroduces the hover blink").not.toMatch(
      /cur\.outerHTML\s*!==?\s*n\.outerHTML/,
    );
  });
});
