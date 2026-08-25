import { describe, expect, it } from "vitest";
import {
  CHAT_MAX_WIDTH,
  CHAT_MIN_WIDTH,
  EDITOR_MIN_WIDTH,
  SIDEBAR_MIN_WIDTH,
  clampChatWidth,
  clampSidebarWidth,
  editorWidth,
  maxChatWidth,
  maxSidebarWidth,
} from "./layout-budget";

describe("layout budget", () => {
  it("keeps the sidebar at ≤ 50% of a wide window with no chat", () => {
    expect(maxSidebarWidth(1920, 0)).toBe(960);
  });

  it("800×600 with sidebar + chat open leaves the editor ≥ 320px", () => {
    // Chat is an overlay at ≤ 1000px, so its column width is 0 here.
    const sidebar = clampSidebarWidth(500, 800, 0);
    expect(sidebar).toBe(400); // 50% cap wins over the 426px budget
    expect(editorWidth(800, sidebar, 0)).toBeGreaterThanOrEqual(EDITOR_MIN_WIDTH);
  });

  it("shares the budget between sidebar and chat above the overlay breakpoint", () => {
    // 1200px window: a 260px sidebar caps the chat at 562, not 800…
    const chat = clampChatWidth(800, 1200, 260);
    expect(chat).toBe(562);
    expect(editorWidth(1200, 260, chat)).toBe(EDITOR_MIN_WIDTH);
    // …and with that chat open the sidebar cannot grow past 260.
    expect(maxSidebarWidth(1200, chat)).toBe(260);
    expect(clampSidebarWidth(400, 1200, chat)).toBe(260);
  });

  it("never returns less than the panel minimums, even in a tiny window", () => {
    expect(maxSidebarWidth(500, 400)).toBe(SIDEBAR_MIN_WIDTH);
    expect(maxChatWidth(500, 400)).toBe(CHAT_MIN_WIDTH);
    expect(clampChatWidth(100, 1920, 260)).toBe(CHAT_MIN_WIDTH);
  });

  it("caps the chat at its absolute maximum on a wide window", () => {
    expect(maxChatWidth(3840, 400)).toBe(CHAT_MAX_WIDTH);
    expect(clampChatWidth(1200, 3840, 400)).toBe(CHAT_MAX_WIDTH);
  });

  it("accounts for the zoomed activity bar width", () => {
    expect(maxSidebarWidth(800, 0, 100)).toBe(376); // 800 - 100 - 4 - 320
  });
});
