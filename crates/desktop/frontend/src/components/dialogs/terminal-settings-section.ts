// Settings ▸ Terminal — the section the settings dialog mounts between Shell
// and Keyboard Shortcuts. Every control applies live: `setTerminalSettings`
// persists the diff from the defaults and `themeEngine.updateAllTerminals()`
// pushes the new options to every open xterm (and a terminal created later
// reads the same settings in `createTerminal`). Font size is the base at
// zoom 1 — the "effective" hint shows the composition with the UI zoom.

import { createDropdown } from "../dropdown";
import { themeEngine } from "../../theme";
import { getUiZoom } from "../../ui-zoom";
import { formatZoom } from "../../zoom";
import {
  CURSOR_STYLES,
  DEFAULT_TERMINAL_SETTINGS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  LINE_HEIGHT_CHOICES,
  SCROLLBACK_CHOICES,
  effectiveTerminalFontSize,
  formatScrollback,
  getTerminalSettings,
  resetTerminalSettings,
  setTerminalSettings,
  type CursorStyle,
  type TerminalSettings,
} from "../../terminal-settings";

export interface TerminalSettingsSection {
  el: HTMLElement;
  /** Back to the defaults (the dialog's "Restore Defaults" button). */
  reset(): void;
}

const CURSOR_LABELS: Record<CursorStyle, string> = { block: "Block", underline: "Underline", bar: "Bar" };

export function buildTerminalSettingsSection(): TerminalSettingsSection {
  const el = document.createElement("div");
  el.className = "settings-section";
  el.innerHTML = `<div class="settings-section-title">Terminal</div>`;

  const grid = document.createElement("div");
  grid.className = "term-set-grid";
  el.appendChild(grid);

  let current = getTerminalSettings();

  const apply = (patch: Partial<TerminalSettings>) => {
    current = setTerminalSettings(patch);
    themeEngine.updateAllTerminals();
    effective.textContent = effectiveText();
  };

  const row = (label: string, control: HTMLElement) => {
    const l = document.createElement("label");
    l.className = "term-set-label";
    l.textContent = label;
    const c = document.createElement("div");
    c.className = "term-set-control";
    c.appendChild(control);
    grid.appendChild(l);
    grid.appendChild(c);
    return c;
  };

  // Font family — free text (a CSS font-family list); empty = theme mono.
  const fontInput = document.createElement("input");
  fontInput.type = "text";
  fontInput.className = "term-set-input term-set-input--font";
  fontInput.placeholder = "Theme font (--font-mono)";
  fontInput.value = current.fontFamily;
  fontInput.setAttribute("aria-label", "Terminal font family");
  let fontTimer: ReturnType<typeof setTimeout> | null = null;
  fontInput.addEventListener("input", () => {
    if (fontTimer) clearTimeout(fontTimer);
    fontTimer = setTimeout(() => apply({ fontFamily: fontInput.value.trim() }), 400);
  });
  row("Font family", fontInput);

  // Font size — base px at zoom 1 with the effective size next to it.
  const sizeInput = document.createElement("input");
  sizeInput.type = "number";
  sizeInput.className = "term-set-input term-set-input--num";
  sizeInput.min = String(FONT_SIZE_MIN);
  sizeInput.max = String(FONT_SIZE_MAX);
  sizeInput.step = "1";
  sizeInput.value = String(current.fontSize);
  sizeInput.setAttribute("aria-label", "Terminal font size");
  const effective = document.createElement("span");
  effective.className = "term-set-effective";
  const effectiveText = () => {
    const zoom = getUiZoom();
    const px = effectiveTerminalFontSize(current, zoom);
    return zoom === 1 ? `px` : `px × ${formatZoom(zoom)} zoom = ${px}px`;
  };
  effective.textContent = effectiveText();
  sizeInput.addEventListener("change", () => {
    apply({ fontSize: Number(sizeInput.value) });
    sizeInput.value = String(current.fontSize);
  });
  const sizeCell = row("Font size", sizeInput);
  sizeCell.appendChild(effective);

  // Line height / scrollback / cursor style — enumerations, never <select>.
  const lineHeight = createDropdown(
    LINE_HEIGHT_CHOICES.map((v) => ({ value: String(v), label: v.toFixed(2) })),
    String(current.lineHeight),
  );
  lineHeight.container.addEventListener("change", () => apply({ lineHeight: Number(lineHeight.value) }));
  row("Line height", lineHeight.container);

  const scrollback = createDropdown(
    SCROLLBACK_CHOICES.map((v) => ({ value: String(v), label: formatScrollback(v) })),
    String(current.scrollback),
  );
  scrollback.container.addEventListener("change", () => apply({ scrollback: Number(scrollback.value) }));
  row("Scrollback", scrollback.container);

  const cursorStyle = createDropdown(
    CURSOR_STYLES.map((v) => ({ value: v, label: CURSOR_LABELS[v] })),
    current.cursorStyle,
  );
  cursorStyle.container.addEventListener("change", () => apply({ cursorStyle: cursorStyle.value as CursorStyle }));
  const cursorCell = row("Cursor", cursorStyle.container);

  const blink = checkbox("Blink", current.cursorBlink, (on) => apply({ cursorBlink: on }));
  cursorCell.appendChild(blink.label);

  const copyOnSelect = checkbox("Copy selected text to the clipboard (one copy per selection)", current.copyOnSelect, (on) =>
    apply({ copyOnSelect: on }),
  );
  row("Copy on select", copyOnSelect.label);

  const hint = document.createElement("div");
  hint.className = "settings-hint";
  hint.textContent =
    "Applies to every open terminal immediately. Font size is the base; the UI zoom (Ctrl+= / Ctrl+-) multiplies it.";
  el.appendChild(hint);

  return {
    el,
    reset() {
      current = resetTerminalSettings();
      themeEngine.updateAllTerminals();
      fontInput.value = DEFAULT_TERMINAL_SETTINGS.fontFamily;
      sizeInput.value = String(DEFAULT_TERMINAL_SETTINGS.fontSize);
      lineHeight.value = String(DEFAULT_TERMINAL_SETTINGS.lineHeight);
      scrollback.value = String(DEFAULT_TERMINAL_SETTINGS.scrollback);
      cursorStyle.value = DEFAULT_TERMINAL_SETTINGS.cursorStyle;
      blink.input.checked = DEFAULT_TERMINAL_SETTINGS.cursorBlink;
      copyOnSelect.input.checked = DEFAULT_TERMINAL_SETTINGS.copyOnSelect;
      effective.textContent = effectiveText();
    },
  };
}

function checkbox(text: string, checked: boolean, onChange: (on: boolean) => void) {
  const label = document.createElement("label");
  label.className = "term-set-check";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => onChange(input.checked));
  label.appendChild(input);
  label.appendChild(document.createTextNode(text));
  return { label, input };
}
