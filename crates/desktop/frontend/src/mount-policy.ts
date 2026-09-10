// Pure decisions of the terminal mount path (vitest-covered): what a mount
// may do besides reparenting the element. `terminal-panel.ts` and
// `pane-view.ts` consult these so a click on a pane never repeats work the
// other panes already paid for.

/** Ask the backend for a daemon restore buffer only on the FIRST mount of a
 *  content: xterm keeps its own state across reparents, and a resync on
 *  every mount re-sent every remote's restore buffer on every pane click. */
export function shouldResync(contentId: string, mountedBefore: ReadonlySet<string>): boolean {
  return !mountedBefore.has(contentId);
}

/** Only the ACTIVE pane's content takes keyboard focus on mount — in a
 *  split, focusing every mounted terminal left focus on the last leaf. */
export function shouldFocusOnMount(contentId: string, activeContentId: string | null | undefined): boolean {
  return activeContentId === contentId;
}

/** Bytes to hold for a hidden terminal before we stop buffering and feed
 *  xterm anyway (never drop output — the emulator state must stay exact). */
export const HIDDEN_BUFFER_MAX_BYTES = 2 * 1024 * 1024;

/** Output queue of a terminal whose pane is not on screen. `push` returns
 *  the chunks the caller must write NOW (the cap overflowed); `drain`
 *  returns everything queued, in order, and empties the queue. */
export class HiddenOutputBuffer {
  private chunks: Uint8Array[] = [];
  private bytes = 0;

  constructor(private readonly maxBytes = HIDDEN_BUFFER_MAX_BYTES) {}

  get size(): number {
    return this.bytes;
  }

  push(chunk: Uint8Array): Uint8Array[] {
    this.chunks.push(chunk);
    this.bytes += chunk.length;
    return this.bytes > this.maxBytes ? this.drain() : [];
  }

  drain(): Uint8Array[] {
    const out = this.chunks;
    this.chunks = [];
    this.bytes = 0;
    return out;
  }
}
