import { settingsStore } from "./settings";

/** True when running on macOS — Cmd (Meta) replaces Ctrl/Alt. */
export const isMac: boolean =
  typeof navigator !== "undefined" && /Mac|iPod|iPhone|iPad/.test(navigator.platform);

/** Platform-aware check: returns true if Ctrl (or Cmd on macOS) is held. */
export function modCtrl(e: KeyboardEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

/** Platform-aware check: returns true if Alt (or Cmd on macOS) is held.
 *  macOS Option key sends special characters, so Cmd is used instead. */
export function modAlt(e: KeyboardEvent): boolean {
  return isMac ? e.metaKey : e.altKey;
}

/** Format a shortcut string for the current platform.
 *  Converts "Ctrl+X" → "⌘+X" and "Alt+X" → "⌘+X" on macOS. */
export function formatShortcut(combo: string): string {
  if (!isMac) return combo;
  return combo.replace(/\bCtrl\b/g, "⌘").replace(/\bAlt\b/g, "⌘");
}

/** Combos that can never be a terminal keystroke: xterm.js turns plain
 *  `Ctrl+<letter>` into a control byte (Ctrl+B → tmux prefix, Ctrl+P → shell
 *  history) and `Ctrl+Space` into NUL, so only Alt-, Ctrl+Shift- and
 *  Ctrl+Alt-chords are allowed to capture while a terminal has focus. The
 *  type restricts the *defaults*; `isTerminalSafeCombo()` applies the same
 *  rule at runtime to whatever the user rebinds. */
export type TerminalSafeCombo =
  | `Alt+${string}`
  | `Ctrl+Shift+${string}`
  | `Ctrl+Alt+${string}`;

interface ShortcutBase {
  id: string;
  label: string;
  /** Help-dialog section this shortcut is listed under. */
  category: string;
  key: string;
  action: () => void;
}

/** A rebindable app shortcut. By default it only fires when focus is
 *  *outside* a terminal, text input or editor — the terminal owns every key
 *  it can see. A def opts into capturing everywhere with
 *  `terminalCapture: true`, which the type system only allows for a
 *  `TerminalSafeCombo` default. */
export type ShortcutDef =
  | (ShortcutBase & { defaultKey: string; terminalCapture?: false })
  | (ShortcutBase & { defaultKey: TerminalSafeCombo; terminalCapture: true });

/** A key the user cannot rebind — hardcoded in a handler, a widget's own
 *  binding (xterm.js, CodeMirror), or not a single keystroke at all
 *  ("Drag divider"). Listed in the help dialog alongside the rebindable
 *  shortcuts, but invisible to the settings dialog and the key dispatcher. */
export interface FixedShortcut {
  category: string;
  key: string;
  label: string;
}

/** Section order for the help dialog. Every `category` string in the two
 *  lists below must appear here — `helpSections()` asserts it. */
export const CATEGORY_ORDER = [
  "General",
  "View & Panels",
  "Search",
  "Git",
  "Agents",
  "Panes & Tabs",
  "Terminal",
  "Code Editor",
] as const;

// Actions are bound later by main.ts via bindAction().
// `terminalCapture: true` = fires even while a terminal/input/editor has
// focus; everything else is outside-only. The pane ops stay outside-only on
// purpose so a chord typed at a shell never rearranges the layout.
const shortcuts: ShortcutDef[] = [
  { id: "command-palette", label: "Command Palette", category: "General", defaultKey: "Ctrl+P", key: "Ctrl+P", action: () => {} },
  { id: "new-workspace", label: "New Workspace", category: "General", defaultKey: "Ctrl+N", key: "Ctrl+N", action: () => {} },
  { id: "workspace-switcher", label: "Workspace Switcher", category: "General", defaultKey: "Ctrl+Space", key: "Ctrl+Space", action: () => {} },
  { id: "dashboard", label: "Dashboard", category: "General", defaultKey: "Alt+D", key: "Alt+D", action: () => {}, terminalCapture: true },
  { id: "help", label: "Keyboard Shortcuts", category: "General", defaultKey: "?", key: "?", action: () => {} },
  { id: "toggle-sidebar", label: "Toggle Sidebar", category: "View & Panels", defaultKey: "Ctrl+B", key: "Ctrl+B", action: () => {} },
  { id: "toggle-chat", label: "Toggle AI Chat", category: "View & Panels", defaultKey: "Ctrl+Shift+L", key: "Ctrl+Shift+L", action: () => {}, terminalCapture: true },
  { id: "kanban", label: "Kanban Board", category: "View & Panels", defaultKey: "Alt+K", key: "Alt+K", action: () => {}, terminalCapture: true },
  { id: "web-preview", label: "Open Web Preview", category: "View & Panels", defaultKey: "Alt+Shift+W", key: "Alt+Shift+W", action: () => {}, terminalCapture: true },
  { id: "theme", label: "Theme Settings", category: "View & Panels", defaultKey: "Alt+T", key: "Alt+T", action: () => {}, terminalCapture: true },
  { id: "settings", label: "Settings", category: "View & Panels", defaultKey: "Alt+S", key: "Alt+S", action: () => {}, terminalCapture: true },
  { id: "manage-providers", label: "Manage Providers", category: "View & Panels", defaultKey: "Alt+P", key: "Alt+P", action: () => {}, terminalCapture: true },
  { id: "logs", label: "Application Logs", category: "View & Panels", defaultKey: "Alt+Shift+L", key: "Alt+Shift+L", action: () => {}, terminalCapture: true },
  { id: "sessions", label: "Sessions (persistent)", category: "View & Panels", defaultKey: "Alt+Shift+S", key: "Alt+Shift+S", action: () => {}, terminalCapture: true },
  { id: "system-info", label: "System Info", category: "View & Panels", defaultKey: "Alt+I", key: "Alt+I", action: () => {}, terminalCapture: true },
  { id: "fuzzy-search", label: "Find File", category: "Search", defaultKey: "Ctrl+F", key: "Ctrl+F", action: () => {} },
  { id: "project-search", label: "Search in Project", category: "Search", defaultKey: "Ctrl+Shift+F", key: "Ctrl+Shift+F", action: () => {}, terminalCapture: true },
  { id: "terminal-search", label: "Search in Terminal", category: "Search", defaultKey: "Ctrl+Shift+B", key: "Ctrl+Shift+B", action: () => {}, terminalCapture: true },
  { id: "api-jq-filter", label: "API jq Filter", category: "Search", defaultKey: "Ctrl+J", key: "Ctrl+J", action: () => {} },
  { id: "merge-rebase", label: "Merge / Rebase", category: "Git", defaultKey: "Ctrl+M", key: "Ctrl+M", action: () => {} },
  { id: "git-log", label: "Git Log", category: "Git", defaultKey: "Alt+L", key: "Alt+L", action: () => {}, terminalCapture: true },
  { id: "git-stash", label: "Git Stash", category: "Git", defaultKey: "Ctrl+Shift+S", key: "Ctrl+Shift+S", action: () => {}, terminalCapture: true },
  { id: "undo", label: "Undo Stage/Unstage", category: "Git", defaultKey: "Ctrl+Z", key: "Ctrl+Z", action: () => {} },
  { id: "code-review", label: "Code Review (PR)", category: "Git", defaultKey: "Ctrl+Shift+R", key: "Ctrl+Shift+R", action: () => {}, terminalCapture: true },
  { id: "agent-manager", label: "Manage Agents", category: "Agents", defaultKey: "Ctrl+Shift+A", key: "Ctrl+Shift+A", action: () => {}, terminalCapture: true },
  { id: "dispatch-agent", label: "Dispatch Agent", category: "Agents", defaultKey: "Ctrl+Shift+D", key: "Ctrl+Shift+D", action: () => {}, terminalCapture: true },
  { id: "jump-attention", label: "Jump to Agent Needing Attention", category: "Agents", defaultKey: "Alt+A", key: "Alt+A", action: () => {}, terminalCapture: true },
  { id: "new-tab", label: "New Blank Tab", category: "Panes & Tabs", defaultKey: "Ctrl+T", key: "Ctrl+T", action: () => {} },
  { id: "split-right", label: "Split Pane Right", category: "Panes & Tabs", defaultKey: "Ctrl+\\", key: "Ctrl+\\", action: () => {} },
  { id: "split-down", label: "Split Pane Down", category: "Panes & Tabs", defaultKey: "Ctrl+Shift+\\", key: "Ctrl+Shift+\\", action: () => {} },
  { id: "close-pane", label: "Close Active Pane", category: "Panes & Tabs", defaultKey: "Ctrl+Shift+Q", key: "Ctrl+Shift+Q", action: () => {} },
];

/** Terminal copy/paste is a widget binding (`terminal-panel.ts`): Cmd+C/V on
 *  macOS like every Mac terminal, Ctrl+Shift+C/V elsewhere because plain
 *  Ctrl+C is SIGINT. `formatShortcut` turns "Ctrl" into ⌘ on macOS. */
const COPY_KEY = isMac ? "Ctrl+C" : "Ctrl+Shift+C";
const PASTE_KEY = isMac ? "Ctrl+V" : "Ctrl+Shift+V";

/** Non-rebindable keys, in help-dialog display order within their section.
 *  Every row must be a key that actually fires somewhere in the app. */
const fixedShortcuts: FixedShortcut[] = [
  { category: "General", key: "Esc", label: "Close Dialog / Overlay" },
  { category: "General", key: "Alt+1…9", label: "Switch to Workspace N" },
  { category: "Search", key: "Ctrl+H", label: "Request History (in API Explorer)" },
  { category: "Git", key: "Ctrl+Enter", label: "Commit (in commit message box)" },
  { category: "Panes & Tabs", key: "Ctrl+Tab", label: "Next Tab" },
  { category: "Panes & Tabs", key: "Ctrl+Shift+Tab", label: "Previous Tab" },
  { category: "Panes & Tabs", key: "Drag divider", label: "Resize split" },
  { category: "Terminal", key: COPY_KEY, label: "Copy Selection" },
  { category: "Terminal", key: PASTE_KEY, label: "Paste from Clipboard" },
  { category: "Terminal", key: "Select text", label: "Auto-copy to Clipboard" },
  { category: "Terminal", key: "Shift+PgUp / PgDn", label: "Scroll one page" },
  { category: "Terminal", key: "Shift+Home / End", label: "Scroll to top / bottom" },
  { category: "Code Editor", key: "Ctrl+I", label: "Quick Edit (in file viewer)" },
  { category: "Code Editor", key: "Ctrl+E", label: "Open in $EDITOR (in file viewer / search results)" },
  { category: "Code Editor", key: "Ctrl+S", label: "Save file (in editor)" },
  { category: "Code Editor", key: "Ctrl+F", label: "Find in file (CodeMirror)" },
];

/** Keys a rebind may not claim: they belong to a widget or a hardcoded
 *  handler and would silently stop working. */
const RESERVED_COMBOS: { key: string; label: string }[] = [
  { key: COPY_KEY, label: "Copy Selection (terminal)" },
  { key: PASTE_KEY, label: "Paste from Clipboard (terminal)" },
  { key: "Ctrl+Tab", label: "Next Tab" },
  { key: "Ctrl+Shift+Tab", label: "Previous Tab" },
  ...Array.from({ length: 9 }, (_, i) => ({ key: `Alt+${i + 1}`, label: `Switch to Workspace ${i + 1}` })),
];

export interface HelpItem {
  key: string;
  label: string;
  /** True when the key only fires with focus outside a terminal/editor. */
  outsideOnly: boolean;
}

/** Help-dialog sections: every shortcut — rebindable ones showing their
 *  *current* key (so a rebind never lies) — merged with the fixed keys,
 *  grouped by category in `CATEGORY_ORDER`. */
export function helpSections(): { category: string; items: HelpItem[] }[] {
  const known = new Set<string>(CATEGORY_ORDER);
  for (const def of shortcuts) {
    if (!known.has(def.category)) throw new Error(`Shortcut '${def.id}' has unknown category '${def.category}'`);
  }
  for (const f of fixedShortcuts) {
    if (!known.has(f.category)) throw new Error(`Fixed shortcut '${f.label}' has unknown category '${f.category}'`);
  }
  return CATEGORY_ORDER.map((category) => ({
    category,
    items: [
      ...shortcuts
        .filter((s) => s.category === category)
        .map((s): HelpItem => ({
          key: formatShortcut(s.key),
          label: s.label,
          outsideOnly: !capturesInTerminal(s),
        })),
      ...fixedShortcuts
        .filter((f) => f.category === category)
        .map((f): HelpItem => ({ key: formatShortcut(f.key), label: f.label, outsideOnly: false })),
    ],
  })).filter((g) => g.items.length > 0);
}

export function getShortcuts(): ShortcutDef[] {
  return shortcuts;
}

export function bindAction(id: string, action: () => void) {
  const def = shortcuts.find((s) => s.id === id);
  if (def) def.action = action;
}

export function getShortcutKey(id: string): string {
  const def = shortcuts.find((s) => s.id === id);
  return def ? formatShortcut(def.key) : "";
}

export function updateShortcut(id: string, newKey: string) {
  const def = shortcuts.find((s) => s.id === id);
  if (def) def.key = newKey;
  persistOverrides();
}

export function resetAllShortcuts() {
  for (const def of shortcuts) {
    def.key = def.defaultKey;
  }
  persistOverrides();
}

/** The shortcut or reserved widget key already bound to `key`, if any. */
export function findConflict(id: string, key: string): { label: string } | null {
  return (
    shortcuts.find((s) => s.id !== id && s.key === key) ??
    RESERVED_COMBOS.find((r) => r.key === key) ??
    null
  );
}

/** True for chords a terminal can never receive as bytes — see
 *  `TerminalSafeCombo`. Used both by the dispatcher and by the settings
 *  dialog to warn about a rebind that would demote a shortcut to
 *  outside-only. */
export function isTerminalSafeCombo(combo: string): boolean {
  const c = parseCombo(combo);
  return c.alt || (c.ctrl && c.shift);
}

/** Whether `def` fires while a terminal/input/editor has focus: it must opt
 *  in *and* its current key must be terminal-safe (a user rebind to a plain
 *  Ctrl+letter demotes it to outside-only instead of eating the key). */
function capturesInTerminal(def: ShortcutDef): boolean {
  return def.terminalCapture === true && isTerminalSafeCombo(def.key);
}

// ── Persistence ────────────────────────────────

const SHORTCUTS_KEY = "shortcuts";
const SHELL_KEY = "shell";

function persistOverrides() {
  const overrides: Record<string, string> = {};
  for (const def of shortcuts) {
    if (def.key !== def.defaultKey) overrides[def.id] = def.key;
  }
  settingsStore.patch(SHORTCUTS_KEY, overrides);
}

/** Apply the user's rebinds from the (already loaded) settings store. */
export function loadShortcuts() {
  const overrides = settingsStore.get<Record<string, string>>(SHORTCUTS_KEY);
  if (!overrides || typeof overrides !== "object") return;
  for (const def of shortcuts) {
    if (overrides[def.id]) def.key = overrides[def.id];
  }
}

export function getShellSetting(): string {
  return settingsStore.get<string>(SHELL_KEY) || "";
}

export function setShellSetting(shell: string) {
  settingsStore.patch(SHELL_KEY, shell || undefined);
}

// ── Key matching ────────────────────────────────

export function parseCombo(combo: string): { ctrl: boolean; shift: boolean; alt: boolean; key: string } {
  const parts = combo.split("+");
  const key = parts[parts.length - 1];
  return {
    ctrl: parts.includes("Ctrl") || parts.includes("⌘"),
    shift: parts.includes("Shift"),
    alt: parts.includes("Alt"),
    key,
  };
}

function matchesEvent(e: KeyboardEvent, combo: string): boolean {
  const c = parseCombo(combo);
  // Printable non-alphanumeric keys (?, {, }, …) may need Shift to produce
  // on some layouts — match on the produced character, not the modifier.
  const shiftAgnostic = c.key.length === 1 && !/[a-z0-9]/i.test(c.key);
  if (!shiftAgnostic && e.shiftKey !== c.shift) return false;

  if (isMac) {
    // On macOS, both Ctrl and Alt bindings map to Cmd (Meta).
    const needsMeta = c.ctrl || c.alt;
    if (e.metaKey !== needsMeta) return false;
    // Ensure plain Alt/Ctrl aren't spuriously held
    if (!c.alt && e.altKey) return false;
    if (!c.ctrl && e.ctrlKey) return false;
  } else {
    if (e.ctrlKey !== c.ctrl) return false;
    if (e.altKey !== c.alt) return false;
  }

  // Handle special keys
  if (c.key === "Space") return e.key === " ";
  if (c.key === "Tab") return e.key === "Tab";
  if (c.key === "?") return e.key === "?";

  // Case-insensitive letter match
  return e.key.toLowerCase() === c.key.toLowerCase();
}

/** Focus is somewhere that owns its own keys: a terminal (every byte is the
 *  shell's), a native text input, or a contenteditable editor (CodeMirror,
 *  Milkdown) with its own keymap. */
function focusOwnsKeys(): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return false;
  if (el.closest(".xterm")) return true;
  if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") return true;
  return el.isContentEditable;
}

export function handleGlobalKeydown(e: KeyboardEvent) {
  const owned = focusOwnsKeys();

  for (const def of shortcuts) {
    if (owned && !capturesInTerminal(def)) continue;
    if (matchesEvent(e, def.key)) {
      e.preventDefault();
      e.stopPropagation();
      def.action();
      return;
    }
  }

  // Ctrl+Tab / Ctrl+Shift+Tab (Cmd+Tab on macOS): tab switching (not customizable)
  if (modCtrl(e) && e.key === "Tab") {
    e.preventDefault();
    e.stopPropagation();
    const event = new CustomEvent("switch-tab", { detail: { direction: e.shiftKey ? -1 : 1 } });
    document.dispatchEvent(event);
    return;
  }

  // Alt+1…9 (Cmd+1…9 on macOS): jump to workspace N (not customizable).
  if (modAlt(e) && !e.shiftKey && !(isMac ? e.ctrlKey || e.altKey : e.ctrlKey) && /^[1-9]$/.test(e.key)) {
    e.preventDefault();
    e.stopPropagation();
    const event = new CustomEvent("switch-workspace", { detail: { index: Number(e.key) - 1 } });
    document.dispatchEvent(event);
  }
}

export function eventToCombo(e: KeyboardEvent): string | null {
  const parts: string[] = [];
  // On macOS, Meta (Cmd) is recorded as "Ctrl" so combos stay portable.
  if (isMac) {
    if (e.metaKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
  } else {
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
  }
  if (e.shiftKey) parts.push("Shift");

  const key = e.key;
  // Skip modifier-only presses
  if (["Control", "Alt", "Shift", "Meta"].includes(key)) return null;

  if (key === " ") parts.push("Space");
  else if (key === "Tab") parts.push("Tab");
  else if (key.length === 1) parts.push(key.toUpperCase());
  else parts.push(key);

  return parts.join("+");
}
