import { beforeEach, describe, expect, it } from "vitest";
import { fuzzyScore, fuzzyScorePath, mruBump, mruRank } from "./fuzzy";

describe("fuzzyScore", () => {
  it("returns 0 for an empty query and null when a char is missing", () => {
    expect(fuzzyScore("", "anything")).toBe(0);
    expect(fuzzyScore("xyz", "abc")).toBeNull();
    expect(fuzzyScore("cba", "abc")).toBeNull(); // order matters
  });

  it("prefers contiguous and word-boundary matches over scattered ones", () => {
    const tight = fuzzyScore("ws", "ws-auth")!;
    const scattered = fuzzyScore("ws", "workspace-list-settings")!;
    expect(tight).toBeGreaterThan(scattered);
    expect(fuzzyScore("wsauth", "ws-auth")).not.toBeNull();
  });

  it("is case-insensitive and rewards camelCase humps", () => {
    expect(fuzzyScore("tsb", "toggleSideBar")).not.toBeNull();
    expect(fuzzyScore("TSB", "togglesidebar")).not.toBeNull();
    expect(fuzzyScore("tsb", "toggleSideBar")!).toBeGreaterThan(fuzzyScore("tsb", "togglesidebar")!);
  });
});

describe("fuzzyScorePath", () => {
  it("weights the basename so `main` finds src/main.ts before a dir with the letters", () => {
    const file = fuzzyScorePath("main", "src/main.ts")!;
    const dir = fuzzyScorePath("main", "domain/index.ts")!;
    expect(file).toBeGreaterThan(dir);
  });

  it("still matches through the directory part", () => {
    expect(fuzzyScorePath("comp/tab", "src/components/tab-bar.ts")).not.toBeNull();
    expect(fuzzyScorePath("zzz", "src/components/tab-bar.ts")).toBeNull();
  });
});

describe("mru ranking", () => {
  beforeEach(() => {
    const mem = new Map<string, string>();
    (globalThis as { localStorage?: unknown }).localStorage = {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => void mem.set(k, v),
      removeItem: (k: string) => void mem.delete(k),
      clear: () => mem.clear(),
    };
  });

  it("ranks the most recent use first and never-used as Infinity", () => {
    expect(mruRank("t", "a")).toBe(Infinity);
    mruBump("t", "a");
    mruBump("t", "b");
    expect(mruRank("t", "b")).toBe(0);
    expect(mruRank("t", "a")).toBe(1);
    mruBump("t", "a");
    expect(mruRank("t", "a")).toBe(0);
  });

  it("keeps namespaces apart", () => {
    mruBump("palette", "x");
    expect(mruRank("files", "x")).toBe(Infinity);
  });
});
