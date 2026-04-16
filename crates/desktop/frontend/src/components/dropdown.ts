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
  trigger.setAttribute("role", "combobox");
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-expanded", "false");
  if (extraStyle) trigger.style.cssText = extraStyle;

  const initialIdx = Math.max(0, options.findIndex((o) => o.value === initial));
  let currentIdx = initialIdx;
  const activeOpt = options[currentIdx];
  let currentValue = activeOpt?.value ?? "";
  let currentData = activeOpt?.data ?? {};
  let currentLabel = activeOpt?.label ?? "";

  trigger.innerHTML = `<span class="dialog-dropdown-text">${esc(currentLabel)}</span><span class="dialog-dropdown-arrow">▾</span>`;
  container.appendChild(trigger);

  function applyIndex(idx: number, fireChange: boolean) {
    if (idx < 0 || idx >= options.length) return;
    const opt = options[idx];
    currentIdx = idx;
    currentValue = opt.value;
    currentData = opt.data ?? {};
    currentLabel = opt.label;
    const text = trigger.querySelector(".dialog-dropdown-text");
    if (text) text.textContent = opt.label;
    if (fireChange) {
      container.dispatchEvent(new Event("change", { bubbles: true }));
    }
  }

  let openState: {
    list: HTMLElement;
    highlight: number;
    items: HTMLButtonElement[];
    closeListener: (ev: MouseEvent) => void;
  } | null = null;

  function setHighlight(next: number) {
    if (!openState) return;
    const len = openState.items.length;
    if (len === 0) return;
    const clamped = ((next % len) + len) % len;
    openState.items.forEach((el, i) => {
      el.classList.toggle("highlight", i === clamped);
    });
    openState.highlight = clamped;
    openState.items[clamped]?.scrollIntoView({ block: "nearest" });
  }

  function closeList() {
    if (!openState) return;
    openState.list.remove();
    document.removeEventListener("click", openState.closeListener);
    openState = null;
    trigger.setAttribute("aria-expanded", "false");
  }

  function openList() {
    if (openState) { closeList(); return; }

    const list = document.createElement("div");
    list.className = "dialog-dropdown-list";
    list.setAttribute("role", "listbox");

    const items: HTMLButtonElement[] = [];
    for (let i = 0; i < options.length; i++) {
      const opt = options[i];
      const item = document.createElement("button");
      item.className = `dialog-dropdown-item${opt.value === currentValue ? " active" : ""}`;
      item.type = "button";
      item.setAttribute("role", "option");
      item.textContent = opt.label;
      item.addEventListener("click", (ev) => {
        ev.stopPropagation();
        applyIndex(i, true);
        closeList();
        trigger.focus();
      });
      item.addEventListener("mousemove", () => {
        setHighlight(i);
      });
      list.appendChild(item);
      items.push(item);
    }

    container.appendChild(list);

    const closeListener = (ev: MouseEvent) => {
      if (!list.contains(ev.target as Node) && ev.target !== trigger && !trigger.contains(ev.target as Node)) {
        closeList();
      }
    };
    setTimeout(() => document.addEventListener("click", closeListener), 0);

    openState = { list, highlight: currentIdx >= 0 ? currentIdx : 0, items, closeListener };
    setHighlight(openState.highlight);
    trigger.setAttribute("aria-expanded", "true");
  }

  trigger.addEventListener("click", (e) => { e.stopPropagation(); openList(); });

  trigger.addEventListener("keydown", (e) => {
    const key = e.key;
    if (openState) {
      switch (key) {
        case "ArrowDown":
          e.preventDefault();
          setHighlight(openState.highlight + 1);
          return;
        case "ArrowUp":
          e.preventDefault();
          setHighlight(openState.highlight - 1);
          return;
        case "Home":
          e.preventDefault();
          setHighlight(0);
          return;
        case "End":
          e.preventDefault();
          setHighlight(options.length - 1);
          return;
        case "Enter":
        case " ":
        case "Tab": {
          e.preventDefault();
          const idx = openState.highlight;
          closeList();
          applyIndex(idx, true);
          return;
        }
        case "Escape":
          e.preventDefault();
          closeList();
          return;
      }
      return;
    }

    switch (key) {
      case "ArrowDown":
        e.preventDefault();
        if (currentIdx < options.length - 1) applyIndex(currentIdx + 1, true);
        return;
      case "ArrowUp":
        e.preventDefault();
        if (currentIdx > 0) applyIndex(currentIdx - 1, true);
        return;
      case "Home":
        e.preventDefault();
        applyIndex(0, true);
        return;
      case "End":
        e.preventDefault();
        applyIndex(options.length - 1, true);
        return;
      case "Enter":
      case " ":
      case "F4":
        e.preventDefault();
        openList();
        return;
    }
  });

  return {
    container,
    get value() { return currentValue; },
    set value(v: string) {
      const idx = options.findIndex((o) => o.value === v);
      if (idx >= 0) applyIndex(idx, false);
    },
    getData() { return { ...currentData }; },
    getLabel() { return currentLabel; },
  };
}
