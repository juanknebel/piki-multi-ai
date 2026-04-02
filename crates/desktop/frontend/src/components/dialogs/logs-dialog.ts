import * as ipc from "../../ipc";
import { toast } from "../toast";

const LEVEL_FILTERS = [
  { label: "All", value: 0 },
  { label: "Error", value: 1 },
  { label: "Warn", value: 2 },
  { label: "Info", value: 3 },
  { label: "Debug", value: 4 },
  { label: "Trace", value: 5 },
];

const LEVEL_COLORS: Record<string, string> = {
  ERROR: "var(--git-deleted)",
  WARN: "var(--accent-warm)",
  INFO: "var(--git-added)",
  DEBUG: "var(--accent-primary)",
  TRACE: "var(--text-muted)",
};

export async function showLogsDialog() {
  document.querySelector(".logs-dialog-backdrop")?.remove();

  let currentFilter = 0;

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop logs-dialog-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "800px";
  dialog.style.maxHeight = "85vh";
  dialog.style.width = "90vw";

  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title">Application Logs</span>
      <span style="display:flex;gap:6px;align-items:center">
        <select class="theme-preset-select" id="log-level-filter" style="width:auto;min-width:80px">
          ${LEVEL_FILTERS.map((f) => `<option value="${f.value}"${f.value === 0 ? " selected" : ""}>${f.label}</option>`).join("")}
        </select>
        <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="log-refresh">Refresh</button>
        <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="log-clear">Clear</button>
        <button class="dialog-close">×</button>
      </span>
    </div>
    <div id="log-entries" style="flex:1;overflow-y:auto;padding:0;font-size:11px;line-height:1.6;max-height:70vh"></div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const entriesContainer = dialog.querySelector<HTMLElement>("#log-entries")!;
  const filterSelect = dialog.querySelector<HTMLSelectElement>("#log-level-filter")!;

  async function loadLogs() {
    try {
      const entries = await ipc.getLogs(currentFilter);
      entriesContainer.innerHTML = "";

      if (entries.length === 0) {
        entriesContainer.innerHTML = '<div class="empty-message">No log entries</div>';
        return;
      }

      for (const entry of entries) {
        const row = document.createElement("div");
        row.style.cssText = `padding:2px 14px;display:flex;gap:8px;border-bottom:1px solid var(--border-subtle);`;

        const color = LEVEL_COLORS[entry.level] || "var(--text-primary)";
        const levelPad = entry.level.padEnd(5);

        row.innerHTML = `
          <span style="color:var(--text-muted);flex-shrink:0">${esc(entry.timestamp)}</span>
          <span style="color:${color};font-weight:600;flex-shrink:0;width:42px">${esc(levelPad)}</span>
          <span style="color:var(--text-secondary);flex-shrink:0;max-width:180px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(entry.target)}</span>
          <span style="color:var(--text-primary);flex:1;white-space:pre-wrap;word-break:break-all">${esc(entry.message)}</span>
        `;
        entriesContainer.appendChild(row);
      }

      // Auto-scroll to bottom
      entriesContainer.scrollTop = entriesContainer.scrollHeight;
    } catch (err) {
      entriesContainer.innerHTML = `<div class="empty-message">Failed to load logs: ${err}</div>`;
    }
  }

  filterSelect.addEventListener("change", () => {
    currentFilter = parseInt(filterSelect.value, 10);
    loadLogs();
  });

  dialog.querySelector("#log-refresh")!.addEventListener("click", () => loadLogs());

  dialog.querySelector("#log-clear")!.addEventListener("click", async () => {
    try {
      await ipc.clearLogs();
      loadLogs();
      toast("Logs cleared", "info");
    } catch (err) {
      toast(`Clear failed: ${err}`, "error");
    }
  });

  const close = () => backdrop.remove();
  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();

  await loadLogs();
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
