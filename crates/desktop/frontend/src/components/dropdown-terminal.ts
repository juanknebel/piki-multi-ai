// The global drop-down ("Quake-style") terminal: one shell, rooted at the
// user's home directory, that belongs to NO workspace. It slides up over the
// editor area from just above the status bar and is toggled from anywhere —
// the status-bar button, `Alt+\`` (fires even with the terminal focused, so
// the same key closes it), the View menu and the palette.
//
// The backend holds the PTY in `DesktopApp.scratch_terminal` (in-process,
// dies with the window). `scratch_terminal_toggle` spawns it lazily on the
// first show and afterwards only flips visibility; its bytes ride the normal
// per-tab output path, so the xterm instance is a plain `terminals` entry
// keyed by the id the toggle command returns, and `ipc.writePty` /
// `ipc.resizePty` accept that id too.

import * as ipc from "../ipc";
import { settingsStore } from "../settings";
import { icon } from "./icons";
import { reportError } from "./toast";
import {
  destroyTerminal,
  fitTerminal,
  mountTerminalInto,
  terminals,
  unmountTerminal,
} from "./terminal-panel";

const HEIGHT_KEY = "dropdownTerminalHeight";
const MIN_HEIGHT = 140;
/** Keep at least this much of the editor visible above the panel. */
const TOP_MARGIN = 80;
const DEFAULT_FRACTION = 0.4;
/** Matches `--dur-base` (0.2s) — how long the slide takes. */
const SLIDE_MS = 220;

let host: HTMLElement | null = null;
let bodyEl: HTMLElement | null = null;
let tabId: string | null = null;
let visible = false;
let hideTimer: number | undefined;

/** Create the panel DOM and wire the resize grip + the process-exit reset.
 *  Call once from `main.ts` after the terminal panel is initialised. */
export function initDropdownTerminal() {
  const area = document.getElementById("editor-area");
  if (!area) return;

  host = document.createElement("div");
  host.id = "dropdown-terminal";
  host.hidden = true;
  host.style.height = `${initialHeight()}px`;
  host.innerHTML = `
    <div class="dropdown-terminal-grip" role="separator" aria-label="Resize terminal"></div>
    <div class="dropdown-terminal-head">
      <span class="dropdown-terminal-title">${icon("terminal")}<span>Terminal · ~</span></span>
      <button type="button" class="ui-btn dropdown-terminal-close" data-variant="ghost" data-size="sm" data-icon
              title="Close" aria-label="Close terminal">${icon("close")}</button>
    </div>
    <div class="dropdown-terminal-body"></div>
  `;
  bodyEl = host.querySelector<HTMLElement>(".dropdown-terminal-body");
  host.querySelector<HTMLButtonElement>(".dropdown-terminal-close")!.addEventListener("click", () => {
    void toggleDropdownTerminal();
  });
  wireResizeGrip(host.querySelector<HTMLElement>(".dropdown-terminal-grip")!);
  area.appendChild(host);

  // If the shell exits (the user typed `exit`, or it crashed), drop the
  // instance and clear the backend slot so the next toggle spawns a fresh one.
  void ipc.onPtyExit((event) => {
    if (event.tab_id !== tabId) return;
    if (tabId) destroyTerminal(tabId);
    tabId = null;
    void ipc.scratchTerminalKill().catch(() => {});
    if (visible) hidePanel();
  });
}

/** Toggle the panel — the one entry point for every trigger. */
export async function toggleDropdownTerminal() {
  if (!host || !bodyEl) return;
  try {
    const state = await ipc.scratchTerminalToggle();
    tabId = state.tab_id;
    visible = state.visible;
    if (visible && tabId) showPanel(tabId);
    else hidePanel();
  } catch (err) {
    reportError("Toggle terminal failed", err);
  }
}

export function isDropdownTerminalOpen(): boolean {
  return visible;
}

function showPanel(id: string) {
  if (!host || !bodyEl) return;
  window.clearTimeout(hideTimer);
  host.hidden = false;
  clampHeight();
  mountTerminalInto(id, bodyEl, { focus: true });
  // Next frame so the removed `hidden` has taken effect before the transition.
  requestAnimationFrame(() => host?.classList.add("open"));
  // Fit once the slide has settled and the body has its final height.
  window.setTimeout(() => {
    const inst = terminals.get(id);
    if (inst && visible) {
      fitTerminal(inst);
      inst.terminal.focus();
    }
  }, SLIDE_MS);
}

function hidePanel() {
  if (!host) return;
  host.classList.remove("open");
  const id = tabId;
  hideTimer = window.setTimeout(() => {
    if (host && !host.classList.contains("open")) {
      host.hidden = true;
      if (id) unmountTerminal(id);
    }
  }, SLIDE_MS);
}

function initialHeight(): number {
  const saved = settingsStore.get<number>(HEIGHT_KEY);
  if (typeof saved === "number" && saved >= MIN_HEIGHT) return saved;
  return Math.round(window.innerHeight * DEFAULT_FRACTION);
}

function maxHeight(): number {
  const area = document.getElementById("editor-area");
  const areaH = area?.clientHeight ?? window.innerHeight;
  return Math.max(MIN_HEIGHT, areaH - TOP_MARGIN);
}

function clampHeight() {
  if (!host) return;
  const h = Math.min(Math.max(parseInt(host.style.height, 10) || MIN_HEIGHT, MIN_HEIGHT), maxHeight());
  host.style.height = `${h}px`;
}

/** Drag the top grip to resize; persist on release. */
function wireResizeGrip(grip: HTMLElement) {
  grip.addEventListener("mousedown", (down: MouseEvent) => {
    down.preventDefault();
    const startY = down.clientY;
    const startH = parseInt(host!.style.height, 10) || MIN_HEIGHT;
    document.body.style.cursor = "row-resize";

    const onMove = (move: MouseEvent) => {
      const next = Math.min(Math.max(startH + (startY - move.clientY), MIN_HEIGHT), maxHeight());
      host!.style.height = `${next}px`;
    };
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      const h = parseInt(host!.style.height, 10) || MIN_HEIGHT;
      settingsStore.patch(HEIGHT_KEY, h);
      if (tabId) {
        const inst = terminals.get(tabId);
        if (inst) fitTerminal(inst);
      }
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  });
}
