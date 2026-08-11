import * as ipc from "./ipc";

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

export interface ShortcutDef {
  id: string;
  label: string;
  /** Help-dialog section this shortcut is listed under. */
  category: string;
  defaultKey: string;
  key: string;
  action: () => void;
  /** If true, shortcut only fires when NOT in terminal/input */
  outsideOnly?: boolean;
}

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

// Actions are bound later by main.ts via bindAction()
const shortcuts: ShortcutDef[] = [
  { id: "command-palette", label: "Command Palette", category: "General", defaultKey: "Ctrl+P", key: "Ctrl+P", action: () => {} },
  { id: "new-workspace", label: "New Workspace", category: "General", defaultKey: "Ctrl+N", key: "Ctrl+N", action: () => {} },
  { id: "workspace-switcher", label: "Workspace Switcher", category: "General", defaultKey: "Ctrl+Space", key: "Ctrl+Space", action: () => {} },
  { id: "dashboard", label: "Dashboard", category: "General", defaultKey: "Alt+D", key: "Alt+D", action: () => {} },
  { id: "help", label: "Keyboard Shortcuts", category: "General", defaultKey: "?", key: "?", action: () => {}, outsideOnly: true },
  { id: "toggle-sidebar", label: "Toggle Sidebar", category: "View & Panels", defaultKey: "Ctrl+B", key: "Ctrl+B", action: () => {} },
  { id: "toggle-chat", label: "Toggle AI Chat", category: "View & Panels", defaultKey: "Ctrl+Shift+L", key: "Ctrl+Shift+L", action: () => {} },
  { id: "kanban", label: "Kanban Board", category: "View & Panels", defaultKey: "Alt+K", key: "Alt+K", action: () => {} },
  { id: "web-preview", label: "Open Web Preview", category: "View & Panels", defaultKey: "Alt+Shift+W", key: "Alt+Shift+W", action: () => {} },
  { id: "theme", label: "Theme Settings", category: "View & Panels", defaultKey: "Alt+T", key: "Alt+T", action: () => {} },
  { id: "settings", label: "Settings", category: "View & Panels", defaultKey: "Alt+S", key: "Alt+S", action: () => {} },
  { id: "manage-providers", label: "Manage Providers", category: "View & Panels", defaultKey: "Alt+P", key: "Alt+P", action: () => {} },
  { id: "logs", label: "Application Logs", category: "View & Panels", defaultKey: "Alt+Shift+L", key: "Alt+Shift+L", action: () => {} },
  { id: "system-info", label: "System Info", category: "View & Panels", defaultKey: "Alt+I", key: "Alt+I", action: () => {} },
  { id: "fuzzy-search", label: "Find File", category: "Search", defaultKey: "Ctrl+F", key: "Ctrl+F", action: () => {} },
  { id: "project-search", label: "Search in Project", category: "Search", defaultKey: "Ctrl+Shift+F", key: "Ctrl+Shift+F", action: () => {} },
  { id: "terminal-search", label: "Search in Terminal", category: "Search", defaultKey: "Ctrl+Shift+B", key: "Ctrl+Shift+B", action: () => {} },
  { id: "api-jq-filter", label: "API jq Filter", category: "Search", defaultKey: "Ctrl+J", key: "Ctrl+J", action: () => {}, outsideOnly: true },
  { id: "merge-rebase", label: "Merge / Rebase", category: "Git", defaultKey: "Ctrl+M", key: "Ctrl+M", action: () => {} },
  { id: "git-log", label: "Git Log", category: "Git", defaultKey: "Alt+L", key: "Alt+L", action: () => {} },
  { id: "git-stash", label: "Git Stash", category: "Git", defaultKey: "Ctrl+Shift+S", key: "Ctrl+Shift+S", action: () => {} },
  { id: "undo", label: "Undo Stage/Unstage", category: "Git", defaultKey: "Ctrl+Z", key: "Ctrl+Z", action: () => {}, outsideOnly: true },
  { id: "code-review", label: "Code Review (PR)", category: "Git", defaultKey: "Ctrl+Shift+R", key: "Ctrl+Shift+R", action: () => {} },
  { id: "agent-manager", label: "Manage Agents", category: "Agents", defaultKey: "Ctrl+Shift+A", key: "Ctrl+Shift+A", action: () => {} },
  { id: "dispatch-agent", label: "Dispatch Agent", category: "Agents", defaultKey: "Ctrl+Shift+D", key: "Ctrl+Shift+D", action: () => {} },
  { id: "new-tab", label: "New Blank Tab", category: "Panes & Tabs", defaultKey: "Ctrl+T", key: "Ctrl+T", action: () => {}, outsideOnly: true },
  { id: "split-right", label: "Split Pane Right", category: "Panes & Tabs", defaultKey: "Ctrl+\\", key: "Ctrl+\\", action: () => {}, outsideOnly: true },
  { id: "split-down", label: "Split Pane Down", category: "Panes & Tabs", defaultKey: "Ctrl+Shift+\\", key: "Ctrl+Shift+\\", action: () => {}, outsideOnly: true },
  { id: "close-pane", label: "Close Active Pane", category: "Panes & Tabs", defaultKey: "Ctrl+Shift+Q", key: "Ctrl+Shift+Q", action: () => {}, outsideOnly: true },
];

/** Non-rebindable keys, in help-dialog display order within their section. */
const fixedShortcuts: FixedShortcut[] = [
  { category: "General", key: "Esc", label: "Close Dialog / Overlay" },
  { category: "Panes & Tabs", key: "Ctrl+Tab", label: "Next Tab" },
  { category: "Panes & Tabs", key: "Ctrl+Shift+Tab", label: "Previous Tab" },
  { category: "Panes & Tabs", key: "▾ on tab / Right-click", label: "Tab options menu" },
  { category: "Panes & Tabs", key: "Drag divider", label: "Resize split" },
  { category: "Terminal", key: "Ctrl+C", label: "Copy Selection" },
  { category: "Terminal", key: "Ctrl+V", label: "Paste from Clipboard" },
  { category: "Terminal", key: "Select text", label: "Auto-copy to Clipboard" },
  { category: "Code Editor", key: "Ctrl+I", label: "Quick Edit (in file viewer)" },
  { category: "Code Editor", key: "Ctrl+S", label: "Save file (in editor)" },
  { category: "Code Editor", key: "Ctrl+F", label: "Find in file (CodeMirror)" },
];

/** Help-dialog sections: every shortcut — rebindable ones showing their
 *  *current* key (so a rebind never lies) — merged with the fixed keys,
 *  grouped by category in `CATEGORY_ORDER`. */
export function helpSections(): { category: string; items: [string, string][] }[] {
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
        .map((s): [string, string] => [formatShortcut(s.key), s.label]),
      ...fixedShortcuts
        .filter((f) => f.category === category)
        .map((f): [string, string] => [formatShortcut(f.key), f.label]),
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
  schedulePersist();
}

export function resetAllShortcuts() {
  for (const def of shortcuts) {
    def.key = def.defaultKey;
  }
  schedulePersist();
}

export function findConflict(id: string, key: string): ShortcutDef | null {
  return shortcuts.find((s) => s.id !== id && s.key === key) ?? null;
}

// ── Persistence ────────────────────────────────

let persistTimer: ReturnType<typeof setTimeout> | null = null;

function schedulePersist() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(async () => {
    const settings = await loadSettingsJson();
    const overrides: Record<string, string> = {};
    for (const def of shortcuts) {
      if (def.key !== def.defaultKey) {
        overrides[def.id] = def.key;
      }
    }
    settings.shortcuts = overrides;
    await ipc.setSettings(JSON.stringify(settings)).catch(() => {});
  }, 300);
}

async function loadSettingsJson(): Promise<Record<string, unknown>> {
  try {
    const raw = await ipc.getSettings();
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return {};
}

export async function loadShortcuts() {
  const settings = await loadSettingsJson();
  const overrides = settings.shortcuts as Record<string, string> | undefined;
  if (overrides) {
    for (const def of shortcuts) {
      if (overrides[def.id]) {
        def.key = overrides[def.id];
      }
    }
  }
}

export function getShellSetting(): Promise<string> {
  return loadSettingsJson().then((s) => (s.shell as string) || "");
}

export async function setShellSetting(shell: string) {
  const settings = await loadSettingsJson();
  settings.shell = shell;
  await ipc.setSettings(JSON.stringify(settings)).catch(() => {});
}

// ── Key matching ────────────────────────────────

function parseCombo(combo: string): { ctrl: boolean; shift: boolean; alt: boolean; key: string } {
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

export function handleGlobalKeydown(e: KeyboardEvent) {
  const inTerminal = !!document.activeElement?.closest(".xterm");
  const inInput =
    document.activeElement?.tagName === "INPUT" ||
    document.activeElement?.tagName === "TEXTAREA";

  for (const def of shortcuts) {
    if (def.outsideOnly && (inTerminal || inInput)) continue;
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
