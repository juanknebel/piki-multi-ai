// Sections derive from the shortcut registry (`shortcuts.ts`) — the single
// source for every key. A rebind in Settings shows up here automatically;
// never hand-maintain a key list in this file.
import { helpSections } from "../../shortcuts";

export function showHelpDialog() {
  document.querySelector(".help-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop help-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "480px";
  dialog.style.maxHeight = "80vh";

  let html = `
    <div class="dialog-header">
      <span class="dialog-title">Keyboard Shortcuts</span>
      <button class="dialog-close">×</button>
    </div>
    <div class="dialog-body" style="overflow-y:auto">
  `;

  for (const group of helpSections()) {
    html += `<div class="shortcut-group">
      <div class="shortcut-group-title">${group.category}</div>`;
    for (const [key, desc] of group.items) {
      html += `
        <div class="shortcut-row">
          <span class="shortcut-row-label">${desc}</span>
          <kbd class="shortcut-row-key">${key}</kbd>
        </div>`;
    }
    html += `</div>`;
  }

  html += `</div>
    <div class="dialog-footer">
      <button class="dialog-btn dialog-btn-secondary" id="help-close">Close</button>
    </div>`;

  dialog.innerHTML = html;
  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();
  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  dialog.querySelector("#help-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}
