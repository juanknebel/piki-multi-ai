// Shared modal confirm overlay. One implementation for every destructive
// confirmation so Escape-to-cancel, Enter-to-confirm, focus trapping and
// focus restore behave the same everywhere. Renders the established
// `.ws-delete-confirm` / `.ws-delete-dialog ui-surface` markup
// (styles/dialog-core.css) with `.ui-btn` actions.

export interface ConfirmAction {
  label: string;
  kind: "primary" | "danger" | "secondary";
  /** Runs when the action is chosen. Receives `close` when `keepOpen`. */
  onSelect?: (ctx: { close: () => void }) => void;
  /** Enter activates this action while focus isn't on another button. */
  isDefault?: boolean;
  /** Initial focus lands here (defaults to the last secondary action). */
  autofocus?: boolean;
  /** Don't auto-close before running `onSelect` (e.g. async save that only
   *  closes on success). */
  keepOpen?: boolean;
}

export interface ConfirmOptions {
  /** Pre-escaped HTML for the dialog body (message + hint paragraphs). */
  bodyHtml: string;
  actions: ConfirmAction[];
  /** Called on Escape or backdrop click. */
  onDismiss?: () => void;
  /** Extra class(es) for the overlay element. */
  className?: string;
}

export function showConfirm(opts: ConfirmOptions): {
  overlay: HTMLDivElement;
  close: () => void;
} {
  document.querySelector(".ws-delete-confirm")?.remove();

  const prevFocus =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;

  const overlay = document.createElement("div");
  overlay.className = `ws-delete-confirm${opts.className ? ` ${opts.className}` : ""}`;
  overlay.tabIndex = -1;

  const dialog = document.createElement("div");
  dialog.className = "ws-delete-dialog ui-surface";
  dialog.innerHTML = opts.bodyHtml;

  const buttons = document.createElement("div");
  buttons.className = "ws-delete-buttons";
  dialog.appendChild(buttons);
  overlay.appendChild(dialog);

  const close = () => {
    overlay.remove();
    prevFocus?.focus();
  };

  const run = (action: ConfirmAction) => {
    if (action.keepOpen) {
      action.onSelect?.({ close });
    } else {
      close();
      action.onSelect?.({ close });
    }
  };

  const btnEls = opts.actions.map((action) => {
    const btn = document.createElement("button");
    btn.className = "ui-btn";
    btn.dataset.variant = action.kind;
    btn.textContent = action.label;
    btn.addEventListener("click", () => run(action));
    buttons.appendChild(btn);
    return btn;
  });

  const dismiss = () => {
    close();
    opts.onDismiss?.();
  };

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) dismiss();
  });

  overlay.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      dismiss();
      return;
    }
    if (e.key === "Enter") {
      // Buttons handle their own Enter; only fill in when focus is elsewhere.
      if (!(e.target instanceof HTMLButtonElement)) {
        const def = opts.actions.findIndex((a) => a.isDefault);
        if (def >= 0) {
          e.preventDefault();
          e.stopPropagation();
          run(opts.actions[def]);
        }
      }
      return;
    }
    if (e.key === "Tab") {
      // Trap focus within the overlay.
      const focusables = Array.from(
        overlay.querySelectorAll<HTMLElement>(
          "button, input, select, textarea, [tabindex]:not([tabindex='-1'])",
        ),
      ).filter((el) => !el.hasAttribute("disabled"));
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && (active === first || active === overlay)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    }
  });

  document.body.appendChild(overlay);

  const focusIdx = opts.actions.findIndex((a) => a.autofocus);
  const safeIdx =
    focusIdx >= 0
      ? focusIdx
      : opts.actions.map((a) => a.kind).lastIndexOf("secondary");
  (btnEls[safeIdx] ?? btnEls[btnEls.length - 1] ?? overlay).focus();

  return { overlay, close };
}

export function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
