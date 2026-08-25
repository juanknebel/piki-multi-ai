// Most-recently-used ranking as pure list operations — the caller owns the
// persistence (the workspace switcher keeps its list under the
// `workspaceMru` settings key via `settingsStore.patch`). No DOM, no IPC;
// covered by mru.test.ts. `components/fuzzy.ts` has the older
// localStorage-backed variant used by the command palette and file search.

import { fuzzyScore } from "./components/fuzzy";

export const MRU_CAP = 50;

/** `list` with `key` moved to the front (most recent first), capped. */
export function mruBump(list: readonly string[], key: string, cap = MRU_CAP): string[] {
  return [key, ...list.filter((k) => k !== key)].slice(0, cap);
}

/** Recency index of `key` in `list`; `Infinity` when it was never used, so
 *  `a - b` comparators put unseen items last. */
export function mruRank(list: readonly string[], key: string): number {
  const i = list.indexOf(key);
  return i === -1 ? Infinity : i;
}

/** An item the switcher can rank: `key` is what the MRU list stores (the
 *  workspace path), `texts` are every string the query may match (name,
 *  branch, repo folder …), `order` breaks the remaining ties. */
export interface RankableItem {
  key: string;
  texts: string[];
  order: number;
}

/** Switcher ordering. Empty query: most recently used first, then the
 *  caller's `order`. Otherwise: fuzzy score across `texts` (best of them,
 *  so "wsauth" finds "ws-auth" and "auth" finds a branch called
 *  `feat/auth`), non-matches dropped, ties broken by recency then order. */
export function rankItems<T extends RankableItem>(items: readonly T[], query: string, mru: readonly string[]): T[] {
  const q = query.trim();
  if (!q) {
    return [...items].sort((a, b) => mruRank(mru, a.key) - mruRank(mru, b.key) || a.order - b.order);
  }
  return items
    .map((item) => {
      let score = -Infinity;
      for (const t of item.texts) {
        const s = fuzzyScore(q, t);
        if (s !== null && s > score) score = s;
      }
      return { item, score };
    })
    .filter((e) => e.score !== -Infinity)
    .sort(
      (a, b) =>
        b.score - a.score ||
        mruRank(mru, a.item.key) - mruRank(mru, b.item.key) ||
        a.item.order - b.item.order,
    )
    .map((e) => e.item);
}
