import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "./toast";

export interface PathPickerOptions {
  /** Pick a directory (default) or a single file */
  directory?: boolean;
  /** Dialog title shown by the OS picker */
  title?: string;
}

const FOLDER_ICON = `
<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d="M1.5 4.5h4l1.5 1.5h7.5v7a1 1 0 0 1-1 1h-12a1 1 0 0 1-1-1v-8.5z"/>
</svg>`.trim();

/**
 * Wrap an existing input element with a folder-icon button that opens
 * the native file/directory picker and writes the selection back into
 * the input. The input keeps its id/classes; only its parent layout
 * changes (we insert a flex row wrapper).
 */
export function attachPathPicker(
  input: HTMLInputElement,
  opts: PathPickerOptions = {},
): void {
  const parent = input.parentElement;
  if (!parent) return;

  const row = document.createElement("div");
  row.className = "path-picker-row";

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "path-picker-btn";
  btn.title = opts.directory === false ? "Choose file" : "Choose folder";
  btn.setAttribute("aria-label", btn.title);
  btn.innerHTML = FOLDER_ICON;

  parent.insertBefore(row, input);
  row.appendChild(input);
  row.appendChild(btn);

  btn.addEventListener("click", async () => {
    try {
      const defaultPath = input.value.trim() || undefined;
      const picked = await open({
        directory: opts.directory !== false,
        multiple: false,
        title: opts.title,
        defaultPath,
      });
      if (typeof picked === "string" && picked.length > 0) {
        input.value = picked;
        // Notify any listeners (e.g. debounced setters) that the value changed.
        input.dispatchEvent(new Event("input", { bubbles: true }));
        input.dispatchEvent(new Event("change", { bubbles: true }));
      }
    } catch (err) {
      toast(`Picker failed: ${err}`, "error");
    }
  });
}
