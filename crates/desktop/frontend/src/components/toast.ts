import * as ipc from "../ipc";
import type { ToastEvent } from "../types";

const TOAST_DURATION = 4000;

export function initToasts() {
  ipc.onToast((event: ToastEvent) => {
    toast(event.message, event.level);
  });
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
  container.appendChild(el);

  setTimeout(() => {
    el.style.opacity = "0";
    el.style.transform = "translateY(8px)";
    el.style.transition = "all 0.2s ease-out";
    setTimeout(() => el.remove(), 200);
  }, TOAST_DURATION);
}
