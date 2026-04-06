import * as ipc from "./ipc";

export interface ShortcutDef {
  id: string;
  label: string;
  defaultKey: string;
  key: string;
  action: () => void;
  /** If true, shortcut only fires when NOT in terminal/input */
  outsideOnly?: boolean;
}

// Actions are bound later by main.ts via bindAction()
const shortcuts: ShortcutDef[] = [
  { id: "command-palette", label: "Command Palette", defaultKey: "Ctrl+P", key: "Ctrl+P", action: () => {} },
  { id: "new-workspace", label: "New Workspace", defaultKey: "Ctrl+N", key: "Ctrl+N", action: () => {} },
  { id: "merge-rebase", label: "Merge / Rebase", defaultKey: "Ctrl+M", key: "Ctrl+M", action: () => {} },
  { id: "workspace-switcher", label: "Workspace Switcher", defaultKey: "Ctrl+Space", key: "Ctrl+Space", action: () => {} },
  { id: "fuzzy-search", label: "Find File", defaultKey: "Ctrl+F", key: "Ctrl+F", action: () => {} },
  { id: "project-search", label: "Search in Project", defaultKey: "Ctrl+Shift+F", key: "Ctrl+Shift+F", action: () => {} },
  { id: "terminal-search", label: "Search in Terminal", defaultKey: "Ctrl+Shift+B", key: "Ctrl+Shift+B", action: () => {} },
  { id: "git-log", label: "Git Log", defaultKey: "Alt+L", key: "Alt+L", action: () => {} },
  { id: "dashboard", label: "Dashboard", defaultKey: "Alt+D", key: "Alt+D", action: () => {} },
  { id: "git-stash", label: "Git Stash", defaultKey: "Ctrl+Shift+S", key: "Ctrl+Shift+S", action: () => {} },
  { id: "code-review", label: "Code Review", defaultKey: "Ctrl+Shift+R", key: "Ctrl+Shift+R", action: () => {} },
  { id: "agent-manager", label: "Manage Agents", defaultKey: "Ctrl+Shift+A", key: "Ctrl+Shift+A", action: () => {} },
  { id: "dispatch-agent", label: "Dispatch Agent", defaultKey: "Ctrl+Shift+D", key: "Ctrl+Shift+D", action: () => {} },
  { id: "kanban", label: "Kanban Board", defaultKey: "Alt+K", key: "Alt+K", action: () => {} },
  { id: "theme", label: "Theme Settings", defaultKey: "Alt+T", key: "Alt+T", action: () => {} },
  { id: "settings", label: "Settings", defaultKey: "Alt+S", key: "Alt+S", action: () => {} },
  { id: "logs", label: "Application Logs", defaultKey: "Alt+Shift+L", key: "Alt+Shift+L", action: () => {} },
  { id: "undo", label: "Undo Stage/Unstage", defaultKey: "Ctrl+Z", key: "Ctrl+Z", action: () => {}, outsideOnly: true },
  { id: "api-jq-filter", label: "API jq Filter", defaultKey: "Ctrl+J", key: "Ctrl+J", action: () => {}, outsideOnly: true },
  { id: "help", label: "Keyboard Shortcuts", defaultKey: "?", key: "?", action: () => {}, outsideOnly: true },
];

export function getShortcuts(): ShortcutDef[] {
  return shortcuts;
}

export function bindAction(id: string, action: () => void) {
  const def = shortcuts.find((s) => s.id === id);
  if (def) def.action = action;
}

export function getShortcutKey(id: string): string {
  const def = shortcuts.find((s) => s.id === id);
  return def?.key ?? "";
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
    ctrl: parts.includes("Ctrl"),
    shift: parts.includes("Shift"),
    alt: parts.includes("Alt"),
    key,
  };
}

function matchesEvent(e: KeyboardEvent, combo: string): boolean {
  const c = parseCombo(combo);
  if (e.ctrlKey !== c.ctrl) return false;
  if (e.shiftKey !== c.shift) return false;
  if (e.altKey !== c.alt) return false;

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

  // Ctrl+Tab / Ctrl+Shift+Tab: tab switching (not customizable)
  if (e.ctrlKey && e.key === "Tab") {
    e.preventDefault();
    e.stopPropagation();
    // This is handled inline because it needs shift-aware direction
    const event = new CustomEvent("switch-tab", { detail: { direction: e.shiftKey ? -1 : 1 } });
    document.dispatchEvent(event);
  }
}

export function eventToCombo(e: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
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
