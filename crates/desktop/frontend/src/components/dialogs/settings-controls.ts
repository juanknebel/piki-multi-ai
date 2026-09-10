// Small DOM helpers shared by the Settings tabs (general / appearance /
// shortcuts): a label + control grid, a checkbox row and the section
// contract every tab implements. Styles: styles/dialog-settings.css.

export interface SettingsSection {
  el: HTMLElement;
  /** Back to the defaults for THIS tab (footer "Reset this tab" and the
   *  global Restore Defaults both call it). */
  reset(): void | Promise<void>;
  /** Move keyboard focus to the tab's first control. */
  focus(): void;
}

export function settingsSection(title: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "settings-section";
  const h = document.createElement("div");
  h.className = "settings-section-title";
  h.textContent = title;
  el.appendChild(h);
  return el;
}

export function settingsHint(text: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "settings-hint";
  el.textContent = text;
  return el;
}

/** `label | control` rows; returns the grid and a `row()` adder. */
export function settingsGrid(parent: HTMLElement) {
  const grid = document.createElement("div");
  grid.className = "settings-grid";
  parent.appendChild(grid);
  const row = (label: string, control: HTMLElement, forId?: string) => {
    const l = document.createElement("label");
    l.className = "settings-grid-label";
    l.textContent = label;
    if (forId) l.htmlFor = forId;
    const c = document.createElement("div");
    c.className = "settings-grid-control";
    c.appendChild(control);
    grid.appendChild(l);
    grid.appendChild(c);
    return c;
  };
  return { grid, row };
}

export function settingsCheckbox(text: string, checked: boolean, onChange: (on: boolean) => void) {
  const label = document.createElement("label");
  label.className = "settings-check";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => onChange(input.checked));
  label.appendChild(input);
  label.appendChild(document.createTextNode(text));
  return { label, input };
}

/** "from config.toml" / "set here" badge next to a shared setting. */
export function sourceBadge(overridden: boolean): HTMLElement {
  const b = document.createElement("span");
  b.className = "settings-source";
  b.dataset.source = overridden ? "override" : "config";
  b.textContent = overridden ? "set here" : "from config.toml";
  b.title = overridden
    ? "Chosen in this dialog — overrides config.toml (stored in the piki database, shared with the TUI)"
    : "Following [sessions] / [notifications] in config.toml";
  return b;
}

export { escapeHtml } from "../confirm";
