import { describe, expect, it } from "vitest";
import { DENSITIES, DENSITY_DEFAULT, DENSITY_LABELS, DENSITY_SETTINGS_KEY, normalizeDensity } from "./density";

describe("density contract (shared with the visual track)", () => {
  it("exposes exactly compact | normal | comfortable, default normal, under appearance.density", () => {
    expect([...DENSITIES]).toEqual(["compact", "normal", "comfortable"]);
    expect(DENSITY_DEFAULT).toBe("normal");
    expect(DENSITY_SETTINGS_KEY).toBe("appearance.density");
    for (const d of DENSITIES) expect(DENSITY_LABELS[d]).toBeTruthy();
  });

  it("normalizes junk to the default and keeps valid values", () => {
    expect(normalizeDensity("compact")).toBe("compact");
    expect(normalizeDensity("comfortable")).toBe("comfortable");
    expect(normalizeDensity("normal")).toBe("normal");
    expect(normalizeDensity(undefined)).toBe("normal");
    expect(normalizeDensity("Compact")).toBe("normal");
    expect(normalizeDensity(3)).toBe("normal");
    expect(normalizeDensity({})).toBe("normal");
  });
});
