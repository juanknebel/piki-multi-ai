// The ONE icon set for the desktop chrome: inline SVG on a 16px grid, drawn
// with `currentColor` so an icon follows the text colour of wherever it
// lands (status colours, hover states, themes — no baked fills). Every
// button, badge and status mark that used to be an emoji / dingbat glyph
// (which rendered differently per platform font and never matched the
// stroke weight of the SVG chevrons) goes through `icon()` now;
// `icons.test.ts` fails the build when one of those glyphs comes back.
//
// Usage — inside an `innerHTML` template, or as an element:
//
//   btn.innerHTML = icon("refresh");                         // aria-hidden
//   `${icon("branch")} ${escapeHtml(branch)}`                 // next to text
//   icon("dot", { class: "workspace-attention" })            // extra class
//   el.appendChild(iconEl("check", { label: "done" }));      // role=img
//
// Sizing is CSS (`styles/icons.css`): 1em square by default, so an icon is
// the size of the text around it; a container class may set its own
// width/height (the `.group-chevron` 10px box does). Stroke attributes live
// on the paths themselves, so a context rule such as `.status-item svg
// { fill: currentColor }` cannot fill a stroke icon by accident. Prose
// (toast bodies, confirm text, tooltips) stays plain text — icons are for
// chrome, not sentences.

/** Every icon name. Adding one: a new key in `ICONS` extends this union. */
export type IconName = keyof typeof ICONS;

export interface IconOpts {
  /** Extra class(es) on the `<svg>` (always after `icon icon-<name>`). */
  class?: string;
  /** Accessible name — makes the icon `role="img"`; without it the icon is
   *  `aria-hidden` and the surrounding button/title carries the meaning. */
  label?: string;
  /** Explicit box (`"12"`, `"1.2em"`); default 1em via CSS. */
  size?: string;
}

const STROKE = ' fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"';

/** Stroked path(s). */
function p(...ds: string[]): string {
  return ds.map((d) => `<path d="${d}"${STROKE}/>`).join("");
}

/** Filled disc. */
function disc(cx: number, cy: number, r: number): string {
  return `<circle cx="${cx}" cy="${cy}" r="${r}" fill="currentColor" stroke="none"/>`;
}

/** Stroked ring. */
function ring(cx: number, cy: number, r: number): string {
  return `<circle cx="${cx}" cy="${cy}" r="${r}"${STROKE}/>`;
}

/** Inner SVG markup per icon (16×16 viewBox). Keep strokes at 1.5 and
 *  shapes inside the 1.5–14.5 box so every icon reads at the same weight. */
export const ICONS = {
  /** ✓ done / commit / synced */
  check: p("M3.5 8.5l3 3 6-7"),
  /** ⚠ needs permission / detached */
  warning: p("M8 2.5l6 11H2z", "M8 6.5v3") + disc(8, 11.9, 0.8),
  /** 📁 cwd / project sub-directory */
  folder: p("M1.5 4.5a1 1 0 0 1 1-1h3.2l1.6 1.5h6.2a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z"),
  /** ⚙ manage / settings */
  gear:
    ring(8, 8, 2.5) +
    p("M8 1.5v2.2M8 12.3v2.2M1.5 8h2.2M12.3 8h2.2M3.4 3.4l1.55 1.55M11.05 11.05l1.55 1.55M3.4 12.6l1.55-1.55M11.05 4.95l1.55-1.55"),
  /** 👁 preview */
  eye: p("M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8z") + ring(8, 8, 2),
  /** ✕ error / close mark (the `×` text glyph stays on close buttons) */
  close: p("M4 4l8 8M12 4l-8 8"),
  /** ✏ the ONE edit / rename icon */
  pencil: p("M11.5 2.5l2 2-8 8-3 1 1-3z", "M9.5 4.5l2 2"),
  /** ⟲ discard / reset to preset */
  undo: p("M3 7h6.5a3.5 3.5 0 0 1 0 7H6", "M5.5 4.5L3 7l2.5 2.5"),
  /** ↺ restored from the session daemon */
  history: p("M2 8a6 6 0 1 0 6-6 6.5 6.5 0 0 0-4.5 1.83L2 5.33", "M2 2v3.33h3.33", "M8 4.67V8l2.67 1.33"),
  /** ↻ refresh / reload / restart */
  refresh: p(
    "M2 8a6 6 0 0 1 6-6 6.5 6.5 0 0 1 4.5 1.83L14 5.33",
    "M14 2v3.33h-3.33",
    "M14 8a6 6 0 0 1-6 6 6.5 6.5 0 0 1-4.5-1.83L2 10.67",
    "M5.33 10.67H2V14",
  ),
  /** ⋯ row / tab menu */
  more: disc(3.5, 8, 1.3) + disc(8, 8, 1.3) + disc(12.5, 8, 1.3),
  /** ● status dot: agent state, attention, dirty, current */
  dot: disc(8, 8, 4),
  /** ○ hollow dot: exited / dead */
  circle: ring(8, 8, 3.5),
  /** ▸ collapsed group / tree twisty (rotate 90° for expanded) */
  "chevron-right": p("M6 4l4 4-4 4"),
  /** ▾ dropdown trigger */
  "chevron-down": p("M4 6l4 4 4-4"),
  "arrow-up": p("M8 13V3", "M4 7l4-4 4 4"),
  "arrow-down": p("M8 3v10", "M4 9l4 4 4-4"),
  "arrow-left": p("M13 8H3", "M7 4L3 8l4 4"),
  "arrow-right": p("M3 8h10", "M9 4l4 4-4 4"),
  /** ⎇ git branch */
  branch: p("M4 2v8", "M12 6a6 6 0 0 1-6 6") + ring(12, 4, 2) + ring(4, 12, 2),
  /** ▷ running / attached */
  play: p("M4.5 3v10l8-5z"),
  /** ⏳ waiting for input */
  clock: ring(8, 8, 5.5) + p("M8 4.5V8l2.5 1.5"),
  /** ＋ dispatch / new */
  plus: p("M8 3v10", "M3 8h10"),
  /** ⇥ split pane right */
  "split-right": p("M2.5 3.5h11v9h-11z", "M8 3.5v9"),
  /** ⤓ split pane down */
  "split-down": p("M2.5 3.5h11v9h-11z", "M2.5 8h11"),
  /** ⌖ / ◎ reveal in Files, auto-reveal */
  locate: ring(8, 8, 3.5) + p("M8 1.5V4M8 12v2.5M1.5 8H4M12 8h2.5"),
  /** file-tree filter */
  search: ring(7, 7, 4.5) + p("M10.5 10.5l4 4"),
} as const;

export const ICON_NAMES = Object.keys(ICONS) as IconName[];

function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

/** Inline SVG markup for `name` (for `innerHTML` templates). */
export function icon(name: IconName, opts: IconOpts = {}): string {
  const cls = opts.class ? `icon icon-${name} ${opts.class}` : `icon icon-${name}`;
  const a11y = opts.label ? ` role="img" aria-label="${escapeAttr(opts.label)}"` : ' aria-hidden="true"';
  const size = opts.size ? ` width="${escapeAttr(opts.size)}" height="${escapeAttr(opts.size)}"` : "";
  return `<svg class="${cls}" viewBox="0 0 16 16"${size}${a11y} focusable="false">${ICONS[name]}</svg>`;
}

/** The same icon as a live `SVGElement` (for `appendChild`). */
export function iconEl(name: IconName, opts: IconOpts = {}): SVGElement {
  const tpl = document.createElement("template");
  tpl.innerHTML = icon(name, opts);
  return tpl.content.firstElementChild as SVGElement;
}
