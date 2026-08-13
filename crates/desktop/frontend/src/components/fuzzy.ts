// Shared fuzzy matching + most-recently-used ranking for the command
// palette and the file search overlay.

/** Subsequence fuzzy match. Every query char must appear in order in `text`.
 *  Returns a score (higher = better) or null when it doesn't match.
 *  Bonuses: contiguous runs, word boundaries (start, after / _ - . space),
 *  camelCase humps. Penalty: how far the match spreads across the text. */
export function fuzzyScore(query: string, text: string): number | null {
  if (!query) return 0;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  let score = 0;
  let ti = 0;
  let first = -1;
  let prevMatch = -2;
  for (let qi = 0; qi < q.length; qi++) {
    const found = t.indexOf(q[qi], ti);
    if (found === -1) return null;
    let bonus = 1;
    if (found === prevMatch + 1) bonus += 3;
    const prev = text[found - 1];
    if (found === 0 || prev === "/" || prev === " " || prev === "-" || prev === "_" || prev === ".") {
      bonus += 4;
    } else if (
      prev &&
      prev === prev.toLowerCase() &&
      text[found] !== t[found] // an uppercase char in the original text
    ) {
      bonus += 2;
    }
    score += bonus;
    if (first === -1) first = found;
    prevMatch = found;
    ti = found + 1;
  }
  // Prefer tight matches over ones scattered across the whole string.
  score -= Math.floor((prevMatch - first - q.length + 1) / 3);
  return score;
}

/** Score a path: the basename counts double so `main` finds `src/main.ts`
 *  before a directory that merely contains the letters. */
export function fuzzyScorePath(query: string, path: string): number | null {
  const base = path.split("/").pop() ?? path;
  const baseScore = fuzzyScore(query, base);
  const fullScore = fuzzyScore(query, path);
  if (baseScore === null && fullScore === null) return null;
  return Math.max(baseScore !== null ? baseScore * 2 + 4 : -Infinity, fullScore ?? -Infinity);
}

// ── Most-recently-used ranking (persisted per namespace) ──────────────

const MRU_CAP = 50;

function mruLoad(ns: string): string[] {
  try {
    const raw = localStorage.getItem(`mru:${ns}`);
    const v = raw ? JSON.parse(raw) : [];
    return Array.isArray(v) ? v.filter((k): k is string => typeof k === "string") : [];
  } catch {
    return [];
  }
}

/** Record a use of `key`; most recent first, capped. */
export function mruBump(ns: string, key: string) {
  const list = mruLoad(ns).filter((k) => k !== key);
  list.unshift(key);
  try {
    localStorage.setItem(`mru:${ns}`, JSON.stringify(list.slice(0, MRU_CAP)));
  } catch {
    // Persistence is best-effort.
  }
}

/** Comparator helper: recency index of `key`, Infinity when never used. */
export function mruRank(ns: string, key: string): number {
  const i = mruLoad(ns).indexOf(key);
  return i === -1 ? Infinity : i;
}
