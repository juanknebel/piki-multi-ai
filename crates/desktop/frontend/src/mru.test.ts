import { describe, expect, it } from "vitest";
import { mruBump, mruRank, rankItems } from "./mru";

describe("mruBump / mruRank", () => {
  it("moves the key to the front and dedups", () => {
    expect(mruBump(["a", "b", "c"], "b")).toEqual(["b", "a", "c"]);
    expect(mruBump([], "x")).toEqual(["x"]);
    expect(mruBump(["a"], "a")).toEqual(["a"]);
  });

  it("caps the list", () => {
    const list = Array.from({ length: 5 }, (_, i) => `k${i}`);
    expect(mruBump(list, "new", 3)).toEqual(["new", "k0", "k1"]);
  });

  it("ranks by recency, unseen last", () => {
    const mru = ["recent", "older"];
    expect(mruRank(mru, "recent")).toBe(0);
    expect(mruRank(mru, "older")).toBe(1);
    expect(mruRank(mru, "never")).toBe(Infinity);
  });
});

const items = [
  { key: "/w/main", texts: ["main", "agent-multi", "main"], order: 0 },
  { key: "/w/ws-auth", texts: ["ws-auth", "agent-multi", "feat/auth"], order: 1 },
  { key: "/w/docs", texts: ["docs", "agent-multi", "docs-refresh"], order: 2 },
];

describe("rankItems", () => {
  it("puts the most recently used workspace first when the query is empty", () => {
    const mru = ["/w/docs", "/w/main"];
    expect(rankItems(items, "", mru).map((i) => i.key)).toEqual(["/w/docs", "/w/main", "/w/ws-auth"]);
  });

  it("falls back to the caller's order without MRU history", () => {
    expect(rankItems(items, "  ", []).map((i) => i.key)).toEqual(["/w/main", "/w/ws-auth", "/w/docs"]);
  });

  it("matches fuzzily across every text: wsauth finds ws-auth", () => {
    const ranked = rankItems(items, "wsauth", []);
    expect(ranked[0].key).toBe("/w/ws-auth");
    expect(ranked.map((i) => i.key)).not.toContain("/w/main");
  });

  it("matches through the branch and drops non-matches", () => {
    expect(rankItems(items, "feat/au", []).map((i) => i.key)).toEqual(["/w/ws-auth"]);
    expect(rankItems(items, "zzz", [])).toEqual([]);
  });

  it("breaks score ties by recency", () => {
    const twins = [
      { key: "a", texts: ["api-one"], order: 0 },
      { key: "b", texts: ["api-two"], order: 1 },
    ];
    expect(rankItems(twins, "api", ["b"]).map((i) => i.key)).toEqual(["b", "a"]);
  });
});
