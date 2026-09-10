// Shared right-click / "more" menu: fixed-position popover clamped to the
// viewport, closed by Esc, click-outside, window blur or picking an item;
// Arrow/Home/End move focus between items, Enter/Space activate. Focus
// returns to where it was when the menu closes.
//
// Use this for every contextual menu (file tree, tabs, workspaces, terminal)
// — never hand-roll another popover.

export interface CtxItem {
  label?: string;
  action?: () => void;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
}

let current: (() => void) | null = null;

/** Close the open context menu, if any. */
export function closeContextMenu() {
  current?.();
}

export function openContextMenu(x: number, y: number, items: CtxItem[]) {
  closeContextMenu();
  const restoreFocus = document.activeElement as HTMLElement | null;

  const menu = document.createElement("div");
  menu.className = "ctx-menu";
  menu.setAttribute("role", "menu");

  const buttons: HTMLButtonElement[] = [];
  for (const it of items) {
    if (it.separator) {
      const s = document.createElement("div");
      s.className = "ctx-menu-sep";
      s.setAttribute("role", "separator");
      menu.appendChild(s);
      continue;
    }
    const b = document.createElement("button");
    b.type = "button";
    b.className = `ctx-menu-item${it.danger ? " danger" : ""}`;
    b.setAttribute("role", "menuitem");
    b.tabIndex = -1;
    b.textContent = it.label ?? "";
    if (it.disabled) b.disabled = true;
    b.addEventListener("click", () => {
      close();
      it.action?.();
    });
    // Hovering moves the keyboard cursor too, like native menus.
    b.addEventListener("mouseenter", () => b.focus());
    menu.appendChild(b);
    if (!it.disabled) buttons.push(b);
  }

  const close = () => {
    if (current !== close) return;
    current = null;
    menu.remove();
    document.removeEventListener("mousedown", onDown, true);
    document.removeEventListener("keydown", onKey, true);
    window.removeEventListener("blur", close);
    if (restoreFocus && document.contains(restoreFocus)) restoreFocus.focus();
  };
  const onDown = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) close();
  };
  const move = (delta: number) => {
    if (buttons.length === 0) return;
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = i === -1 ? (delta > 0 ? 0 : buttons.length - 1) : (i + delta + buttons.length) % buttons.length;
    buttons[next].focus();
  };
  const onKey = (e: KeyboardEvent) => {
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        close();
        break;
      case "ArrowDown":
        e.preventDefault();
        e.stopPropagation();
        move(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        e.stopPropagation();
        move(-1);
        break;
      case "Home":
        e.preventDefault();
        e.stopPropagation();
        buttons[0]?.focus();
        break;
      case "End":
        e.preventDefault();
        e.stopPropagation();
        buttons[buttons.length - 1]?.focus();
        break;
      case "Tab":
        // Keep focus inside the menu; Esc is the way out.
        e.preventDefault();
        e.stopPropagation();
        move(e.shiftKey ? -1 : 1);
        break;
      default:
        break;
    }
  };

  current = close;
  document.body.appendChild(menu);
  const r = menu.getBoundingClientRect();
  const left = Math.min(x, window.innerWidth - r.width - 4);
  const top = Math.min(y, window.innerHeight - r.height - 4);
  menu.style.left = `${Math.max(4, left)}px`;
  menu.style.top = `${Math.max(4, top)}px`;
  document.addEventListener("mousedown", onDown, true);
  document.addEventListener("keydown", onKey, true);
  window.addEventListener("blur", close);
  buttons[0]?.focus();
}
