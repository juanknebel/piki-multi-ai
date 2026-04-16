import * as ipc from "./ipc";

// ── Types ────────────────────────────────────────────

/** Base color keys — stored in presets and overrides. CSS var name without "--" prefix. */
export type ThemeColorKey = keyof ThemeColors;

export interface ThemeColors {
  "bg-primary": string;
  "bg-secondary": string;
  "bg-tertiary": string;
  "bg-hover": string;
  "bg-active": string;
  "bg-input": string;
  "bg-dropdown": string;
  "bg-surface": string;
  "bg-elevated": string;

  "text-primary": string;
  "text-secondary": string;
  "text-muted": string;
  "text-accent": string;
  "text-bright": string;

  "border-primary": string;
  "border-active": string;
  "border-inactive": string;

  "accent-primary": string;
  "accent-hover": string;
  "accent-focus": string;
  "accent-warm": string;

  "activity-bar-bg": string;
  "activity-bar-fg": string;
  "activity-bar-inactive": string;
  "activity-bar-badge": string;

  "sidebar-bg": string;
  "sidebar-header-bg": string;
  "sidebar-header-fg": string;
  "sidebar-item-hover": string;
  "sidebar-item-active": string;

  "tab-active-bg": string;
  "tab-active-fg": string;
  "tab-inactive-bg": string;
  "tab-inactive-fg": string;
  "tab-border": string;
  "tab-active-border-top": string;

  "terminal-bg": string;
  "terminal-fg": string;
  "terminal-cursor": string;

  "statusbar-bg": string;
  "statusbar-fg": string;
  "statusbar-no-folder": string;

  "git-modified": string;
  "git-added": string;
  "git-deleted": string;
  "git-renamed": string;
  "git-untracked": string;
  "git-conflicted": string;
  "git-staged": string;
  "git-staged-modified": string;

  "dialog-bg": string;
  "dialog-border": string;

  "toast-info-bg": string;
  "toast-success-bg": string;
  "toast-error-bg": string;
  "toast-fg": string;

  // xterm ANSI colors
  "xterm-black": string;
  "xterm-red": string;
  "xterm-green": string;
  "xterm-yellow": string;
  "xterm-blue": string;
  "xterm-magenta": string;
  "xterm-cyan": string;
  "xterm-white": string;
  "xterm-bright-black": string;
  "xterm-bright-red": string;
  "xterm-bright-green": string;
  "xterm-bright-yellow": string;
  "xterm-bright-blue": string;
  "xterm-bright-magenta": string;
  "xterm-bright-cyan": string;
  "xterm-bright-white": string;
}

export interface ThemePreset {
  id: string;
  name: string;
  isDark: boolean;
  colors: ThemeColors;
}

interface ThemeState {
  activePreset: string;
  overrides: Partial<ThemeColors>;
}

// ── Color groups for the dialog ──────────────────────

export interface ColorGroup {
  label: string;
  keys: ThemeColorKey[];
}

export const COLOR_GROUPS: ColorGroup[] = [
  { label: "Backgrounds", keys: ["bg-primary", "bg-secondary", "bg-tertiary", "bg-hover", "bg-active", "bg-input", "bg-dropdown", "bg-surface", "bg-elevated"] },
  { label: "Text", keys: ["text-primary", "text-secondary", "text-muted", "text-accent", "text-bright"] },
  { label: "Borders", keys: ["border-primary", "border-active", "border-inactive"] },
  { label: "Accents", keys: ["accent-primary", "accent-hover", "accent-focus", "accent-warm"] },
  { label: "Activity Bar", keys: ["activity-bar-bg", "activity-bar-fg", "activity-bar-inactive", "activity-bar-badge"] },
  { label: "Sidebar", keys: ["sidebar-bg", "sidebar-header-bg", "sidebar-header-fg", "sidebar-item-hover", "sidebar-item-active"] },
  { label: "Tabs", keys: ["tab-active-bg", "tab-active-fg", "tab-inactive-bg", "tab-inactive-fg", "tab-border", "tab-active-border-top"] },
  { label: "Terminal", keys: ["terminal-bg", "terminal-fg", "terminal-cursor"] },
  { label: "Status Bar", keys: ["statusbar-bg", "statusbar-fg", "statusbar-no-folder"] },
  { label: "Git Status", keys: ["git-modified", "git-added", "git-deleted", "git-renamed", "git-untracked", "git-conflicted", "git-staged", "git-staged-modified"] },
  { label: "Dialog", keys: ["dialog-bg", "dialog-border"] },
  { label: "Toast", keys: ["toast-info-bg", "toast-success-bg", "toast-error-bg", "toast-fg"] },
  { label: "Terminal ANSI", keys: ["xterm-black", "xterm-red", "xterm-green", "xterm-yellow", "xterm-blue", "xterm-magenta", "xterm-cyan", "xterm-white", "xterm-bright-black", "xterm-bright-red", "xterm-bright-green", "xterm-bright-yellow", "xterm-bright-blue", "xterm-bright-magenta", "xterm-bright-cyan", "xterm-bright-white"] },
];

// ── Utilities ────────────────────────────────────────

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [parseInt(h.substring(0, 2), 16), parseInt(h.substring(2, 4), 16), parseInt(h.substring(4, 6), 16)];
}

function hexToRgba(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function hexToGlow(hex: string, alpha: number, radius: number = 10): string {
  return `0 0 ${radius}px ${hexToRgba(hex, alpha)}`;
}

export function isValidHex(s: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(s);
}

/** "bg-primary" → "Primary", "xterm-bright-red" → "Bright Red" */
export function keyToLabel(key: string): string {
  // Strip common prefixes
  let s = key;
  for (const prefix of ["bg-", "text-", "border-", "accent-", "activity-bar-", "sidebar-", "sidebar-item-", "sidebar-header-", "tab-", "tab-active-", "tab-inactive-", "terminal-", "statusbar-", "git-", "dialog-", "toast-", "xterm-"]) {
    if (s.startsWith(prefix)) { s = s.slice(prefix.length); break; }
  }
  return s.split("-").map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}

// ── Derived colors (computed from base, not stored) ──

function computeDerived(colors: Record<string, string>, isDark: boolean): Record<string, string> {
  const ap = colors["accent-primary"] || "#39bae6";
  const aw = colors["accent-warm"] || "#e6a730";
  const tc = colors["terminal-cursor"] || ap;

  return {
    "accent-muted": hexToRgba(ap, 0.12),
    "accent-glow": hexToGlow(ap, 0.3),
    "accent-glow-lg": `${hexToGlow(ap, 0.2, 20)}, ${hexToGlow(ap, 0.06, 40)}`,
    "accent-warm-muted": hexToRgba(aw, 0.12),
    "accent-warm-glow": hexToGlow(aw, 0.2),
    "sidebar-item-focus": hexToRgba(ap, 0.08),
    "scrollbar-thumb": hexToRgba(ap, 0.08),
    "scrollbar-thumb-hover": hexToRgba(ap, 0.22),
    "terminal-selection": hexToRgba(tc, 0.18),
    "statusbar-item-hover": hexToRgba(ap, 0.06),
    "border-subtle": isDark ? "rgba(255, 255, 255, 0.03)" : "rgba(0, 0, 0, 0.06)",
    "dialog-shadow": isDark ? "rgba(0, 0, 0, 0.65)" : "rgba(0, 0, 0, 0.2)",
  };
}

// ── Presets ───────────────────────────────────────────

const OBSIDIAN_DARK: ThemePreset = {
  id: "obsidian-dark", name: "Obsidian Dark", isDark: true,
  colors: {
    "bg-primary": "#0b0f14", "bg-secondary": "#0e1319", "bg-tertiary": "#151c25",
    "bg-hover": "#182030", "bg-active": "#1e2a3c", "bg-input": "#111820",
    "bg-dropdown": "#151c25", "bg-surface": "#0d1117", "bg-elevated": "#1a2332",
    "text-primary": "#adbac7", "text-secondary": "#8b96a3", "text-muted": "#5d7080",
    "text-accent": "#39bae6", "text-bright": "#e6edf3",
    "border-primary": "#1a2231", "border-active": "#39bae6", "border-inactive": "#1a2231",
    "accent-primary": "#39bae6", "accent-hover": "#5cc8f0", "accent-focus": "#39bae6",
    "accent-warm": "#e6a730",
    "activity-bar-bg": "#080b0f", "activity-bar-fg": "#e6edf3",
    "activity-bar-inactive": "#64768a", "activity-bar-badge": "#39bae6",
    "sidebar-bg": "#0d1117", "sidebar-header-bg": "#0b0f14", "sidebar-header-fg": "#768390",
    "sidebar-item-hover": "#131a23", "sidebar-item-active": "#182030",
    "tab-active-bg": "#0b0f14", "tab-active-fg": "#e6edf3", "tab-inactive-bg": "#0d1117",
    "tab-inactive-fg": "#768390", "tab-border": "#151c25", "tab-active-border-top": "#39bae6",
    "terminal-bg": "#0b0f14", "terminal-fg": "#adbac7", "terminal-cursor": "#39bae6",
    "statusbar-bg": "#080b0f", "statusbar-fg": "#8b95a1", "statusbar-no-folder": "#5a2d72",
    "git-modified": "#d4a12e", "git-added": "#3fb950", "git-deleted": "#f85149",
    "git-renamed": "#3fb950", "git-untracked": "#7ee787", "git-conflicted": "#f47067",
    "git-staged": "#3fb950", "git-staged-modified": "#d4a12e",
    "dialog-bg": "#111820", "dialog-border": "#253040",
    "toast-info-bg": "#122a3e", "toast-success-bg": "#0f2818",
    "toast-error-bg": "#2d1215", "toast-fg": "#e6edf3",
    "xterm-black": "#0b0f14", "xterm-red": "#f85149", "xterm-green": "#3fb950",
    "xterm-yellow": "#d4a12e", "xterm-blue": "#39bae6", "xterm-magenta": "#bc8cff",
    "xterm-cyan": "#39bae6", "xterm-white": "#adbac7",
    "xterm-bright-black": "#4d5566", "xterm-bright-red": "#ff7b72",
    "xterm-bright-green": "#56d364", "xterm-bright-yellow": "#e3b341",
    "xterm-bright-blue": "#5cc8f0", "xterm-bright-magenta": "#d2a8ff",
    "xterm-bright-cyan": "#5cc8f0", "xterm-bright-white": "#e6edf3",
  },
};

const NORD_DARK: ThemePreset = {
  id: "nord-dark", name: "Nord", isDark: true,
  colors: {
    "bg-primary": "#2e3440", "bg-secondary": "#3b4252", "bg-tertiary": "#434c5e",
    "bg-hover": "#4c566a", "bg-active": "#4c566a", "bg-input": "#3b4252",
    "bg-dropdown": "#3b4252", "bg-surface": "#2e3440", "bg-elevated": "#434c5e",
    "text-primary": "#d8dee9", "text-secondary": "#9ab3cc", "text-muted": "#8892a3",
    "text-accent": "#88c0d0", "text-bright": "#eceff4",
    "border-primary": "#3b4252", "border-active": "#88c0d0", "border-inactive": "#3b4252",
    "accent-primary": "#88c0d0", "accent-hover": "#8fbcbb", "accent-focus": "#88c0d0",
    "accent-warm": "#ebcb8b",
    "activity-bar-bg": "#2e3440", "activity-bar-fg": "#eceff4",
    "activity-bar-inactive": "#8892a3", "activity-bar-badge": "#88c0d0",
    "sidebar-bg": "#2e3440", "sidebar-header-bg": "#2e3440", "sidebar-header-fg": "#9ab3cc",
    "sidebar-item-hover": "#3b4252", "sidebar-item-active": "#434c5e",
    "tab-active-bg": "#2e3440", "tab-active-fg": "#eceff4", "tab-inactive-bg": "#3b4252",
    "tab-inactive-fg": "#9ab3cc", "tab-border": "#3b4252", "tab-active-border-top": "#88c0d0",
    "terminal-bg": "#2e3440", "terminal-fg": "#d8dee9", "terminal-cursor": "#88c0d0",
    "statusbar-bg": "#2e3440", "statusbar-fg": "#d8dee9", "statusbar-no-folder": "#5e81ac",
    "git-modified": "#ebcb8b", "git-added": "#a3be8c", "git-deleted": "#d9838a",
    "git-renamed": "#a3be8c", "git-untracked": "#a3be8c", "git-conflicted": "#d08770",
    "git-staged": "#a3be8c", "git-staged-modified": "#ebcb8b",
    "dialog-bg": "#3b4252", "dialog-border": "#4c566a",
    "toast-info-bg": "#3b4252", "toast-success-bg": "#3b4252",
    "toast-error-bg": "#3b4252", "toast-fg": "#eceff4",
    "xterm-black": "#3b4252", "xterm-red": "#bf616a", "xterm-green": "#a3be8c",
    "xterm-yellow": "#ebcb8b", "xterm-blue": "#81a1c1", "xterm-magenta": "#b48ead",
    "xterm-cyan": "#88c0d0", "xterm-white": "#e5e9f0",
    "xterm-bright-black": "#4c566a", "xterm-bright-red": "#bf616a",
    "xterm-bright-green": "#a3be8c", "xterm-bright-yellow": "#ebcb8b",
    "xterm-bright-blue": "#81a1c1", "xterm-bright-magenta": "#b48ead",
    "xterm-bright-cyan": "#8fbcbb", "xterm-bright-white": "#eceff4",
  },
};

const CATPPUCCIN_MOCHA: ThemePreset = {
  id: "catppuccin-mocha", name: "Catppuccin Mocha", isDark: true,
  colors: {
    "bg-primary": "#1e1e2e", "bg-secondary": "#181825", "bg-tertiary": "#313244",
    "bg-hover": "#45475a", "bg-active": "#585b70", "bg-input": "#313244",
    "bg-dropdown": "#313244", "bg-surface": "#1e1e2e", "bg-elevated": "#45475a",
    "text-primary": "#cdd6f4", "text-secondary": "#a6adc8", "text-muted": "#7e819a",
    "text-accent": "#89b4fa", "text-bright": "#cdd6f4",
    "border-primary": "#313244", "border-active": "#89b4fa", "border-inactive": "#313244",
    "accent-primary": "#89b4fa", "accent-hover": "#b4d0fb", "accent-focus": "#89b4fa",
    "accent-warm": "#fab387",
    "activity-bar-bg": "#181825", "activity-bar-fg": "#cdd6f4",
    "activity-bar-inactive": "#858999", "activity-bar-badge": "#89b4fa",
    "sidebar-bg": "#1e1e2e", "sidebar-header-bg": "#181825", "sidebar-header-fg": "#a6adc8",
    "sidebar-item-hover": "#313244", "sidebar-item-active": "#45475a",
    "tab-active-bg": "#1e1e2e", "tab-active-fg": "#cdd6f4", "tab-inactive-bg": "#181825",
    "tab-inactive-fg": "#a6adc8", "tab-border": "#313244", "tab-active-border-top": "#89b4fa",
    "terminal-bg": "#1e1e2e", "terminal-fg": "#cdd6f4", "terminal-cursor": "#f5e0dc",
    "statusbar-bg": "#181825", "statusbar-fg": "#a6adc8", "statusbar-no-folder": "#cba6f7",
    "git-modified": "#f9e2af", "git-added": "#a6e3a1", "git-deleted": "#f38ba8",
    "git-renamed": "#a6e3a1", "git-untracked": "#94e2d5", "git-conflicted": "#f38ba8",
    "git-staged": "#a6e3a1", "git-staged-modified": "#f9e2af",
    "dialog-bg": "#313244", "dialog-border": "#45475a",
    "toast-info-bg": "#313244", "toast-success-bg": "#313244",
    "toast-error-bg": "#313244", "toast-fg": "#cdd6f4",
    "xterm-black": "#45475a", "xterm-red": "#f38ba8", "xterm-green": "#a6e3a1",
    "xterm-yellow": "#f9e2af", "xterm-blue": "#89b4fa", "xterm-magenta": "#cba6f7",
    "xterm-cyan": "#94e2d5", "xterm-white": "#bac2de",
    "xterm-bright-black": "#585b70", "xterm-bright-red": "#f38ba8",
    "xterm-bright-green": "#a6e3a1", "xterm-bright-yellow": "#f9e2af",
    "xterm-bright-blue": "#89b4fa", "xterm-bright-magenta": "#cba6f7",
    "xterm-bright-cyan": "#94e2d5", "xterm-bright-white": "#a6adc8",
  },
};

const SOLARIZED_LIGHT: ThemePreset = {
  id: "solarized-light", name: "Solarized Light", isDark: false,
  colors: {
    "bg-primary": "#fdf6e3", "bg-secondary": "#eee8d5", "bg-tertiary": "#e8e1cc",
    "bg-hover": "#ddd6c1", "bg-active": "#d3ccb7", "bg-input": "#eee8d5",
    "bg-dropdown": "#eee8d5", "bg-surface": "#fdf6e3", "bg-elevated": "#eee8d5",
    "text-primary": "#3b4f56", "text-secondary": "#546b73", "text-muted": "#6e7e7e",
    "text-accent": "#1a6599", "text-bright": "#073642",
    "border-primary": "#ddd6c1", "border-active": "#1a6599", "border-inactive": "#ddd6c1",
    "accent-primary": "#1a6599", "accent-hover": "#2aa198", "accent-focus": "#1a6599",
    "accent-warm": "#b58900",
    "activity-bar-bg": "#eee8d5", "activity-bar-fg": "#073642",
    "activity-bar-inactive": "#546b73", "activity-bar-badge": "#1a6599",
    "sidebar-bg": "#fdf6e3", "sidebar-header-bg": "#eee8d5", "sidebar-header-fg": "#546b73",
    "sidebar-item-hover": "#eee8d5", "sidebar-item-active": "#ddd6c1",
    "tab-active-bg": "#fdf6e3", "tab-active-fg": "#073642", "tab-inactive-bg": "#eee8d5",
    "tab-inactive-fg": "#546b73", "tab-border": "#ddd6c1", "tab-active-border-top": "#1a6599",
    "terminal-bg": "#fdf6e3", "terminal-fg": "#3b4f56", "terminal-cursor": "#1a6599",
    "statusbar-bg": "#eee8d5", "statusbar-fg": "#3b4f56", "statusbar-no-folder": "#6c71c4",
    "git-modified": "#7d5c00", "git-added": "#496800", "git-deleted": "#b82523",
    "git-renamed": "#496800", "git-untracked": "#2aa198", "git-conflicted": "#cb4b16",
    "git-staged": "#496800", "git-staged-modified": "#7d5c00",
    "dialog-bg": "#eee8d5", "dialog-border": "#ddd6c1",
    "toast-info-bg": "#eee8d5", "toast-success-bg": "#eee8d5",
    "toast-error-bg": "#eee8d5", "toast-fg": "#073642",
    "xterm-black": "#073642", "xterm-red": "#dc322f", "xterm-green": "#859900",
    "xterm-yellow": "#b58900", "xterm-blue": "#268bd2", "xterm-magenta": "#d33682",
    "xterm-cyan": "#2aa198", "xterm-white": "#eee8d5",
    "xterm-bright-black": "#002b36", "xterm-bright-red": "#cb4b16",
    "xterm-bright-green": "#586e75", "xterm-bright-yellow": "#657b83",
    "xterm-bright-blue": "#839496", "xterm-bright-magenta": "#6c71c4",
    "xterm-bright-cyan": "#93a1a1", "xterm-bright-white": "#fdf6e3",
  },
};

const TOKYO_NIGHT: ThemePreset = {
  id: "tokyo-night", name: "Tokyo Night", isDark: true,
  colors: {
    "bg-primary": "#1a1b26", "bg-secondary": "#16161e", "bg-tertiary": "#24283b",
    "bg-hover": "#292e42", "bg-active": "#33467c", "bg-input": "#1a1b26",
    "bg-dropdown": "#24283b", "bg-surface": "#16161e", "bg-elevated": "#292e42",
    "text-primary": "#a9b1d6", "text-secondary": "#7982b0", "text-muted": "#656d90",
    "text-accent": "#7aa2f7", "text-bright": "#c0caf5",
    "border-primary": "#24283b", "border-active": "#7aa2f7", "border-inactive": "#24283b",
    "accent-primary": "#7aa2f7", "accent-hover": "#89b4fa", "accent-focus": "#7aa2f7",
    "accent-warm": "#e0af68",
    "activity-bar-bg": "#16161e", "activity-bar-fg": "#c0caf5",
    "activity-bar-inactive": "#7982b0", "activity-bar-badge": "#7aa2f7",
    "sidebar-bg": "#1a1b26", "sidebar-header-bg": "#16161e", "sidebar-header-fg": "#7982b0",
    "sidebar-item-hover": "#24283b", "sidebar-item-active": "#292e42",
    "tab-active-bg": "#1a1b26", "tab-active-fg": "#c0caf5", "tab-inactive-bg": "#16161e",
    "tab-inactive-fg": "#7982b0", "tab-border": "#24283b", "tab-active-border-top": "#7aa2f7",
    "terminal-bg": "#1a1b26", "terminal-fg": "#a9b1d6", "terminal-cursor": "#c0caf5",
    "statusbar-bg": "#16161e", "statusbar-fg": "#a9b1d6", "statusbar-no-folder": "#9d7cd8",
    "git-modified": "#e0af68", "git-added": "#9ece6a", "git-deleted": "#f7768e",
    "git-renamed": "#9ece6a", "git-untracked": "#73daca", "git-conflicted": "#f7768e",
    "git-staged": "#9ece6a", "git-staged-modified": "#e0af68",
    "dialog-bg": "#24283b", "dialog-border": "#292e42",
    "toast-info-bg": "#24283b", "toast-success-bg": "#24283b",
    "toast-error-bg": "#24283b", "toast-fg": "#c0caf5",
    "xterm-black": "#15161e", "xterm-red": "#f7768e", "xterm-green": "#9ece6a",
    "xterm-yellow": "#e0af68", "xterm-blue": "#7aa2f7", "xterm-magenta": "#bb9af7",
    "xterm-cyan": "#7dcfff", "xterm-white": "#a9b1d6",
    "xterm-bright-black": "#414868", "xterm-bright-red": "#f7768e",
    "xterm-bright-green": "#9ece6a", "xterm-bright-yellow": "#e0af68",
    "xterm-bright-blue": "#7aa2f7", "xterm-bright-magenta": "#bb9af7",
    "xterm-bright-cyan": "#7dcfff", "xterm-bright-white": "#c0caf5",
  },
};

const ALL_PRESETS: ThemePreset[] = [OBSIDIAN_DARK, NORD_DARK, CATPPUCCIN_MOCHA, SOLARIZED_LIGHT, TOKYO_NIGHT];

// ── Theme Engine ─────────────────────────────────────

class ThemeEngine {
  private state: ThemeState = { activePreset: "obsidian-dark", overrides: {} };
  private presets = new Map<string, ThemePreset>();
  private persistTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    for (const p of ALL_PRESETS) this.presets.set(p.id, p);
  }

  getPresets(): ThemePreset[] { return ALL_PRESETS; }
  getActivePresetId(): string { return this.state.activePreset; }
  getActivePreset(): ThemePreset { return this.presets.get(this.state.activePreset) ?? OBSIDIAN_DARK; }
  getOverrides(): Partial<ThemeColors> { return { ...this.state.overrides }; }
  hasOverride(key: ThemeColorKey): boolean { return key in this.state.overrides; }

  getEffectiveColor(key: ThemeColorKey): string {
    return (this.state.overrides as Record<string, string>)[key]
      ?? this.getActivePreset().colors[key];
  }

  // ── Apply ──────────────────────────────────────

  applyTheme(): void {
    const preset = this.getActivePreset();
    const root = document.documentElement;
    const effective: Record<string, string> = {};

    // Apply base colors
    for (const key of Object.keys(preset.colors) as ThemeColorKey[]) {
      if (key.startsWith("xterm-")) continue; // xterm colors don't map to CSS vars
      const value = this.getEffectiveColor(key);
      effective[key] = value;
      root.style.setProperty(`--${key}`, value);
    }

    // Apply derived colors
    const derived = computeDerived(effective, preset.isDark);
    for (const [key, value] of Object.entries(derived)) {
      root.style.setProperty(`--${key}`, value);
    }

    // Sync xterm terminals
    this.updateAllTerminals();
  }

  setPreset(id: string): void {
    if (!this.presets.has(id)) return;
    this.state.activePreset = id;
    this.state.overrides = {};
    this.applyTheme();
    this.schedulePersist();
  }

  setColorOverride(key: ThemeColorKey, value: string): void {
    if (!isValidHex(value)) return;
    (this.state.overrides as Record<string, string>)[key] = value;
    this.applyTheme();
    this.schedulePersist();
  }

  clearSingleOverride(key: ThemeColorKey): void {
    delete (this.state.overrides as Record<string, string>)[key];
    this.applyTheme();
    this.schedulePersist();
  }

  clearOverrides(): void {
    this.state.overrides = {};
    this.applyTheme();
    this.schedulePersist();
  }

  // ── Persistence ────────────────────────────────

  async loadFromStorage(): Promise<void> {
    try {
      const [preset, overridesJson] = await ipc.getTheme();
      if (preset && this.presets.has(preset)) {
        this.state.activePreset = preset;
      }
      if (overridesJson) {
        try {
          const parsed = JSON.parse(overridesJson);
          if (typeof parsed === "object" && parsed !== null) {
            this.state.overrides = parsed;
          }
        } catch { /* ignore bad JSON */ }
      }
    } catch {
      // Storage not available yet, use defaults
    }
    this.applyTheme();
  }

  private schedulePersist(): void {
    if (this.persistTimer) clearTimeout(this.persistTimer);
    this.persistTimer = setTimeout(() => {
      ipc.setTheme(
        this.state.activePreset,
        JSON.stringify(this.state.overrides),
      ).catch(() => {});
    }, 300);
  }

  // ── Import / Export ────────────────────────────

  exportTheme(): string {
    return JSON.stringify({
      preset: this.state.activePreset,
      overrides: this.state.overrides,
    }, null, 2);
  }

  importTheme(json: string): void {
    try {
      const data = JSON.parse(json);
      if (data.preset && this.presets.has(data.preset)) {
        this.state.activePreset = data.preset;
      }
      if (typeof data.overrides === "object" && data.overrides !== null) {
        // Validate each value is a valid hex
        const clean: Record<string, string> = {};
        for (const [k, v] of Object.entries(data.overrides)) {
          if (typeof v === "string" && isValidHex(v)) clean[k] = v;
        }
        this.state.overrides = clean as Partial<ThemeColors>;
      }
      this.applyTheme();
      this.schedulePersist();
    } catch { /* ignore invalid JSON */ }
  }

  // ── Terminal sync ──────────────────────────────

  buildXtermTheme(): Record<string, string> {
    const c = (key: ThemeColorKey) => this.getEffectiveColor(key);
    return {
      background: c("terminal-bg"),
      foreground: c("terminal-fg"),
      cursor: c("terminal-cursor"),
      cursorAccent: c("terminal-bg"),
      selectionBackground: hexToRgba(c("terminal-cursor"), 0.18),
      selectionForeground: c("text-bright"),
      black: c("xterm-black"),
      red: c("xterm-red"),
      green: c("xterm-green"),
      yellow: c("xterm-yellow"),
      blue: c("xterm-blue"),
      magenta: c("xterm-magenta"),
      cyan: c("xterm-cyan"),
      white: c("xterm-white"),
      brightBlack: c("xterm-bright-black"),
      brightRed: c("xterm-bright-red"),
      brightGreen: c("xterm-bright-green"),
      brightYellow: c("xterm-bright-yellow"),
      brightBlue: c("xterm-bright-blue"),
      brightMagenta: c("xterm-bright-magenta"),
      brightCyan: c("xterm-bright-cyan"),
      brightWhite: c("xterm-bright-white"),
    };
  }

  updateAllTerminals(): void {
    // Dynamic import to avoid circular dependency (theme ↔ terminal-panel)
    import("./components/terminal-panel").then(({ terminals }) => {
      const theme = this.buildXtermTheme();
      for (const instance of terminals.values()) {
        if (instance.opened) {
          instance.terminal.options.theme = theme;
        }
      }
    }).catch(() => { /* terminal-panel not loaded yet */ });
  }
}

export const themeEngine = new ThemeEngine();
