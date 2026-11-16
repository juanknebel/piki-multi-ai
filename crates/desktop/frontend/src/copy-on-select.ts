// Selection-to-clipboard helpers (no DOM, vitest-covered): the copy-on-select
// gesture machine plus the guidance for an explicit Copy that found nothing.
//
// xterm fires `onSelectionChange` for every pointer move while the user
// drags — selecting 40 rows is 40+ events — and the old code wrote the
// clipboard on each one. This machine turns a gesture into ONE write:
//
//   selectionChanged()  — any number of times; marks the gesture dirty
//   mouseUp()           — the gesture ended; tells the caller to schedule a
//                         flush (xterm fires one more selection change from
//                         its own document-level mouseup *after* ours, so
//                         the flush must run a tick later, not inline)
//   flush(selection)    — the deferred read: returns the text to copy once,
//                         or null when nothing was selected / no gesture
//
// A click that merely clears the selection is a gesture too (dirty, then an
// empty flush) — it copies nothing and leaves no stale dirty flag behind, so
// the next click-to-focus never re-copies the previous selection.

export interface SelectionCopier {
  /** xterm `onSelectionChange` — however many times it fires. */
  selectionChanged(): void;
  /** The pointer was released over the terminal. True when a flush should
   *  be scheduled (a selection change happened since the last flush). */
  mouseUp(): boolean;
  /** Deferred after `mouseUp`: the text to copy exactly once, else null. */
  flush(selection: string): string | null;
  /** True between a selection change and its flush. */
  readonly dirty: boolean;
}

export function createSelectionCopier(): SelectionCopier {
  let dirty = false;
  let pending = false;
  return {
    selectionChanged() {
      dirty = true;
    },
    mouseUp() {
      if (!dirty) return false;
      if (pending) return false; // one flush per gesture, even on a double mouseup
      pending = true;
      return true;
    },
    flush(selection) {
      pending = false;
      if (!dirty) return null;
      dirty = false;
      return selection.length > 0 ? selection : null;
    },
    get dirty() {
      return dirty;
    },
  };
}

/** Guidance toast for an explicit Copy (menu / Ctrl+Shift+C) that found no
 *  selection. `mouseTracking` is `terminal.modes.mouseTrackingMode !==
 *  "none"`; `mac` is `isMac` (shortcuts.ts).
 *
 *  A program that reports the mouse (mirror's TUI enables 1000/1003/1006)
 *  eats plain drags — mirror even selects inside its own UI — so xterm's
 *  selection stays empty and the copy would silently do nothing. Off macOS
 *  xterm bypasses the app while Shift is held (`shouldForceSelection`), so
 *  Shift+drag selects; on macOS there is no bypass (we don't set
 *  `macOptionClickForcesSelection`), so Select All is the way out. */
export function emptyCopyHint(mouseTracking: boolean, mac: boolean): string {
  if (!mouseTracking) return "Nothing selected — drag to select text first";
  if (!mac) return "This app tracks the mouse — hold Shift while dragging to select, then copy again";
  return "This app tracks the mouse — use Select All from the right-click menu, then copy";
}
