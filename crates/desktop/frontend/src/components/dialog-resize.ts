/** Corner drag-grip that makes a dialog user-resizable, with the chosen
 *  size persisted per dialog id (`dialogSizes` settings key) so every
 *  dialog reopens at the size you gave it. Double-click on the grip
 *  resets to the stylesheet's budget.
 *
 *  Attach AFTER the dialog is in the DOM — the height clamp measures the
 *  dialog's top offset (dialogs are top-anchored by the backdrop's
 *  padding), so an unmounted dialog would clamp against the wrong origin.
 */
import { settingsStore } from "../settings";

export const DIALOG_MIN_W = 360;
export const DIALOG_MIN_H = 240;
/** Margin kept below the dialog so the resized box never touches the
 *  viewport edge. */
export const DIALOG_BOTTOM_MARGIN = 10;

export type DialogSize = { w: number; h: number };
type StoredSizes = Record<string, DialogSize>;

const KEY = "dialogSizes";

/** `w`/`h` clamped to the dialog's floor and the available viewport space
 *  (`maxW`/`maxH` — the caller derives them from the viewport and the
 *  dialog's top offset). Pure — covered by dialog-resize.test.ts. */
export function clampDialogSize(
  w: number,
  h: number,
  maxW: number,
  maxH: number,
): DialogSize {
  return {
    w: Math.round(Math.min(Math.max(w, DIALOG_MIN_W), Math.max(maxW, DIALOG_MIN_W))),
    h: Math.round(Math.min(Math.max(h, DIALOG_MIN_H), Math.max(maxH, DIALOG_MIN_H))),
  };
}

/** The stored size for `id`, or null when none was saved or the record is
 *  malformed (settings are user-editable state — never trust the shape). */
export function storedDialogSize(sizes: unknown, id: string): DialogSize | null {
  if (typeof sizes !== "object" || sizes === null) return null;
  const entry = (sizes as Record<string, unknown>)[id];
  if (typeof entry !== "object" || entry === null) return null;
  const { w, h } = entry as Record<string, unknown>;
  if (typeof w !== "number" || typeof h !== "number" || !isFinite(w) || !isFinite(h)) {
    return null;
  }
  return { w, h };
}

/** Make `dialog` resizable from a bottom-right grip and remember the size
 *  under `id`. The dialog keeps its stylesheet budget until the user first
 *  drags; from then on the stored size wins on every open (clamped to the
 *  current viewport). */
export function attachDialogResize(dialog: HTMLElement, id: string) {
  const apply = (w: number, h: number) => {
    const top = dialog.getBoundingClientRect().top;
    const size = clampDialogSize(
      w,
      h,
      Math.round(window.innerWidth * 0.95),
      window.innerHeight - top - DIALOG_BOTTOM_MARGIN,
    );
    dialog.style.width = `${size.w}px`;
    dialog.style.height = `${size.h}px`;
    // The stylesheet's max-* would cap the user's choice; our clamp above
    // already bounds it to the viewport.
    dialog.style.maxWidth = "none";
    dialog.style.maxHeight = "none";
  };

  const stored = storedDialogSize(settingsStore.get<StoredSizes>(KEY), id);
  if (stored) apply(stored.w, stored.h);

  const grip = document.createElement("div");
  grip.className = "dialog-resize-grip";
  grip.title = "Drag to resize · double-click to reset";
  dialog.appendChild(grip);

  let raf = 0;
  grip.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    e.stopPropagation();
    grip.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startY = e.clientY;
    const rect = dialog.getBoundingClientRect();
    let pending: DialogSize | null = null;

    const onMove = (ev: PointerEvent) => {
      pending = {
        w: rect.width + (ev.clientX - startX),
        h: rect.height + (ev.clientY - startY),
      };
      // One resize per frame — the drag fires far faster than paint.
      if (!raf) {
        raf = requestAnimationFrame(() => {
          raf = 0;
          if (pending) apply(pending.w, pending.h);
        });
      }
    };
    const onUp = () => {
      grip.removeEventListener("pointermove", onMove);
      grip.removeEventListener("pointerup", onUp);
      if (raf) {
        cancelAnimationFrame(raf);
        raf = 0;
      }
      if (pending) apply(pending.w, pending.h);
      const end = dialog.getBoundingClientRect();
      const sizes: StoredSizes = { ...(settingsStore.get<StoredSizes>(KEY) ?? {}) };
      sizes[id] = { w: Math.round(end.width), h: Math.round(end.height) };
      settingsStore.patch(KEY, sizes);
    };
    grip.addEventListener("pointermove", onMove);
    grip.addEventListener("pointerup", onUp);
  });

  grip.addEventListener("dblclick", () => {
    dialog.style.width = "";
    dialog.style.height = "";
    dialog.style.maxWidth = "";
    dialog.style.maxHeight = "";
    const sizes: StoredSizes = { ...(settingsStore.get<StoredSizes>(KEY) ?? {}) };
    delete sizes[id];
    settingsStore.patch(KEY, Object.keys(sizes).length > 0 ? sizes : undefined);
  });
}
