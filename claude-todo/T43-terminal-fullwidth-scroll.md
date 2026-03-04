# T43 — Terminal panel: dynamic width + scrollback

**Status**: DONE
**Blocked by**: —

## Problem

1. **Fixed width**: PTY is spawned with hardcoded 80×24 (`main.rs:290`: `PtySession::spawn(&ws.path, 24, 80, ...)`). Doesn't use the actual panel width, wasting space.
2. **No scroll**: The vt100 parser maintains a 1000-line scrollback buffer (`session.rs:45`) but `PseudoTerminal` only renders the visible screen. No way to see previous output.

## Solution

### Part 1: Dynamic PTY dimensions

- Calculate actual terminal panel size (minus borders) when creating workspace
- Pass real dimensions to `PtySession::spawn()` instead of 80×24
- Implement PTY resize when terminal window changes (crossterm `Resize` event)
- The `resize()` method already exists in `PtySession` but is never called

### Part 2: Scrollback with keyboard and mouse

- **Keyboard** (in normal mode, not interacting):
  - `Shift+K` / `Shift+J` or `PageUp` / `PageDown`: scroll line by line or by page
  - Similar to existing diff view scroll
- **Mouse**:
  - Enable mouse event capture in crossterm
  - `ScrollUp` / `ScrollDown` to navigate scrollback buffer
- Use `parser.screen().scrollback()` from vt100 to access history
- Add scroll offset to workspace/tab state
- Auto-scroll to bottom on new output (reset offset)
- Visual indicator when viewing history (e.g. "[+42 lines]" in status bar)

## Files to modify

- `src/main.rs` — resize on Resize event, pass real dimensions on spawn
- `src/pty/session.rs` — verify resize() works correctly
- `src/ui/terminal.rs` — render with scroll offset, access scrollback
- `src/ui/layout.rs` — expose computed panel size
- `src/app.rs` — per-tab scroll state, mouse event handling
