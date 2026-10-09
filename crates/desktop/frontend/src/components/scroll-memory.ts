// Remembering where a panel was scrolled across a tab switch.
//
// A content that is not on the visible tab is hidden (`display: none`) and
// parked in `#pane-holding` by `pane-view.ts detachPanelElements`. Either one
// drops the element's layout box, and the browser zeroes every `scrollTop`
// inside it — so the panel comes back at the top.
//
// Reading the position on the way out does NOT work: on a top-level tab
// switch `render()` detaches the outgoing tab's panes BEFORE `syncMounts`
// calls the panel's `unmount*`, so a capture there records a zero. The
// position is therefore recorded while the panel is VISIBLE and re-applied
// by the next mount. A `scroll` event cannot fire while the element is
// detached, so the last value recorded is always one the user read at.
//
// CodeMirror panels use `EditorView.scrollSnapshot()` instead, which restores
// a document position rather than a pixel offset.

export interface ScrollPos {
  top: number;
  left: number;
}

export function readScroll(el: Element): ScrollPos {
  return { top: el.scrollTop, left: el.scrollLeft };
}

/** Put `pos` back. A no-op when there is nothing remembered, or when the
 *  element is gone (a re-render may not have rebuilt that scroller). */
export function applyScroll(el: Element | null | undefined, pos: ScrollPos | undefined) {
  if (!el || !pos) return;
  el.scrollTop = pos.top;
  el.scrollLeft = pos.left;
}

/** Record every scroll inside `root` into `into`, under the key `keyOf`
 *  gives that scroller (null = don't remember this one).
 *
 *  One listener on the panel's own root, in the CAPTURE phase: `scroll` does
 *  not bubble, but it does reach ancestors on the way down, so this keeps
 *  working across the re-renders that replace the scrollers underneath —
 *  which is why the kanban board can rebuild its columns and still come back
 *  to the same place. */
export function trackScroll(
  root: HTMLElement,
  into: Map<string, ScrollPos>,
  keyOf: (el: Element) => string | null,
) {
  root.addEventListener(
    "scroll",
    (e) => {
      const el = e.target;
      if (!(el instanceof Element)) return;
      const key = keyOf(el);
      if (key) into.set(key, readScroll(el));
    },
    true,
  );
}
