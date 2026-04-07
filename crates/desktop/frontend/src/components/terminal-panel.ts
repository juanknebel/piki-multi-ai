import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { SearchAddon } from "@xterm/addon-search";
import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { themeEngine } from "../theme";
import { hideKanbanPanels, showKanbanPanel } from "./kanban-panel";
import { hideApiPanels, showApiPanel } from "./api-panel";
import { hideMarkdownEditorPanels, showMarkdownEditorPanel } from "./markdown-editor-panel";

export interface TerminalInstance {
  tabId: string;
  terminal: Terminal;
  fitAddon: FitAddon;
  searchAddon: SearchAddon;
  element: HTMLDivElement;
  opened: boolean;
}

export const terminals = new Map<string, TerminalInstance>();
let mainContent: HTMLElement;

/**
 * Initialize the terminal panel. Must be awaited so event listeners
 * are registered before any PTY can be spawned.
 */
export async function initTerminalPanel(container: HTMLElement) {
  mainContent = container;

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

  // Show/hide terminals when active tab changes
  appState.on("active-tab-changed", showActiveTerminal);
  appState.on("tabs-changed", showActiveTerminal);
  appState.on("active-workspace-changed", showActiveTerminal);

  // Handle window resizes
  const resizeObserver = new ResizeObserver(() => {
    const ws = appState.activeWs;
    if (!ws) return;
    const tab = ws.tabs[ws.activeTab];
    if (!tab) return;
    const instance = terminals.get(tab.id);
    if (instance && instance.opened) fitTerminal(instance);
  });
  resizeObserver.observe(container);

  showActiveTerminal();
}

/**
 * Pre-create a Terminal instance for a tab. The xterm.js `open()` call
 * is deferred until the element is visible (in showActiveTerminal),
 * because xterm.js needs a non-zero-size container to render.
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

  mainContent.appendChild(element);
  // NOTE: terminal.open() is NOT called here — deferred until visible

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

  // Ctrl+Shift+C = copy, Ctrl+Shift+V = paste via backend system clipboard
  terminal.attachCustomKeyEventHandler((e) => {
    if (e.ctrlKey && e.shiftKey && e.type === "keydown" && e.key === "C") {
      const sel = terminal.getSelection();
      if (sel) {
        ipc.clipboardCopy(sel).catch((err) => {
          console.error("clipboard copy failed:", err);
          toast(`Copy failed: ${err}`, "error");
        });
      }
      return false;
    }
    if (e.ctrlKey && e.shiftKey && e.type === "keydown" && e.key === "V") {
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
  };
  terminals.set(tabId, instance);

  return instance;
}

function showActiveTerminal() {
  // Hide all terminals and kanban panels
  for (const instance of terminals.values()) {
    instance.element.style.display = "none";
  }
  hideKanbanPanels();
  hideApiPanels();
  hideMarkdownEditorPanels();

  // Remove welcome message if present
  mainContent.querySelector(".terminal-welcome")?.remove();

  const ws = appState.activeWs;
  if (!ws || ws.tabs.length === 0) {
    showWelcome();
    return;
  }

  const tab = ws.tabs[ws.activeTab];
  if (!tab) {
    showWelcome();
    return;
  }

  // Route non-PTY tabs to their panels
  if (tab.provider === "Kanban") {
    showKanbanPanel(tab.id);
    return;
  }
  if (tab.provider === "Api") {
    showApiPanel(tab.id, appState.activeWorkspace);
    return;
  }
  if (tab.provider === "Markdown") {
    showMarkdownEditorPanel(tab.id);
    return;
  }

  let instance = terminals.get(tab.id);
  if (!instance) {
    instance = createTerminal(tab.id);
  }

  // Make visible BEFORE opening xterm (it needs a non-zero container)
  instance.element.style.display = "block";

  // Deferred open: xterm.js needs a visible, sized container
  if (!instance.opened) {
    instance.terminal.open(instance.element);
    instance.opened = true;

    // Try WebGL addon for performance
    try {
      instance.terminal.loadAddon(new WebglAddon());
    } catch {
      // WebGL not available, software rendering is fine
    }
  }

  fitTerminal(instance);
  instance.terminal.focus();
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

function showWelcome() {
  if (mainContent.querySelector(".terminal-welcome")) return;

  const welcome = document.createElement("div");
  welcome.className = "terminal-welcome";
  welcome.innerHTML = `
    <div class="welcome-logo">PIKI</div>
    <div class="welcome-subtitle">Multi-Agent Workspace</div>
    <p>Select a workspace or create one to begin.</p>
    <div class="welcome-shortcuts">
      <div class="shortcut-item"><span class="shortcut-key">Ctrl+N</span><span class="shortcut-label">New workspace</span></div>
      <div class="shortcut-item"><span class="shortcut-key">Ctrl+P</span><span class="shortcut-label">Command palette</span></div>
      <div class="shortcut-item"><span class="shortcut-key">Ctrl+Space</span><span class="shortcut-label">Switch workspace</span></div>
      <div class="shortcut-item"><span class="shortcut-key">?</span><span class="shortcut-label">All shortcuts</span></div>
    </div>
  `;
  mainContent.appendChild(welcome);
}

export function destroyTerminal(tabId: string) {
  const instance = terminals.get(tabId);
  if (!instance) return;
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
