// Debug counters for docs/performance.md — compiled to no-ops in production.
//
// In a dev build (`import.meta.env.DEV`) `perfCount(name)` increments a
// named counter and `window.__pikiPerf` exposes `{ counters, reset }` to the
// devtools console, e.g. after clicking between panes:
//   __pikiPerf.counters   // { "pane.render": 3, "terminal.resync": 1, … }
//   __pikiPerf.reset()

const DEV = typeof import.meta !== "undefined" && Boolean(import.meta.env?.DEV);

const counters: Record<string, number> = {};

export function perfCount(name: string, by = 1): void {
  if (!DEV) return;
  counters[name] = (counters[name] ?? 0) + by;
}

export function perfCounters(): Readonly<Record<string, number>> {
  return counters;
}

export function perfReset(): void {
  for (const k of Object.keys(counters)) delete counters[k];
}

if (DEV && typeof window !== "undefined") {
  (window as unknown as { __pikiPerf: unknown }).__pikiPerf = { counters, reset: perfReset };
}
