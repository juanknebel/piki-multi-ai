// Shared width budget for the three-column shell (activity bar | sidebar |
// editor | chat): the sidebar and the chat panel are user-dragged, and each
// clamp used to ignore the other, so a wide sidebar plus a wide chat could
// squeeze the editor to nothing. Both clamps (sidebar.ts, chat-panel.ts) now
// go through this module so the editor keeps EDITOR_MIN_WIDTH whatever the
// window size. layout.css enforces the same floor declaratively
// (`minmax(var(--editor-min-width), 1fr)`) and, at or below
// CHAT_OVERLAY_BREAKPOINT, takes the chat out of the grid as an overlay.
//
// The arithmetic is pure and unit-tested (layout-budget.test.ts); the DOM
// readers at the bottom are the only thing that touches the document.

/** Same number as `--editor-min-width` in variables.css. */
export const EDITOR_MIN_WIDTH = 320;
/** The two drag handles (`#sidebar-resize-v`, `#chat-resize-v`) are 4px columns. */
export const RESIZE_HANDLE_WIDTH = 4;
/** Fallback for the activity bar column (4rem at zoom 1). */
export const ACTIVITY_BAR_FALLBACK = 50;
export const SIDEBAR_MIN_WIDTH = 150;
/** The sidebar may never take more than half the window, even with no chat. */
export const SIDEBAR_MAX_FRACTION = 0.5;
export const CHAT_MIN_WIDTH = 240;
export const CHAT_MAX_WIDTH = 800;
/** Same number as the `@media (max-width: …)` in layout.css: at or below
 *  it the chat panel floats over the editor instead of taking a column. */
export const CHAT_OVERLAY_BREAKPOINT = 1000;

/**
 * Widest sidebar that still leaves the editor EDITOR_MIN_WIDTH, given the
 * chat column currently taking `chatWidth` px (0 when hidden or overlaid).
 */
export function maxSidebarWidth(windowWidth: number, chatWidth: number, activityBar = ACTIVITY_BAR_FALLBACK): number {
  const half = Math.floor(windowWidth * SIDEBAR_MAX_FRACTION);
  const chat = chatWidth > 0 ? chatWidth + RESIZE_HANDLE_WIDTH : 0;
  const budget = windowWidth - activityBar - RESIZE_HANDLE_WIDTH - chat - EDITOR_MIN_WIDTH;
  return Math.max(SIDEBAR_MIN_WIDTH, Math.min(half, budget));
}

/**
 * Widest chat panel that still leaves the editor EDITOR_MIN_WIDTH next to a
 * sidebar of `sidebarWidth` px (0 when hidden). Capped at CHAT_MAX_WIDTH.
 */
export function maxChatWidth(windowWidth: number, sidebarWidth: number, activityBar = ACTIVITY_BAR_FALLBACK): number {
  const sidebar = sidebarWidth > 0 ? sidebarWidth + RESIZE_HANDLE_WIDTH : 0;
  const budget = windowWidth - activityBar - sidebar - RESIZE_HANDLE_WIDTH - EDITOR_MIN_WIDTH;
  return Math.max(CHAT_MIN_WIDTH, Math.min(CHAT_MAX_WIDTH, budget));
}

export function clampSidebarWidth(px: number, windowWidth: number, chatWidth: number, activityBar = ACTIVITY_BAR_FALLBACK): number {
  return Math.max(SIDEBAR_MIN_WIDTH, Math.min(maxSidebarWidth(windowWidth, chatWidth, activityBar), px));
}

export function clampChatWidth(px: number, windowWidth: number, sidebarWidth: number, activityBar = ACTIVITY_BAR_FALLBACK): number {
  return Math.max(CHAT_MIN_WIDTH, Math.min(maxChatWidth(windowWidth, sidebarWidth, activityBar), px));
}

/**
 * Width the editor column gets for a given layout — what the "Done when"
 * check reasons about: `editorWidth(800, 400, 0)` must be ≥ EDITOR_MIN_WIDTH.
 */
export function editorWidth(windowWidth: number, sidebarWidth: number, chatWidth: number, activityBar = ACTIVITY_BAR_FALLBACK): number {
  const sidebar = sidebarWidth > 0 ? sidebarWidth + RESIZE_HANDLE_WIDTH : 0;
  const chat = chatWidth > 0 ? chatWidth + RESIZE_HANDLE_WIDTH : 0;
  return windowWidth - activityBar - sidebar - chat;
}

// ── DOM readers (never called from tests) ──────────────────────────────

/** Px the chat column takes right now: 0 when hidden or floating as an overlay. */
export function visibleChatWidth(): number {
  const app = document.getElementById("app");
  if (!app || !app.classList.contains("chat-visible")) return 0;
  if (window.innerWidth <= CHAT_OVERLAY_BREAKPOINT) return 0;
  return document.getElementById("chat-panel")?.offsetWidth ?? 0;
}

/** Px the sidebar column takes right now: 0 when hidden. */
export function visibleSidebarWidth(): number {
  const app = document.getElementById("app");
  if (!app || app.classList.contains("sidebar-hidden")) return 0;
  return document.getElementById("sidebar")?.offsetWidth ?? 0;
}

/** Live activity-bar column width (it is rem-sized, so it follows the zoom). */
export function activityBarWidth(): number {
  return document.getElementById("activity-bar")?.offsetWidth || ACTIVITY_BAR_FALLBACK;
}
