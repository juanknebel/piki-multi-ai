// Pure agent-signal logic shared by the Agents panel, the status-bar
// segment, the activity-bar badge and the `Alt+A` jump. No DOM, no IPC —
// covered by agent-attention.test.ts. The wiring lives in
// components/agents-panel.ts.

import { agentStatusSeverity, type AgentRow } from "./types";

/** Severity at or above which an agent "needs you": waiting for permission,
 *  or idle/done with news the user hasn't looked at. Mirrors
 *  `piki_core::cli_agent::status_severity` (4 / 3). */
const ATTENTION_SEVERITY = 3;

/** The rows that need the user, worst first (permission before unseen
 *  news), stable within a severity so the panel order is the jump order. */
export function attentionRows(rows: AgentRow[]): AgentRow[] {
  return rows
    .filter((r) => r.status !== null && agentStatusSeverity(r.status, r.attention) >= ATTENTION_SEVERITY)
    .map((r, i) => ({ r, i, sev: agentStatusSeverity(r.status!, r.attention) }))
    .sort((a, b) => b.sev - a.sev || a.i - b.i)
    .map((x) => x.r);
}

/** Where `Alt+A` lands: the worst agent needing attention — or, when the
 *  user is already standing on one of them, the next one down the list
 *  (cyclic), so repeated presses walk through everything that needs them.
 *  `null` when nothing does. */
export function pickAttentionTarget(
  rows: AgentRow[],
  current: { workspace_idx: number; tab_id: string | undefined } | null,
): AgentRow | null {
  const targets = attentionRows(rows);
  if (targets.length === 0) return null;
  const here = current
    ? targets.findIndex((r) => r.workspace_idx === current.workspace_idx && r.tab_id === current.tab_id)
    : -1;
  return here < 0 ? targets[0] : targets[(here + 1) % targets.length];
}

/** Elapsed seconds for a row as of `nowMs`, ticking forward from the
 *  backend's snapshot taken at `fetchedAtMs`. `null` when no run is in
 *  flight. */
export function liveElapsedSecs(row: AgentRow, fetchedAtMs: number, nowMs: number): number | null {
  if (row.elapsed_secs === null || row.elapsed_secs === undefined) return null;
  return row.elapsed_secs + Math.max(0, Math.floor((nowMs - fetchedAtMs) / 1000));
}
