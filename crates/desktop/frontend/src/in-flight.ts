// One-at-a-time guard for user-triggered async operations (push, pull,
// checkout…). A double click, a palette entry and a menu item all funnel
// through `runExclusive(key, fn)`: while `fn` is pending for that key every
// other call is a no-op that resolves to `undefined`, so one intent = one
// process, whatever the entry point. Pure (no DOM) so it is unit-tested;
// `onInFlightChange` lets a panel re-render its buttons as busy/idle.

const running = new Set<string>();
const listeners = new Set<(key: string, busy: boolean) => void>();

/** Whether an operation with this key is currently pending. */
export function isInFlight(key: string): boolean {
  return running.has(key);
}

/** Subscribe to busy/idle transitions; returns the unsubscribe function. */
export function onInFlightChange(cb: (key: string, busy: boolean) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function notify(key: string, busy: boolean) {
  for (const cb of listeners) cb(key, busy);
}

/** Run `fn` unless one is already pending under `key`. Resolves to `fn`'s
 *  value, or `undefined` when skipped. A rejection releases the key and is
 *  rethrown to the caller. */
export async function runExclusive<T>(key: string, fn: () => Promise<T>): Promise<T | undefined> {
  if (running.has(key)) return undefined;
  running.add(key);
  notify(key, true);
  try {
    return await fn();
  } finally {
    running.delete(key);
    notify(key, false);
  }
}
