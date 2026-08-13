import * as ipc from "../ipc";
import type { ToastEvent } from "../types";

const TOAST_DURATION = 4000;
/** Errors carry the only record of a failure — give them time to be read. */
const ERROR_TOAST_DURATION = 8000;
/** Cap the stack; a burst drops the oldest instead of flooding the screen. */
const MAX_TOASTS = 5;

export function initToasts() {
  ipc.onToast((event: ToastEvent) => {
    toast(event.message, event.level);
  });
  const container = document.getElementById("toast-container");
  if (container) {
    container.setAttribute("role", "status");
    container.setAttribute("aria-live", "polite");
  }
}

/** Surface a failed user action: error toast for the user, console.error
 *  (with the raw error object) for diagnostics. `context` is the user-facing
 *  description, e.g. "Commit failed". */
export function reportError(context: string, err: unknown) {
  console.error(context, err);
  toast(`${context}: ${err}`, "error");
}

export function toast(
  message: string,
  level: "info" | "success" | "error" = "info",
) {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const el = document.createElement("div");
  el.className = `toast ${level}`;
  el.textContent = message;
  el.title = "Click to dismiss";

  let dismissed = false;
  const dismiss = () => {
    if (dismissed) return;
    dismissed = true;
    el.style.opacity = "0";
    el.style.transform = "translateY(8px)";
    el.style.transition = "all 0.2s ease-out";
    setTimeout(() => el.remove(), 200);
  };
  el.addEventListener("click", dismiss);

  container.appendChild(el);
  while (container.children.length > MAX_TOASTS) {
    container.firstElementChild?.remove();
  }

  setTimeout(dismiss, level === "error" ? ERROR_TOAST_DURATION : TOAST_DURATION);
}
