// Text-shortening rules shared by every chrome that shows a git branch
// (workspace list, status bar, switcher, dashboard, empty state). Pure —
// covered by labels.test.ts. Callers put the full value in a `title`.

/** Longest branch label painted anywhere; the middle is elided beyond it
 *  so both the prefix (`feat/`) and the distinctive tail stay visible. */
export const BRANCH_LABEL_MAX = 28;

/** `s` shortened to at most `max` characters by eliding the middle with an
 *  ellipsis (`feat/very-long…-name`). Short strings pass through. */
export function truncateMiddle(s: string, max: number): string {
  const chars = Array.from(s);
  if (max < 2 || chars.length <= max) return s;
  const keep = max - 1;
  const head = Math.ceil(keep / 2);
  const tail = keep - head;
  return `${chars.slice(0, head).join("")}…${tail > 0 ? chars.slice(-tail).join("") : ""}`;
}

/** The one branch-label rule: middle-truncated at `BRANCH_LABEL_MAX`.
 *  `null` (no branch known / not a git repo) renders as an em dash. */
export function branchLabel(branch: string | null | undefined, max = BRANCH_LABEL_MAX): string {
  if (!branch) return "—";
  return truncateMiddle(branch, max);
}
