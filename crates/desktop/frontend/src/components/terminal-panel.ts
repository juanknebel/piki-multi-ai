import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { SearchAddon } from "@xterm/addon-search";
import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { themeEngine } from "../theme";
import { isMac, modCtrl } from "../shortcuts";

export interface TerminalInstance {
  tabId: string;
  terminal: Terminal;
  fitAddon: FitAddon;
  searchAddon: SearchAddon;
  element: HTMLDivElement;
  opened: boolean;
  resizeObserver: ResizeObserver | null;
}

export const terminals = new Map<string, TerminalInstance>();

/**
 * Initialize the terminal panel. Must be awaited so event listeners
 * are registered before any PTY can be spawned.
 *
 * Tab rendering and visibility are handled by `pane-view.ts`; this module only
 * owns PTY I/O and the per-instance xterm lifecycle.
 */
export async function initTerminalPanel(_container: HTMLElement) {
  // Await listener registration so no PTY events are missed
  await ipc.onPtyOutput((event) => {
    const instance = terminals.get(event.tab_id);
    if (!instance) return;
    const bytes = Uint8Array.from(atob(event.data), (c) => c.charCodeAt(0));
    instance.terminal.write(bytes);
  });

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

  const terminal = new Terminal({
    fontFamily: '"JetBrainsMono NF Mono", monospace',
    fontSize: 14,
    lineHeight: 1.25,
    theme: themeEngine.buildXtermTheme(),
    cursorBlink: true,
    cursorStyle: "block",
    scrollback: 5000,
    allowProposedApi: true,
  });

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);

  const searchAddon = new SearchAddon();
  terminal.loadAddon(searchAddon);

  // The element is NOT attached anywhere yet — `mountTerminalInto` attaches it
  // to the active pane's content host on demand. xterm.js's `terminal.open()`
  // is also deferred until the element is in the DOM and visible.

  // Copy on selection (auto-copy like most terminal emulators)
  terminal.onSelectionChange(() => {
    const sel = terminal.getSelection();
    if (sel) {
      ipc.clipboardCopy(sel).catch((e) => {
        console.error("clipboard copy failed:", e);
        toast(`Copy failed: ${e}`, "error");
      });
    }
  });

  // Copy/paste: Cmd+C / Cmd+V on macOS, Ctrl+Shift+C / Ctrl+Shift+V on Linux.
  // macOS terminals (iTerm2, Terminal.app) use Cmd+C without Shift.
  // Ctrl+C (without Cmd) always sends SIGINT to the terminal.
  terminal.attachCustomKeyEventHandler((e) => {
    const key = e.key.toLowerCase();
    const isCopyPaste = isMac ? modCtrl(e) : modCtrl(e) && e.shiftKey;

    if (isCopyPaste && e.type === "keydown" && key === "c") {
      const sel = terminal.getSelection();
      if (sel) {
        ipc.clipboardCopy(sel).catch((err) => {
          console.error("clipboard copy failed:", err);
          toast(`Copy failed: ${err}`, "error");
        });
      }
      return false;
    }
    if (isCopyPaste && e.type === "keydown" && key === "v") {
      ipc.clipboardPaste().then((text) => {
        if (text) terminal.paste(text);
      }).catch((err) => {
        console.error("clipboard paste failed:", err);
        toast(`Paste failed: ${err}`, "error");
      });
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

  // Block native paste events so xterm.js doesn't double-paste
  element.addEventListener("paste", (e) => {
    e.preventDefault();
    e.stopPropagation();
  }, true);

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

  const instance: TerminalInstance = {
    tabId,
    terminal,
    searchAddon,
    fitAddon,
    element,
    opened: false,
    resizeObserver: null,
  };
  terminals.set(tabId, instance);

  // Refit whenever the host element resizes — covers split-handle drags and
  // window resizes, since the terminal element is sized by its parent flex.
  instance.resizeObserver = new ResizeObserver(() => {
    if (instance.opened) fitTerminal(instance);
  });
  instance.resizeObserver.observe(element);

  return instance;
}

/**
 * Mount a terminal tab into the given host element. Creates the xterm instance
 * if needed, reparents it into `host`, opens xterm on first mount, fits the PTY,
 * and focuses. Idempotent — safe to call when already mounted.
 */
export function mountTerminalInto(tabId: string, host: HTMLElement) {
  let instance = terminals.get(tabId);
  if (!instance) {
    instance = createTerminal(tabId);
  }
  if (instance.element.parentElement !== host) {
    host.appendChild(instance.element);
  }
  instance.element.style.display = "block";

  if (!instance.opened) {
    instance.terminal.open(instance.element);
    instance.opened = true;
    try {
      instance.terminal.loadAddon(new WebglAddon());
    } catch {
      // WebGL not available, software rendering is fine
    }
  }

  // Defer the fit until the browser has laid out the new host. Calling
  // `fitAddon.fit()` synchronously right after a reparent reads stale
  // (often near-zero) dimensions and resizes the PTY to ~10 cols, which
  // shows up as text wrapping every word or two when the tab is shown
  // again.
  const inst = instance;
  requestAnimationFrame(() => {
    fitTerminal(inst);
    inst.terminal.focus();
  });
}

/** Hide a terminal tab without destroying its state. */
export function unmountTerminal(tabId: string) {
  const instance = terminals.get(tabId);
  if (!instance) return;
  instance.element.style.display = "none";
}


function fitTerminal(instance: TerminalInstance) {
  if (!instance.opened) return;
  try {
    instance.fitAddon.fit();
    const dims = instance.fitAddon.proposeDimensions();
    if (dims) {
      ipc
        .resizePty(instance.tabId, dims.rows, dims.cols)
        .catch(() => {}); // Ignore resize errors for dead PTYs
    }
  } catch {
    // Element might not be visible yet
  }
}

export function destroyTerminal(tabId: string) {
  const instance = terminals.get(tabId);
  if (!instance) return;
  instance.resizeObserver?.disconnect();
  instance.terminal.dispose();
  instance.element.remove();
  terminals.delete(tabId);
}

/** Open a search bar for the active terminal */
export function openTerminalSearch() {
  const ws = appState.activeWs;
  if (!ws || ws.tabs.length === 0) return;
  const tab = ws.tabs[ws.activeTab];
  if (!tab) return;
  const instance = terminals.get(tab.id);
  if (!instance || !instance.opened) return;

  // Remove existing search bar
  instance.element.querySelector(".term-search-bar")?.remove();

  const bar = document.createElement("div");
  bar.className = "term-search-bar";
  bar.innerHTML = `
    <input class="term-search-input" type="text" placeholder="Search..." autofocus />
    <button class="term-search-btn" id="ts-prev" title="Previous">↑</button>
    <button class="term-search-btn" id="ts-next" title="Next">↓</button>
    <button class="term-search-btn" id="ts-close" title="Close">×</button>
  `;
  instance.element.prepend(bar);

  const input = bar.querySelector<HTMLInputElement>(".term-search-input")!;

  input.addEventListener("input", () => {
    instance.searchAddon.findNext(input.value, { regex: false, caseSensitive: false });
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) {
        instance.searchAddon.findPrevious(input.value);
      } else {
        instance.searchAddon.findNext(input.value);
      }
    }
    if (e.key === "Escape") {
      bar.remove();
      instance.searchAddon.clearDecorations();
      instance.terminal.focus();
    }
  });

  bar.querySelector("#ts-next")!.addEventListener("click", () => {
    instance.searchAddon.findNext(input.value);
  });
  bar.querySelector("#ts-prev")!.addEventListener("click", () => {
    instance.searchAddon.findPrevious(input.value);
  });
  bar.querySelector("#ts-close")!.addEventListener("click", () => {
    bar.remove();
    instance.searchAddon.clearDecorations();
    instance.terminal.focus();
  });

  input.focus();
}
