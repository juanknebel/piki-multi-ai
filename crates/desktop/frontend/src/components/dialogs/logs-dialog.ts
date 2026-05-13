import * as ipc from "../../ipc";
import { toast } from "../toast";
import { createDropdown } from "../dropdown";
import { modCtrl } from "../../shortcuts";
import type { LogEntry } from "../../ipc";

const LEVEL_FILTERS = [
  { label: "All", value: "0" },
  { label: "Error", value: "1" },
  { label: "Warn", value: "2" },
  { label: "Info", value: "3" },
  { label: "Debug", value: "4" },
  { label: "Trace", value: "5" },
];

const LEVEL_COLORS: Record<string, string> = {
  ERROR: "var(--git-deleted)",
  WARN: "var(--accent-warm)",
  INFO: "var(--git-added)",
  DEBUG: "var(--accent-primary)",
  TRACE: "var(--text-muted)",
};

const REFRESH_INTERVAL_MS = 1500;

export async function showLogsDialog() {
  document.querySelector(".logs-dialog-backdrop")?.remove();

  let currentFilter = 0;
  let searchText = "";
  let lastEntries: LogEntry[] = [];
  let selectedIdx: number | null = null;
  let refreshTimer: number | null = null;

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
        <input type="text" class="dialog-input log-search" placeholder="Filter target or message..." />
        <span id="log-level-slot"></span>
        <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="log-refresh" title="Refresh">Refresh</button>
        <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="log-clear" title="Clear">Clear</button>
        <button class="dialog-close">×</button>
      </span>
    </div>
    <div id="log-entries" style="flex:1;overflow-y:auto;padding:0;font-size:11px;line-height:1.6;max-height:70vh"></div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const levelDropdown = createDropdown(LEVEL_FILTERS, "0", "width:auto;min-width:80px");
  dialog.querySelector("#log-level-slot")!.replaceWith(levelDropdown.container);

  const entriesContainer = dialog.querySelector<HTMLElement>("#log-entries")!;
  const searchInput = dialog.querySelector<HTMLInputElement>(".log-search")!;

  function visibleEntries(): LogEntry[] {
    const q = searchText.trim().toLowerCase();
    if (!q) return lastEntries;
    return lastEntries.filter((e) =>
      e.target.toLowerCase().includes(q) || e.message.toLowerCase().includes(q),
    );
  }

  function renderEntries() {
    const visible = visibleEntries();

    const wasAtBottom =
      entriesContainer.scrollTop + entriesContainer.clientHeight >=
      entriesContainer.scrollHeight - 4;

    entriesContainer.innerHTML = "";

    if (visible.length === 0) {
      entriesContainer.innerHTML =
        lastEntries.length === 0
          ? '<div class="empty-message">No log entries</div>'
          : '<div class="empty-message">No logs match the filter</div>';
      return;
    }

    visible.forEach((entry, idx) => {
      const row = document.createElement("div");
      row.className = "log-row";
      row.dataset.idx = String(idx);
      if (idx === selectedIdx) row.classList.add("log-row-selected");

      const color = LEVEL_COLORS[entry.level] || "var(--text-primary)";
      const levelPad = entry.level.padEnd(5);

      row.innerHTML = `
        <span style="color:var(--text-muted);flex-shrink:0">${esc(entry.timestamp)}</span>
        <span style="color:${color};font-weight:600;flex-shrink:0;width:42px">${esc(levelPad)}</span>
        <span style="color:var(--text-secondary);flex-shrink:0;max-width:180px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(entry.target)}</span>
        <span style="color:var(--text-primary);flex:1;white-space:pre-wrap;word-break:break-all">${esc(entry.message)}</span>
      `;
      row.addEventListener("click", () => {
        selectRow(idx);
      });
      entriesContainer.appendChild(row);
    });

    if (wasAtBottom) {
      entriesContainer.scrollTop = entriesContainer.scrollHeight;
    } else if (selectedIdx !== null) {
      const sel = entriesContainer.querySelector<HTMLElement>(
        `.log-row[data-idx="${selectedIdx}"]`,
      );
      sel?.scrollIntoView({ block: "nearest" });
    }
  }

  function selectRow(idx: number) {
    const visible = visibleEntries();
    if (idx < 0 || idx >= visible.length) return;
    entriesContainer
      .querySelectorAll<HTMLElement>(".log-row")
      .forEach((r) => r.classList.remove("log-row-selected"));
    const row = entriesContainer.querySelector<HTMLElement>(
      `.log-row[data-idx="${idx}"]`,
    );
    if (row) {
      row.classList.add("log-row-selected");
      row.scrollIntoView({ block: "nearest" });
    }
    selectedIdx = idx;
  }

  async function fetchLogs() {
    try {
      const next = await ipc.getLogs(currentFilter);
      // Skip re-render if nothing changed (avoid flicker + keep selection stable)
      if (
        next.length === lastEntries.length &&
        next.length > 0 &&
        next[next.length - 1].timestamp ===
          lastEntries[lastEntries.length - 1].timestamp
      ) {
        return;
      }
      lastEntries = next;
      // Clear selection if it points past the end after a refresh
      if (selectedIdx !== null && selectedIdx >= visibleEntries().length) {
        selectedIdx = null;
      }
      renderEntries();
    } catch (err) {
      entriesContainer.innerHTML = `<div class="empty-message">Failed to load logs: ${err}</div>`;
    }
  }

  // ── Wire UI ──────────────────────────────────────

  searchInput.addEventListener("input", () => {
    searchText = searchInput.value;
    selectedIdx = null;
    renderEntries();
  });

  levelDropdown.container.addEventListener("change", () => {
    currentFilter = parseInt(levelDropdown.value, 10);
    selectedIdx = null;
    void fetchLogs();
  });

  dialog.querySelector("#log-refresh")!.addEventListener("click", () => {
    void fetchLogs();
  });

  dialog.querySelector("#log-clear")!.addEventListener("click", async () => {
    try {
      await ipc.clearLogs();
      lastEntries = [];
      selectedIdx = null;
      renderEntries();
      toast("Logs cleared", "info");
    } catch (err) {
      toast(`Clear failed: ${err}`, "error");
    }
  });

  const close = () => {
    if (refreshTimer !== null) {
      window.clearInterval(refreshTimer);
      refreshTimer = null;
    }
    backdrop.remove();
  };

  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });

  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      close();
      return;
    }

    const tag = (e.target as HTMLElement).tagName;
    const inInput = tag === "INPUT" || tag === "TEXTAREA";

    // Level shortcut 0-5 — only when not typing in the search field
    if (!inInput && e.key >= "0" && e.key <= "5") {
      const lvl = parseInt(e.key, 10);
      currentFilter = lvl;
      levelDropdown.value = String(lvl);
      selectedIdx = null;
      void fetchLogs();
      e.preventDefault();
      return;
    }

    // Copy selected row — works even from inside the search input
    if (e.key === "c" && modCtrl(e) && selectedIdx !== null) {
      const visible = visibleEntries();
      const sel = visible[selectedIdx];
      if (sel) {
        const line = `${sel.timestamp}  ${sel.level.padEnd(5)}  ${sel.target}  ${sel.message}`;
        ipc.clipboardCopy(line).then(() => {
          toast("Copied to clipboard", "info");
        }).catch((err) => toast(`Copy failed: ${err}`, "error"));
        e.preventDefault();
      }
      return;
    }

    // Arrow navigation — only when focus is not in the search input
    if (!inInput && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      const visible = visibleEntries();
      if (visible.length === 0) return;
      const next = selectedIdx === null
        ? (e.key === "ArrowDown" ? 0 : visible.length - 1)
        : selectedIdx + (e.key === "ArrowDown" ? 1 : -1);
      const clamped = Math.max(0, Math.min(visible.length - 1, next));
      selectRow(clamped);
      e.preventDefault();
    }
  });

  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();

  await fetchLogs();

  // Auto-refresh streaming
  refreshTimer = window.setInterval(() => {
    void fetchLogs();
  }, REFRESH_INTERVAL_MS);
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
