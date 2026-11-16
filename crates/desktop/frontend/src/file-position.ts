// Reading position of a file in the read-only viewer
// (`components/file-viewer.ts`), remembered for the life of the session so
// reopening the same file — from project search, the file explorer, or after
// a stray click on the backdrop closed it — lands where you left off instead
// of back at line 1.
//
// In memory on purpose: this is reading state, not a preference. It must not
// outlive the app or grow `settings.json`, so the cache lives here and is
// capped like `mru.ts`. Covered by file-position.test.ts.

/** Files kept in the cache; the least recently stored one is dropped. */
export const POSITION_CAP = 50;

export interface FilePosition {
  /** `.cm-scroller` scrollTop in px — the reading position itself. */
  top: number;
  /** Selection offsets, so the cursor survives the round trip too. */
  anchor: number;
  head: number;
}

/** Cache key. Workspaces have their own checkouts, so the same relative
 *  path in two of them is two different files. */
export function positionKey(workspaceIdx: number, path: string): string {
  return `${workspaceIdx}:${path}`;
}

/** `pos` pulled inside a `docLength`-long document, or null when there is
 *  nothing to restore. The file can change on disk between two opens and
 *  `EditorState` throws on a selection past the end of the doc, so a stale
 *  offset must never reach CodeMirror. */
export function clampPosition(
  pos: FilePosition | undefined,
  docLength: number,
): FilePosition | null {
  if (!pos) return null;
  const inDoc = (n: number) => Math.min(Math.max(Math.round(n), 0), Math.max(docLength, 0));
  return {
    top: Math.max(0, Math.round(pos.top)),
    anchor: inDoc(pos.anchor),
    head: inDoc(pos.head),
  };
}

/** A 1-based line number pulled inside a `totalLines`-long document — the
 *  same staleness guard for a search hit's `line_num`. */
export function clampLine(line: number, totalLines: number): number {
  if (!isFinite(line)) return 1;
  return Math.min(Math.max(Math.round(line), 1), Math.max(totalLines, 1));
}

const remembered = new Map<string, FilePosition>();

/** Store `pos` under `key`, evicting the least recently stored file past
 *  `cap`. Re-inserting keeps `Map` iteration order as recency order. */
export function rememberFilePosition(key: string, pos: FilePosition, cap = POSITION_CAP) {
  remembered.delete(key);
  remembered.set(key, pos);
  while (remembered.size > cap) {
    const oldest = remembered.keys().next();
    if (oldest.done) break;
    remembered.delete(oldest.value);
  }
}

export function recallFilePosition(key: string): FilePosition | undefined {
  return remembered.get(key);
}

/** Drop the whole cache. Used by the tests; the app never needs it. */
export function forgetFilePositions() {
  remembered.clear();
}
