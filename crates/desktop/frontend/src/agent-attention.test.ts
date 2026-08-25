import { describe, expect, it } from "vitest";
import { attentionRows, liveElapsedSecs, pickAttentionTarget } from "./agent-attention";
import { formatElapsed, type AgentRow, type CliAgentStatus } from "./types";

function row(
  ws: number,
  tab: string,
  status: CliAgentStatus | null,
  attention = false,
  elapsed_secs: number | null = null,
): AgentRow {
  return {
    workspace_idx: ws,
    workspace_name: `ws${ws}`,
    tab_idx: 0,
    tab_id: tab,
    label: "Claude",
    alive: true,
    status,
    attention,
    summary: null,
    elapsed_secs,
  };
}

describe("attentionRows", () => {
  it("keeps only agents that need the user, permission before unseen news, stable otherwise", () => {
    const rows = [
      row(0, "a", "running"),
      row(0, "b", "idle", true),
      row(1, "c", "done", true),
      row(1, "d", "waiting-permission"),
      row(2, "e", "idle", false),
      row(2, "f", "done", false),
      row(2, "g", null),
    ];
    expect(attentionRows(rows).map((r) => r.tab_id)).toEqual(["d", "b", "c"]);
  });
});

describe("pickAttentionTarget", () => {
  const rows = [
    row(0, "a", "idle", true),
    row(1, "b", "waiting-permission"),
    row(2, "c", "done", true),
    row(2, "d", "running"),
  ];

  it("returns null when nothing needs attention", () => {
    expect(pickAttentionTarget([row(0, "x", "running")], null)).toBeNull();
    expect(pickAttentionTarget([], null)).toBeNull();
  });

  it("lands on the worst agent when the user is elsewhere", () => {
    expect(pickAttentionTarget(rows, { workspace_idx: 2, tab_id: "d" })?.tab_id).toBe("b");
    expect(pickAttentionTarget(rows, null)?.tab_id).toBe("b");
  });

  it("walks to the next one (cyclic) when already standing on a target", () => {
    expect(pickAttentionTarget(rows, { workspace_idx: 1, tab_id: "b" })?.tab_id).toBe("a");
    expect(pickAttentionTarget(rows, { workspace_idx: 0, tab_id: "a" })?.tab_id).toBe("c");
    expect(pickAttentionTarget(rows, { workspace_idx: 2, tab_id: "c" })?.tab_id).toBe("b");
  });
});

describe("liveElapsedSecs", () => {
  it("ticks the backend snapshot forward and never backwards", () => {
    const r = row(0, "a", "running", false, 100);
    expect(liveElapsedSecs(r, 1000, 1000)).toBe(100);
    expect(liveElapsedSecs(r, 1000, 4500)).toBe(103);
    expect(liveElapsedSecs(r, 5000, 1000)).toBe(100);
    expect(liveElapsedSecs(row(0, "b", "done"), 0, 9000)).toBeNull();
  });
});

describe("formatElapsed", () => {
  it("mirrors piki_core::cli_agent::format_elapsed", () => {
    expect(formatElapsed(0)).toBe("0s");
    expect(formatElapsed(45)).toBe("45s");
    expect(formatElapsed(192)).toBe("3m 12s");
    expect(formatElapsed(600)).toBe("10m 00s");
    expect(formatElapsed(3720)).toBe("1h 02m");
  });
});
