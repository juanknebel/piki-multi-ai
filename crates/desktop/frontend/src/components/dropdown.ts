/**
 * Custom dropdown replacement for native <select>.
 * Native <select> option elements ignore CSS styling in WebKit/Tauri,
 * causing unreadable text (dark on dark) in dark themes.
 */

export interface DropdownOption {
  value: string;
  label: string;
  data?: Record<string, string>;
}

export interface DropdownHandle {
  /** The DOM element to mount */
  container: HTMLElement;
  /** Current selected value */
  value: string;
  /** Get the data attributes of the current selection */
  getData(): Record<string, string>;
  /** Get the label of the current selection */
  getLabel(): string;
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/**
 * Create a custom dropdown that matches dialog-select styling
 * but uses a fully-styled list instead of native options.
 */
export function createDropdown(
  options: DropdownOption[],
  initial: string,
  extraStyle?: string,
): DropdownHandle {
  const container = document.createElement("div");
  container.className = "dialog-dropdown";

  const trigger = document.createElement("button");
  trigger.className = "dialog-dropdown-trigger";
  trigger.type = "button";
  if (extraStyle) trigger.style.cssText = extraStyle;

  const activeOpt = options.find((o) => o.value === initial) ?? options[0];
  let currentValue = activeOpt?.value ?? "";
  let currentData = activeOpt?.data ?? {};
  let currentLabel = activeOpt?.label ?? "";

  trigger.innerHTML = `<span class="dialog-dropdown-text">${esc(currentLabel)}</span><span class="dialog-dropdown-arrow">▾</span>`;
  container.appendChild(trigger);

  function openList() {
    const existing = container.querySelector(".dialog-dropdown-list");
    if (existing) { existing.remove(); return; }

    const list = document.createElement("div");
    list.className = "dialog-dropdown-list";

    for (const opt of options) {
      const item = document.createElement("button");
      item.className = `dialog-dropdown-item${opt.value === currentValue ? " active" : ""}`;
      item.type = "button";
      item.textContent = opt.label;
      item.addEventListener("click", (ev) => {
        ev.stopPropagation();
        currentValue = opt.value;
        currentData = opt.data ?? {};
        currentLabel = opt.label;
        trigger.querySelector(".dialog-dropdown-text")!.textContent = opt.label;
        list.remove();
        container.dispatchEvent(new Event("change", { bubbles: true }));
      });
      list.appendChild(item);
    }

    container.appendChild(list);

    const closeList = (ev: MouseEvent) => {
      if (!list.contains(ev.target as Node) && ev.target !== trigger) {
        list.remove();
        document.removeEventListener("click", closeList);
      }
    };
    setTimeout(() => document.addEventListener("click", closeList), 0);
  }

  trigger.addEventListener("click", (e) => { e.stopPropagation(); openList(); });

  return {
    container,
    get value() { return currentValue; },
    set value(v: string) {
      const opt = options.find((o) => o.value === v);
      if (opt) {
        currentValue = v;
        currentData = opt.data ?? {};
        currentLabel = opt.label;
        trigger.querySelector(".dialog-dropdown-text")!.textContent = opt.label;
      }
    },
    getData() { return { ...currentData }; },
    getLabel() { return currentLabel; },
  };
}
