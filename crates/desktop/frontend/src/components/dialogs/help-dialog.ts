const SHORTCUTS: { category: string; items: [string, string][] }[] = [
  {
    category: "General",
    items: [
      ["Ctrl+P", "Command Palette"],
      ["Ctrl+N", "New Workspace"],
      ["Ctrl+Space", "Workspace Switcher"],
      ["Alt+D", "Dashboard"],
      ["Ctrl+Tab", "Next Tab"],
      ["Ctrl+Shift+Tab", "Previous Tab"],
      ["?", "Keyboard Shortcuts"],
      ["Esc", "Close Dialog / Overlay"],
    ],
  },
  {
    category: "Git",
    items: [
      ["Ctrl+F", "Find File"],
      ["Alt+L", "Git Log"],
      ["Ctrl+M", "Merge / Rebase"],
      ["Ctrl+Shift+S", "Git Stash"],
      ["Ctrl+Shift+F", "Search in Project"],
      ["Ctrl+Shift+B", "Search in Terminal"],
      ["Ctrl+Z", "Undo Stage / Unstage"],
    ],
  },
  {
    category: "Review & Agents",
    items: [
      ["Ctrl+Shift+R", "Code Review (PR)"],
      ["Ctrl+Shift+A", "Manage Agents"],
      ["Ctrl+Shift+D", "Dispatch Agent"],
      ["Alt+K", "Kanban Board"],
      ["Alt+T", "Theme Settings"],
      ["Alt+Shift+L", "Application Logs"],
    ],
  },
  {
    category: "Terminal",
    items: [
      ["Ctrl+Shift+C", "Copy Selection"],
      ["Ctrl+Shift+V", "Paste from Clipboard"],
      ["Select text", "Auto-copy to Clipboard"],
    ],
  },
];

export function showHelpDialog() {
  document.querySelector(".dialog-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";

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

  for (const group of SHORTCUTS) {
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
