import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { appState } from "../state";
import * as ipc from "../ipc";
import { toast, reportError } from "./toast";
import { cssToken, themeEngine } from "../theme";
import { getShortcutKey, isMac, modCtrl } from "../shortcuts";
import { getTerminalSettings, xtermOptionsFor } from "../terminal-settings";
import { createSelectionCopier, type SelectionCopier } from "../copy-on-select";
import { armLiteralNext, disarmLiteralNext, isLiteralNextArmed, isLiteralPass, literalNextTab } from "../literal-next";
import { openContextMenu, type CtxItem } from "./context-menu";
import { icon } from "./icons";
import { allLeaves } from "../pane-tree";
import { flashTabChip } from "./tab-bar";
import { decodeBase64Bytes } from "../pty-frame";
import { HiddenOutputBuffer, shouldResync } from "../mount-policy";
import { perfCount } from "../perf-counters";
import type { MountOptions } from "../tab-mount";

/** Per-tab search bar state, remembered across open/close. */
interface SearchState {
  query: string;
  regex: boolean;
  caseSensitive: boolean;
}

export interface TerminalInstance {
  tabId: string;
  terminal: Terminal;
  fitAddon: FitAddon;
  searchAddon: SearchAddon;
  element: HTMLDivElement;
  opened: boolean;
  resizeObserver: ResizeObserver | null;
  /** One clipboard write per selection gesture (copy-on-select.ts). */
  copier: SelectionCopier;
  /** Link under the pointer (web-links addon hover), for the context menu. */
  hoveredLink: string | null;
  search: SearchState;
  /** The element is in a visible pane. While false, output is queued in
   *  `hidden` (xterm's parser is not fed) and no fit/resize runs. */
  visible: boolean;
  hidden: HiddenOutputBuffer;
  /** Last `rows×cols` sent to the PTY — identical proposals are skipped. */
  lastSent: { rows: number; cols: number } | null;
  /** Latest proposal waiting for the next frame's single `resizePty`. */
  pendingResize: { rows: number; cols: number } | null;
}

export const terminals = new Map<string, TerminalInstance>();

/** Contents that already mounted once — a daemon restore is fetched only
 *  the first time (`shouldResync`). Cleared by `destroyTerminal`. */
const mountedOnce = new Set<string>();

/** One place for every byte that reaches a terminal, whichever transport
 *  delivered it: feed xterm when the pane is on screen, queue otherwise. */
function deliverOutput(tabId: string, bytes: Uint8Array) {
  const instance = terminals.get(tabId);
  if (!instance) return;
  perfCount("pty.batch");
  perfCount("pty.bytes", bytes.length);
  if (instance.visible) {
    instance.terminal.write(bytes);
    return;
  }
  perfCount("pty.buffered");
  for (const chunk of instance.hidden.push(bytes)) instance.terminal.write(chunk);
}

/** Feed xterm everything that arrived while the pane was hidden. */
function flushHidden(instance: TerminalInstance) {
  const queued = instance.hidden.drain();
  if (queued.length === 0) return;
  perfCount("pty.flushHidden");
  for (const chunk of queued) instance.terminal.write(chunk);
}

/** Middle-click paste: how long after `auxclick` we wait for the platform's
 *  own primary-selection paste before falling back to the clipboard. The
 *  native paste is dispatched synchronously with the click, so this only
 *  has to outlast the same task. */
const MIDDLE_PASTE_GRACE_MS = 80;

/**
 * Initialize the terminal panel. Must be awaited so event listeners
 * are registered before any PTY can be spawned.
 *
 * Tab rendering and visibility are handled by `pane-view.ts`; this module only
 * owns PTY I/O and the per-instance xterm lifecycle.
 */
export async function initTerminalPanel(_container: HTMLElement) {
  // Await listener registration so no PTY events are missed. The raw
  // channel is the transport (binary batches, no base64); the JSON event
  // only carries what the backend could not put on the channel.
  await ipc.onPtyOutput((event) => {
    perfCount("pty.fallbackEvent");
    deliverOutput(event.tab_id, decodeBase64Bytes(event.data));
  });
  try {
    await ipc.registerPtyOutputChannel((frame) => deliverOutput(frame.tabId, frame.data));
  } catch (err) {
    // Older backend without the channel command: the event path still works.
    console.error("PTY output channel unavailable, using events:", err);
  }

  await ipc.onPtyExit((event) => {
    const instance = terminals.get(event.tab_id);
    if (!instance) return;
    appState.markTabDead(event.tab_id);
    const code = event.exit_code ?? 0;
    instance.terminal.writeln(
      `\r\n\x1b[90m[Process exited with code ${code}]\x1b[0m`,
    );
  });
}

/** The terminal instance in the active pane, if the pane holds one. */
export function activeTerminalInstance(): TerminalInstance | undefined {
  const wt = appState.activeTabTree;
  if (!wt) return undefined;
  const leaf = allLeaves(wt.paneTree).find((l) => l.id === wt.activePaneId);
  const id = leaf?.contentId;
  return id ? terminals.get(id) : undefined;
}

// ── Clipboard helpers ──────────────────────────

/** Copy the terminal's selection. `explicit` (menu / Ctrl+Shift+C) gets a
 *  toast; copy-on-select stays silent — 40 rows of drag is not 40 toasts,
 *  and not even one. */
function copySelection(instance: TerminalInstance, explicit: boolean) {
  const sel = instance.terminal.getSelection();
  if (!sel) return;
  ipc.clipboardCopy(sel)
    .then(() => {
      if (explicit) toast("Copied to clipboard", "success");
    })
    .catch((err) => reportError("Clipboard copy failed", err));
}

function pasteFromClipboard(terminal: Terminal) {
  ipc.clipboardPaste()
    .then((text) => {
      if (text) terminal.paste(text);
    })
    .catch((err) => reportError("Clipboard paste failed", err));
}

function openLink(uri: string) {
  ipc.openExternalUrl(uri).catch((err) => reportError("Open link failed", err));
}

/** Resolves once the bundled terminal face (`--font-mono`, a ~1 MB WOFF2)
 *  is loaded — or right away when the Font Loading API is missing, the load
 *  fails, or it takes longer than FONT_WAIT_MS. `mountTerminalInto` waits on
 *  it before `terminal.open()`: xterm measures its cell grid ONCE at open
 *  (and again only on a font-option change), so opening on the fallback
 *  mono would leave every cell the wrong width after the swap. Lazy (not at
 *  module init) so the stylesheet that defines the token is applied. */
const FONT_WAIT_MS = 2000;
let fontsLoaded = false;
let fontsReady: Promise<void> | null = null;
function ensureFontsReady(): Promise<void> {
  if (!fontsReady) {
    fontsReady = (async () => {
      try {
        if (typeof document === "undefined" || !("fonts" in document)) return;
        const family = cssToken("--font-mono", "monospace");
        const load = Promise.all([document.fonts.load(`1em ${family}`), document.fonts.load(`bold 1em ${family}`)]);
        await Promise.race([load, new Promise((resolve) => setTimeout(resolve, FONT_WAIT_MS))]);
      } catch {
        // A broken font must never block the terminal; xterm falls back.
      } finally {
        fontsLoaded = true;
      }
    })();
  }
  return fontsReady;
}
/** Tabs whose first open is parked on `ensureFontsReady()`. */
const pendingOpen = new Set<string>();

/**
 * Pre-create a Terminal instance for a tab. The xterm.js `open()` call
 * is deferred until the element is visible (in mountTerminalInto),
 * because xterm.js needs a non-zero-size container to render.
 *
 * The element starts detached from the DOM. Calling
 * `mountTerminalInto(tabId, host)` later attaches it into `host` and runs
 * post-mount work (including the deferred `terminal.open(host)`).
 */
export function createTerminal(tabId: string): TerminalInstance {
  const element = document.createElement("div");
  element.className = "terminal-container";
  element.style.display = "none";

  const settings = getTerminalSettings();
  const terminal = new Terminal({
    ...xtermOptionsFor(settings, Number(cssToken("--ui-zoom", "1")), cssToken("--font-mono", "monospace")),
    theme: themeEngine.buildXtermTheme(),
    allowProposedApi: true,
  });

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);

  const searchAddon = new SearchAddon();
  terminal.loadAddon(searchAddon);

  // Unicode 11 widths: emoji and other wide glyphs take two cells, so a box
  // drawn around them (Claude's status lines) stays aligned.
  terminal.loadAddon(new Unicode11Addon());
  terminal.unicode.activeVersion = "11";

  const instance: TerminalInstance = {
    tabId,
    terminal,
    searchAddon,
    fitAddon,
    element,
    opened: false,
    resizeObserver: null,
    copier: createSelectionCopier(),
    hoveredLink: null,
    search: { query: "", regex: false, caseSensitive: false },
    visible: false,
    hidden: new HiddenOutputBuffer(),
    lastSent: null,
    pendingResize: null,
  };

  // URLs: Ctrl+click (Cmd on macOS) opens in the browser via the backend —
  // never `window.open` inside the webview. Hover is tracked for the
  // context menu's "Open link".
  terminal.loadAddon(
    new WebLinksAddon(
      (event, uri) => {
        if (isMac ? event.metaKey : event.ctrlKey) openLink(uri);
      },
      {
        hover: (_event, text) => {
          instance.hoveredLink = text;
        },
        leave: () => {
          instance.hoveredLink = null;
        },
      },
    ),
  );

  // The element is NOT attached anywhere yet — `mountTerminalInto` attaches it
  // to the active pane's content host on demand. xterm.js's `terminal.open()`
  // is also deferred until the element is in the DOM and visible.

  // Copy on selection: xterm fires onSelectionChange per pointer move; the
  // copier marks the gesture dirty and the mouseup flushes it ONCE, a tick
  // later (xterm's own document-level mouseup fires after ours). The mouseup
  // is watched on the document so a drag released outside the terminal —
  // past its edge, over the sidebar — still ends the gesture.
  terminal.onSelectionChange(() => {
    instance.copier.selectionChanged();
  });
  const onGestureEnd = (e: MouseEvent) => {
    if (e.button !== 0) return;
    if (!instance.copier.mouseUp()) return;
    setTimeout(() => {
      if (!terminals.has(tabId)) return;
      const text = instance.copier.flush(terminal.getSelection());
      if (text && getTerminalSettings().copyOnSelect) {
        ipc.clipboardCopy(text).catch((err) => reportError("Clipboard copy failed", err));
      }
    }, 0);
  };
  element.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    document.addEventListener("mouseup", onGestureEnd, { once: true, capture: true });
  });

  // Bell → flash the tab chip. Title (OSC 0/2) → tab-label fallback that
  // never overrides a user rename (`getTabLabel`).
  terminal.onBell(() => flashTabChip(tabId));
  terminal.onTitleChange((title) => appState.applyTerminalTitle(tabId, title));

  // Copy/paste: Cmd+C / Cmd+V on macOS, Ctrl+Shift+C / Ctrl+Shift+V on Linux.
  // macOS terminals (iTerm2, Terminal.app) use Cmd+C without Shift.
  // Ctrl+C (without Cmd) always sends SIGINT to the terminal.
  terminal.attachCustomKeyEventHandler((e) => {
    // The one keydown "Send next key to terminal" let through: no
    // interception at all, xterm maps it to bytes as any terminal would.
    if (isLiteralPass(e)) return true;

    const key = e.key.toLowerCase();
    const isCopyPaste = isMac ? modCtrl(e) : modCtrl(e) && e.shiftKey;

    if (isCopyPaste && e.type === "keydown" && key === "c") {
      // preventDefault keeps WebKit from also firing its own copy/paste;
      // the native `paste` event below is then only ever a middle-click or
      // Shift+Insert.
      e.preventDefault();
      copySelection(instance, true);
      return false;
    }
    if (isCopyPaste && e.type === "keydown" && key === "v") {
      e.preventDefault();
      pasteFromClipboard(terminal);
      return false;
    }
    // Shift+PageUp/Down/Home/End for scrollback navigation
    if (e.shiftKey && e.type === "keydown") {
      if (e.key === "PageUp") { terminal.scrollPages(-1); return false; }
      if (e.key === "PageDown") { terminal.scrollPages(1); return false; }
      if (e.key === "Home") { terminal.scrollToTop(); return false; }
      if (e.key === "End") { terminal.scrollToBottom(); return false; }
    }
    return true;
  });

  // Middle-click paste. WebKitGTK pastes the X11/Wayland *primary* selection
  // into the focused editable on a middle click — a native `paste` event whose
  // clipboardData is the selection; xterm moves its textarea under the pointer
  // on `auxclick` so that paste lands on the terminal. We take that event over
  // (xterm's own paste path would double up) and, when the platform delivers
  // none — a Wayland session without the primary-selection protocol, macOS,
  // Windows — fall back to the clipboard via the plugin, so middle-click
  // always pastes something. While a program tracks the mouse the click is
  // its own (tmux, Claude's TUI) and nothing is pasted.
  let middlePaste: { nativeSeen: boolean } | null = null;
  element.addEventListener("mousedown", (e) => {
    if (e.button !== 1 || terminal.modes.mouseTrackingMode !== "none") return;
    middlePaste = { nativeSeen: false };
  });
  element.addEventListener("auxclick", (e) => {
    if (e.button !== 1) return;
    if (terminal.modes.mouseTrackingMode !== "none") return;
    e.preventDefault();
    const gesture = middlePaste ?? { nativeSeen: false };
    middlePaste = gesture;
    setTimeout(() => {
      if (middlePaste === gesture) middlePaste = null;
      if (!gesture.nativeSeen && terminals.has(tabId)) pasteFromClipboard(terminal);
    }, MIDDLE_PASTE_GRACE_MS);
  });
  element.addEventListener("paste", (e) => {
    e.preventDefault();
    e.stopPropagation();
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (middlePaste) middlePaste.nativeSeen = true;
    if (text) terminal.paste(text);
  }, true);

  // Right-click: the terminal's own menu (the global contextmenu blocker in
  // main.ts only stops the webview's native one).
  element.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    openContextMenu(e.clientX, e.clientY, terminalMenuItems(instance));
  });

  // Mouse-wheel scrollback fix. When a program enables mouse tracking (Claude
  // Code, tmux, etc.), xterm forwards wheel events to it as mouse reports
  // instead of moving the scrollback — so the wheel appears dead and the user
  // is forced onto the arrow keys, and the forwarded events make the program
  // redraw from the top (the viewport snapping "all the way up"). On the normal
  // buffer there's real scrollback the wheel should move, so we scroll it here
  // and swallow the event. The alternate buffer (vim, less, fzf) keeps xterm's
  // default alt-scroll behavior so those apps still get their wheel events.
  element.addEventListener("wheel", (e) => {
    if (terminal.buffer.active.type !== "normal") return;
    if (terminal.modes.mouseTrackingMode === "none") return;
    const rows = terminal.rows || 1;
    const rowHeight = element.clientHeight > 0 ? element.clientHeight / rows : 17;
    let lines: number;
    if (e.deltaMode === WheelEvent.DOM_DELTA_LINE) lines = e.deltaY;
    else if (e.deltaMode === WheelEvent.DOM_DELTA_PAGE) lines = e.deltaY * rows;
    else lines = e.deltaY / rowHeight;
    terminal.scrollLines(Math.round(lines) || (e.deltaY > 0 ? 1 : -1));
    e.preventDefault();
    e.stopImmediatePropagation();
  }, { capture: true });

  // Send keystrokes to backend (UTF-8 safe base64 encoding)
  terminal.onData((data) => {
    const bytes = new TextEncoder().encode(data);
    let binary = "";
    for (const b of bytes) binary += String.fromCharCode(b);
    const encoded = btoa(binary);
    ipc.writePty(tabId, encoded).catch((err) =>
      console.error("PTY write error:", err),
    );
  });

  terminals.set(tabId, instance);

  // Refit whenever the host element resizes — covers split-handle drags and
  // window resizes, since the terminal element is sized by its parent flex.
  instance.resizeObserver = new ResizeObserver(() => {
    if (instance.opened) fitTerminal(instance);
  });
  instance.resizeObserver.observe(element);

  return instance;
}

/** Items of the terminal's right-click menu. */
function terminalMenuItems(instance: TerminalInstance): CtxItem[] {
  const { terminal } = instance;
  const link = instance.hoveredLink;
  const items: CtxItem[] = [];
  if (link) {
    items.push(
      { label: "Open Link", action: () => openLink(link) },
      {
        label: "Copy Link",
        action: () => {
          ipc.clipboardCopy(link)
            .then(() => toast("Link copied", "success"))
            .catch((err) => reportError("Clipboard copy failed", err));
        },
      },
      { separator: true },
    );
  }
  items.push(
    { label: "Copy", disabled: !terminal.hasSelection(), action: () => copySelection(instance, true) },
    { label: "Paste", action: () => pasteFromClipboard(terminal) },
    {
      label: "Select All",
      action: () => {
        terminal.selectAll();
        // No mouse gesture ends a programmatic selection: honour copy-on-select here.
        if (getTerminalSettings().copyOnSelect) copySelection(instance, false);
      },
    },
    { separator: true },
    { label: "Clear", action: () => terminal.clear() },
    { label: "Search…", action: () => openTerminalSearch(instance.tabId) },
    { separator: true },
    {
      label: `Send Next Key to Terminal (${getShortcutKey("literal-next")})`,
      action: () => {
        armLiteralNext(instance.tabId);
        terminal.focus();
      },
    },
  );
  return items;
}

/**
 * Mount a terminal tab into the given host element. Creates the xterm instance
 * if needed, reparents it into `host`, opens xterm on first mount, replays
 * output queued while hidden, fits the PTY, focuses when `opts.focus` (the
 * active pane) and fetches the daemon restore on the FIRST mount only.
 * Idempotent — a re-mount into the same host is a no-op apart from the fit.
 */
export function mountTerminalInto(tabId: string, host: HTMLElement, opts: MountOptions = {}) {
  let instance = terminals.get(tabId);
  if (!instance) {
    instance = createTerminal(tabId);
  }
  perfCount("terminal.mount");
  if (instance.element.parentElement !== host) {
    host.appendChild(instance.element);
  }
  instance.element.style.display = "block";
  instance.visible = true;
  flushHidden(instance);

  if (!instance.opened) {
    // First open waits for the Nerd Font (see ensureFontsReady); the mount
    // is replayed once it lands, if the tab still wants this host.
    if (!fontsLoaded) {
      if (!pendingOpen.has(tabId)) {
        pendingOpen.add(tabId);
        void ensureFontsReady().then(() => {
          pendingOpen.delete(tabId);
          const still = terminals.get(tabId);
          if (still === instance && instance.element.parentElement === host && instance.visible) {
            mountTerminalInto(tabId, host, opts);
          }
        });
      }
      return;
    }
    instance.terminal.open(instance.element);
    instance.opened = true;
    try {
      instance.terminal.loadAddon(new WebglAddon());
    } catch {
      // WebGL not available, software rendering is fine
    }
    // Losing focus while "send next key" is armed for this terminal would
    // hand the raw key to whatever gets focus instead — disarm.
    instance.terminal.textarea?.addEventListener("blur", () => {
      if (literalNextTab() === tabId) disarmLiteralNext();
    });
  }

  // Defer the fit until the browser has laid out the new host. Calling
  // `fitAddon.fit()` synchronously right after a reparent reads stale
  // (often near-zero) dimensions and resizes the PTY to ~10 cols, which
  // shows up as text wrapping every word or two when the tab is shown
  // again.
  const inst = instance;
  const focus = opts.focus === true;
  const resync = shouldResync(tabId, mountedOnce);
  mountedOnce.add(tabId);
  requestAnimationFrame(() => {
    if (!inst.visible) return; // hidden again before the frame
    fitTerminal(inst);
    if (focus) inst.terminal.focus();
    // Re-fetch the restore buffer for a persistent (daemon-backed) tab now
    // that the xterm instance exists and is sized — its restore may have been
    // emitted before this terminal was created (a no-op for local tabs).
    // First mount only: xterm keeps its own state across re-mounts.
    if (resync) {
      perfCount("terminal.resync");
      ipc.resyncPty(inst.tabId).catch((err) => console.error("PTY resync failed:", err));
    }
  });
}

/** Hide a terminal tab without destroying its state. Output arriving while
 *  hidden is queued (`HiddenOutputBuffer`) and replayed on the next mount. */
export function unmountTerminal(tabId: string) {
  const instance = terminals.get(tabId);
  if (!instance) return;
  instance.element.style.display = "none";
  instance.visible = false;
  instance.pendingResize = null;
}


export function fitTerminal(instance: TerminalInstance) {
  if (!instance.opened || !instance.visible) return;
  // Skip when the element is hidden or detached — its clientWidth is 0 so
  // `fitAddon.fit()` would shrink the PTY to its minimum cols (~2). The
  // ResizeObserver fires on display:none transitions, so without this guard
  // every tab switch resizes the inactive PTY down to nothing, and when the
  // tab is shown again Claude's already-rendered output wraps at ~2 cols.
  if (instance.element.offsetWidth === 0 || instance.element.offsetHeight === 0) {
    return;
  }
  try {
    instance.fitAddon.fit();
    const dims = instance.fitAddon.proposeDimensions();
    if (dims) scheduleResizePty(instance, dims.rows, dims.cols);
  } catch {
    // Element might not be visible yet
  }
}

/** Throttle of the PTY resize IPC: at most ONE `resizePty` per instance per
 *  animation frame (the trailing proposal wins), and none at all when the
 *  grid did not change. A divider drag fires the ResizeObserver on every
 *  frame for every pane it touches; xterm is refitted locally each time
 *  (cheap, keeps the text laid out) but the PTY only hears the last size. */
let resizeFrame: number | null = null;
function scheduleResizePty(instance: TerminalInstance, rows: number, cols: number) {
  if (instance.lastSent && instance.lastSent.rows === rows && instance.lastSent.cols === cols) {
    instance.pendingResize = null;
    return;
  }
  instance.pendingResize = { rows, cols };
  if (resizeFrame !== null) return;
  resizeFrame = requestAnimationFrame(() => {
    resizeFrame = null;
    flushPendingResizes();
  });
}

/** Send every queued PTY resize now — the exact final size on a divider
 *  mouseup (`pane-view.ts`), so the drag's trailing frame is never lost. */
export function flushPendingResizes() {
  if (resizeFrame !== null) {
    cancelAnimationFrame(resizeFrame);
    resizeFrame = null;
  }
  for (const instance of terminals.values()) {
    const dims = instance.pendingResize;
    if (!dims || !instance.visible) continue;
    instance.pendingResize = null;
    instance.lastSent = dims;
    perfCount("terminal.resizePty");
    ipc.resizePty(instance.tabId, dims.rows, dims.cols).catch(() => {}); // Ignore resize errors for dead PTYs
  }
}

export function destroyTerminal(tabId: string) {
  const instance = terminals.get(tabId);
  if (!instance) return;
  if (literalNextTab() === tabId) disarmLiteralNext();
  instance.resizeObserver?.disconnect();
  instance.terminal.dispose();
  instance.element.remove();
  terminals.delete(tabId);
  mountedOnce.delete(tabId);
}

// ── Actions (shortcuts, palette, menu bar) ─────

/** Arm "send next key to terminal" for the active pane's terminal; pressing
 *  the chord again disarms. */
export function toggleLiteralNext() {
  if (isLiteralNextArmed()) {
    disarmLiteralNext();
    return;
  }
  const instance = activeTerminalInstance();
  if (!instance || !instance.opened) {
    toast("The active pane is not a terminal", "info");
    return;
  }
  armLiteralNext(instance.tabId);
  instance.terminal.focus();
}

/** Clear the active terminal's screen and scrollback (like `clear`, but
 *  without typing into a running program). */
export function clearActiveTerminal() {
  const instance = activeTerminalInstance();
  if (!instance || !instance.opened) {
    toast("The active pane is not a terminal", "info");
    return;
  }
  instance.terminal.clear();
}

// ── Search bar ─────────────────────────────────

/** Open the search bar of a terminal (the active pane's by default). */
export function openTerminalSearch(tabId?: string) {
  const instance = tabId ? terminals.get(tabId) : activeTerminalInstance();
  if (!instance || !instance.opened) return;

  // Already open: just refocus the query.
  const existing = instance.element.querySelector<HTMLInputElement>(".term-search-bar .term-search-input");
  if (existing) {
    existing.focus();
    existing.select();
    return;
  }

  const { terminal, searchAddon, search } = instance;

  const bar = document.createElement("div");
  bar.className = "term-search-bar";
  bar.setAttribute("role", "search");
  bar.innerHTML = `
    <input data-size="sm" class="term-search-input ui-input" type="text" placeholder="Search…" aria-label="Search in terminal" />
    <span class="term-search-count" aria-live="polite"></span>
    <button type="button" data-variant="ghost" data-size="sm" class="term-search-toggle ui-btn" data-opt="caseSensitive" aria-pressed="${search.caseSensitive}" title="Match case">Aa</button>
    <button type="button" data-variant="ghost" data-size="sm" class="term-search-toggle ui-btn" data-opt="regex" aria-pressed="${search.regex}" title="Regular expression">.*</button>
    <button type="button" data-variant="ghost" data-icon class="term-search-btn ui-btn" data-act="prev" title="Previous match (Shift+Enter)" aria-label="Previous match">${icon("arrow-up")}</button>
    <button type="button" data-variant="ghost" data-icon class="term-search-btn ui-btn" data-act="next" title="Next match (Enter)" aria-label="Next match">${icon("arrow-down")}</button>
    <button type="button" data-variant="ghost" data-icon class="term-search-btn ui-btn" data-act="close" title="Close (Esc)">×</button>
  `;
  // Anchored to the terminal container (position: absolute, top-right) — it
  // floats over the first row and never moves with the viewport.
  instance.element.prepend(bar);

  const input = bar.querySelector<HTMLInputElement>(".term-search-input")!;
  const count = bar.querySelector<HTMLElement>(".term-search-count")!;
  input.value = search.query;

  const resultsSub = searchAddon.onDidChangeResults(({ resultIndex, resultCount }) => {
    if (!input.value) {
      count.textContent = "";
      count.classList.remove("term-search-count--none");
      return;
    }
    if (resultCount === 0) {
      count.textContent = "No results";
      count.classList.add("term-search-count--none");
      return;
    }
    count.classList.remove("term-search-count--none");
    // -1 = more matches than the addon decorates; the count is still exact.
    count.textContent = resultIndex < 0 ? `${resultCount}+` : `${resultIndex + 1}/${resultCount}`;
  });

  const options = (incremental: boolean) => ({
    regex: search.regex,
    caseSensitive: search.caseSensitive,
    incremental,
    decorations: themeEngine.searchDecorations(),
  });

  const run = (dir: "next" | "prev", incremental = false) => {
    search.query = input.value;
    if (!search.query) {
      searchAddon.clearDecorations();
      count.textContent = "";
      count.classList.remove("term-search-count--none");
      return;
    }
    if (dir === "next") searchAddon.findNext(search.query, options(incremental));
    else searchAddon.findPrevious(search.query, options(incremental));
  };

  const close = () => {
    resultsSub.dispose();
    bar.remove();
    searchAddon.clearDecorations();
    terminal.focus();
  };

  input.addEventListener("input", () => run("next", true));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      run(e.shiftKey ? "prev" : "next");
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  });

  bar.querySelectorAll<HTMLButtonElement>(".term-search-toggle").forEach((btn) => {
    btn.addEventListener("click", () => {
      const opt = btn.dataset.opt as "regex" | "caseSensitive";
      search[opt] = !search[opt];
      btn.setAttribute("aria-pressed", String(search[opt]));
      run("next", true);
      input.focus();
    });
  });
  bar.querySelector('[data-act="next"]')!.addEventListener("click", () => run("next"));
  bar.querySelector('[data-act="prev"]')!.addEventListener("click", () => run("prev"));
  bar.querySelector('[data-act="close"]')!.addEventListener("click", close);

  input.focus();
  if (search.query) {
    input.select();
    run("next", true);
  }
}
