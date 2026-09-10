import { describe, expect, it } from "vitest";
import {
  ZOOM_DEFAULT,
  ZOOM_LEVELS,
  ZOOM_MAX,
  ZOOM_MIN,
  clampZoom,
  formatZoom,
  stepZoom,
  terminalFontSizeFor,
} from "./zoom";

describe("clampZoom", () => {
  it("returns the default for junk and clamps to the level range", () => {
    expect(clampZoom(undefined)).toBe(ZOOM_DEFAULT);
    expect(clampZoom("abc")).toBe(ZOOM_DEFAULT);
    expect(clampZoom(0)).toBe(ZOOM_DEFAULT);
    expect(clampZoom(-2)).toBe(ZOOM_DEFAULT);
    expect(clampZoom(0.1)).toBe(ZOOM_MIN);
    expect(clampZoom(9)).toBe(ZOOM_MAX);
    expect(clampZoom("1.25")).toBe(1.25);
    expect(clampZoom(1.2345)).toBe(1.23);
  });
});

describe("stepZoom", () => {
  it("walks the level list in both directions and sticks at the ends", () => {
    expect(stepZoom(1, 1)).toBe(1.1);
    expect(stepZoom(1, -1)).toBe(0.9);
    expect(stepZoom(ZOOM_MAX, 1)).toBe(ZOOM_MAX);
    expect(stepZoom(ZOOM_MIN, -1)).toBe(ZOOM_MIN);
  });

  it("snaps an off-grid value to the neighbouring level", () => {
    expect(stepZoom(1.05, 1)).toBe(1.1);
    expect(stepZoom(1.05, -1)).toBe(1);
  });

  it("reaches every level from the default", () => {
    let z = ZOOM_DEFAULT;
    const up: number[] = [];
    while (z < ZOOM_MAX) { z = stepZoom(z, 1); up.push(z); }
    expect(up).toEqual(ZOOM_LEVELS.filter((l) => l > ZOOM_DEFAULT));
  });
});

describe("terminalFontSizeFor / formatZoom", () => {
  it("scales the 14px terminal font with the zoom, rounded to whole px", () => {
    expect(terminalFontSizeFor(1)).toBe(14);
    expect(terminalFontSizeFor(1.5)).toBe(21);
    expect(terminalFontSizeFor(0.7)).toBe(10);
    expect(terminalFontSizeFor(NaN)).toBe(14);
  });

  it("formats as a percentage", () => {
    expect(formatZoom(1)).toBe("100%");
    expect(formatZoom(1.25)).toBe("125%");
  });
});
