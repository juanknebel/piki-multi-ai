// Pure colour math for the theme engine — no DOM, no IPC — so it can be unit
// tested (theme-derive.test.ts) and reused by anything that needs a colour
// derived from the palette. `computeDerived` produces every CSS token that is
// NOT stored in a preset but computed from the base colours on each apply
// (glows, muted tints, `--on-accent`, `::selection`, file-icon colours…).
// The static fallbacks for the same tokens live in styles/variables.css.

export function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [parseInt(h.substring(0, 2), 16), parseInt(h.substring(2, 4), 16), parseInt(h.substring(4, 6), 16)];
}

export function rgbToHex(r: number, g: number, b: number): string {
  const c = (v: number) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

export function hexToRgba(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

export function hexToGlow(hex: string, alpha: number, radius: number = 10): string {
  return `0 0 ${radius}px ${hexToRgba(hex, alpha)}`;
}

/** WCAG relative luminance, 0 (black) … 1 (white). */
export function relativeLuminance(hex: string): number {
  const lin = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  const [r, g, b] = hexToRgb(hex);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/**
 * Black or white, whichever contrasts more with `bg`. The threshold is the
 * luminance where the WCAG contrast ratio against black equals the one
 * against white: (L + 0.05) / 0.05 = 1.05 / (L + 0.05)  →  L ≈ 0.179.
 */
export function onColorFor(bg: string): "#000" | "#fff" {
  return relativeLuminance(bg) > 0.179 ? "#000" : "#fff";
}

/** Linear sRGB-space mix: t = 0 → a, t = 1 → b. */
export function mixHex(a: string, b: string, t: number): string {
  const [ar, ag, ab] = hexToRgb(a);
  const [br, bg, bb] = hexToRgb(b);
  return rgbToHex(ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t);
}

export type ThemeTone = "dark" | "light";

/**
 * Whether a palette reads as dark or light, from the luminance of its main
 * background — the same L ≈ 0.179 threshold `onColorFor` uses, i.e. "light"
 * means black text contrasts better on it than white. theme.ts stamps the
 * result as `data-theme-tone` on <html>; variables.css flips scrims and
 * shadows on it, reset.css drops the noise texture in light tone.
 */
export function themeTone(colors: Record<string, string>): ThemeTone {
  return onColorFor(colors["bg-primary"] || "#0b0f14") === "#000" ? "light" : "dark";
}

/** Static Obsidian swatches — the first-paint defaults in variables.css. */
const KANBAN_FALLBACK = [
  "#39bae6", "#e6a730", "#7b61ff", "#3fb950",
  "#f85149", "#f778ba", "#d2a8ff", "#79c0ff",
  "#56d4dd", "#a5d6ff", "#ffa657", "#ff7b72",
  "#8b949e", "#c9d1d9", "#e3b341", "#7ee787",
];

/** Palette keys that make good column colours, most distinctive first. */
const KANBAN_CANDIDATES = [
  "accent-primary", "accent-warm", "xterm-magenta", "git-added",
  "git-deleted", "xterm-red", "xterm-green", "xterm-yellow",
  "xterm-blue", "xterm-cyan", "xterm-bright-red", "xterm-bright-green",
  "xterm-bright-yellow", "xterm-bright-blue", "xterm-bright-magenta", "xterm-bright-cyan",
  "accent-hover", "git-untracked", "git-conflicted", "git-modified",
  "text-secondary", "text-primary", "text-bright", "text-muted",
];

/**
 * Sixteen distinct swatches for the kanban column picker, drawn from the
 * accent + ANSI palette so the board follows any preset. Exact duplicates
 * (Obsidian's blue == cyan == accent) are skipped; missing keys fall back
 * to the Obsidian swatch in the same slot.
 */
export function kanbanPalette(colors: Record<string, string>): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const push = (hex: string) => {
    const k = hex.toLowerCase();
    if (out.length < 16 && !seen.has(k)) { seen.add(k); out.push(k); }
  };
  for (const key of KANBAN_CANDIDATES) {
    const v = colors[key];
    if (v) push(v);
  }
  for (const hex of KANBAN_FALLBACK) push(hex);
  return out;
}

/**
 * Derived tokens, computed from the effective base colours (all ThemeColors
 * keys, xterm ANSI included). Keys are CSS var names without the `--`.
 */
export function computeDerived(colors: Record<string, string>, isDark: boolean): Record<string, string> {
  const c = (key: string, fallback: string) => colors[key] || fallback;
  const ap = c("accent-primary", "#39bae6");
  const aw = c("accent-warm", "#e6a730");
  const tc = c("terminal-cursor", ap);
  const badge = c("activity-bar-badge", ap);
  const red = c("xterm-red", "#f85149");
  const yellow = c("xterm-yellow", "#d4a12e");
  const magenta = c("xterm-magenta", "#bc8cff");
  const swatches = kanbanPalette(colors);
  const kanban: Record<string, string> = {
    "kanban-col-todo": ap,
    "kanban-col-in-progress": aw,
    "kanban-col-in-review": magenta,
    "kanban-col-done": c("git-added", "#3fb950"),
  };
  swatches.forEach((hex, i) => { kanban[`kanban-swatch-${i + 1}`] = hex; });

  return {
    ...kanban,
    "accent-muted": hexToRgba(ap, 0.12),
    "accent-glow": hexToGlow(ap, 0.3),
    "accent-warm-muted": hexToRgba(aw, 0.12),
    "accent-warm-glow": hexToGlow(aw, 0.2),
    "sidebar-item-focus": hexToRgba(ap, 0.08),
    "scrollbar-thumb": hexToRgba(ap, 0.08),
    "scrollbar-thumb-hover": hexToRgba(ap, 0.22),
    "terminal-selection": hexToRgba(tc, 0.18),
    "statusbar-item-hover": hexToRgba(ap, 0.06),
    "border-subtle": isDark ? "rgba(255, 255, 255, 0.03)" : "rgba(0, 0, 0, 0.06)",
    "dialog-shadow": isDark ? "rgba(0, 0, 0, 0.65)" : "rgba(0, 0, 0, 0.2)",
    // Text over accent / badge backgrounds, by luminance (a dark accent such
    // as Solarized's blue gets white text, a light one black).
    "on-accent": onColorFor(ap),
    "on-error": onColorFor(c("git-deleted", "#f85149")),
    "activity-bar-badge-fg": onColorFor(badge),
    // Secondary hue for gradients / wishlist / JSON booleans / "in review".
    "accent-alt": magenta,
    "activity-bar-badge-glow": hexToGlow(badge, 0.3),
    "selection-bg": hexToRgba(ap, 0.25),
    // File-type icon colours from the ANSI + text palette, so the tree
    // follows any preset instead of a fixed Obsidian set.
    "icon-rust": mixHex(red, yellow, 0.35),
    "icon-ts": c("xterm-blue", "#3178c6"),
    "icon-js": c("xterm-bright-yellow", "#e8d44d"),
    "icon-py": c("xterm-bright-blue", "#4b8bbe"),
    "icon-go": c("xterm-cyan", "#4fc3dc"),
    "icon-web": mixHex(red, yellow, 0.5),
    "icon-data": c("xterm-green", "#6cc04a"),
    "icon-doc": c("text-secondary", "#8b96a3"),
    "icon-asset": c("xterm-magenta", "#b07cd6"),
    "icon-muted": c("text-muted", "#5d7080"),
    "icon-default": c("sidebar-header-fg", "#768390"),
    "icon-folder": c("text-muted", "#5d7080"),
  };
}
