import { describe, expect, it } from "vitest";
import { getTabLabel, type TabInfo } from "./types";

const shell: TabInfo = { id: "t1", provider: "Shell", alive: true } as TabInfo;

describe("getTabLabel", () => {
  it("falls back to the provider label", () => {
    expect(getTabLabel(shell)).toBe("Shell");
  });

  it("uses the terminal (OSC) title when there is no custom title", () => {
    expect(getTabLabel(shell, "zsh — ~/proj")).toBe("zsh — ~/proj");
    expect(getTabLabel(shell, "   ")).toBe("Shell");
    expect(getTabLabel(shell, null)).toBe("Shell");
  });

  it("a user rename always wins over the terminal title", () => {
    const renamed = { ...shell, custom_title: "build" };
    expect(getTabLabel(renamed, "zsh — ~/proj")).toBe("build");
    const blank = { ...shell, custom_title: "  " };
    expect(getTabLabel(blank, "zsh — ~/proj")).toBe("zsh — ~/proj");
  });
});
