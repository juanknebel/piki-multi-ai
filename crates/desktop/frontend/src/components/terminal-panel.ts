import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { appState } from "../state";
import * as ipc from "../ipc";

interface TerminalInstance {
  tabId: string;
  terminal: Terminal;
  fitAddon: FitAddon;
  element: HTMLDivElement;
  opened: boolean;
}

const terminals = new Map<string, TerminalInstance>();
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
    lineHeight: 1.2,
    theme: {
      background: "#1e1e1e",
      foreground: "#cccccc",
      cursor: "#ffffff",
      selectionBackground: "rgba(38, 79, 120, 0.5)",
      black: "#000000",
      red: "#cd3131",
      green: "#0dbc79",
      yellow: "#e5e510",
      blue: "#2472c8",
      magenta: "#bc3fbc",
      cyan: "#11a8cd",
      white: "#e5e5e5",
      brightBlack: "#666666",
      brightRed: "#f14c4c",
      brightGreen: "#23d18b",
      brightYellow: "#f5f543",
      brightBlue: "#3b8eea",
      brightMagenta: "#d670d6",
      brightCyan: "#29b8db",
      brightWhite: "#e5e5e5",
    },
    cursorBlink: true,
    scrollback: 5000,
    allowProposedApi: true,
  });

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);

  mainContent.appendChild(element);
  // NOTE: terminal.open() is NOT called here — deferred until visible

  // Send keystrokes to backend
  terminal.onData((data) => {
    const encoded = btoa(data);
    ipc.writePty(tabId, encoded).catch((err) =>
      console.error("PTY write error:", err),
    );
  });

  const instance: TerminalInstance = {
    tabId,
    terminal,
    fitAddon,
    element,
    opened: false,
  };
  terminals.set(tabId, instance);

  return instance;
}

function showActiveTerminal() {
  // Hide all terminals
  for (const instance of terminals.values()) {
    instance.element.style.display = "none";
  }

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
    <h2>Piki Desktop</h2>
    <p>Select a workspace from the sidebar to get started, or create a new one.</p>
    <p>Use <kbd>+</kbd> in the tab bar to open a new terminal session.</p>
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
