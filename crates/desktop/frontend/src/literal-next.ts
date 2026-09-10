// "Send next key to the terminal" — the one-shot chord that lets a user type
// a combo the app would otherwise capture (Alt+B is readline's backward-word
// but also Switch Branch; Ctrl+Shift+F is Search in Project…). Pure state,
// vitest-covered; the wiring lives in shortcuts.ts (`handleGlobalKeydown`
// steps aside while armed), terminal-panel.ts (xterm's custom key handler
// lets the marked event through its copy/paste interception) and
// pane-view.ts (the hint in the pane header).
//
// Lifecycle: `arm(tabId)` → the next real keydown is `consume`d — the app
// dispatcher skips it and xterm turns it into bytes as it would in any
// terminal — or `Escape` cancels. Modifier-only presses (holding Ctrl on
// the way to Ctrl+Shift+F) keep it armed.

export type LiteralNextVerdict = "modifier" | "cancel" | "pass";

type Listener = (armedTab: string | null) => void;

let armedTab: string | null = null;
let passEvent: unknown = null;
const listeners = new Set<Listener>();

function notify() {
  for (const l of listeners) l(armedTab);
}

/** Arm for `tabId` — the terminal that should receive the raw key. */
export function armLiteralNext(tabId: string): void {
  if (armedTab === tabId) return;
  armedTab = tabId;
  notify();
}

export function disarmLiteralNext(): void {
  if (armedTab === null) return;
  armedTab = null;
  notify();
}

export function isLiteralNextArmed(): boolean {
  return armedTab !== null;
}

/** The tab the raw key is meant for, or null. */
export function literalNextTab(): string | null {
  return armedTab;
}

/** Subscribe to arm/disarm (the pane-header hint). Returns the unsubscribe. */
export function onLiteralNextChange(cb: Listener): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock"]);

/** What a keydown means while armed. Does not change state. */
export function literalNextVerdict(e: { key: string }): LiteralNextVerdict {
  if (MODIFIER_KEYS.has(e.key)) return "modifier";
  if (e.key === "Escape") return "cancel";
  return "pass";
}

/** Disarm and mark `e` as THE event that must reach the terminal raw. The
 *  dispatcher calls this before letting the keydown continue; the terminal's
 *  key handler recognises the same event object via `isLiteralPass`. */
export function consumeLiteralNext(e: unknown): string | null {
  const tab = armedTab;
  passEvent = e;
  disarmLiteralNext();
  return tab;
}

/** True for the one keydown `consumeLiteralNext` let through. */
export function isLiteralPass(e: unknown): boolean {
  return passEvent !== null && e === passEvent;
}

/** Test hook / hard reset. */
export function resetLiteralNext(): void {
  passEvent = null;
  disarmLiteralNext();
  listeners.clear();
}
