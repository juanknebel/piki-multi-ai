import { appState } from "../state";
import * as ipc from "../ipc";

let switcherEl: HTMLElement | null = null;

export function openWorkspaceSwitcher() {
  if (switcherEl) {
    closeWorkspaceSwitcher();
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";

  const palette = document.createElement("div");
  palette.className = "palette";

  palette.innerHTML = `
    <input class="palette-input" type="text" placeholder="Switch workspace..." autofocus />
    <div class="palette-results"></div>
  `;

  backdrop.appendChild(palette);
  document.body.appendChild(backdrop);
  switcherEl = backdrop;

  const input = palette.querySelector<HTMLInputElement>(".palette-input")!;
  const results = palette.querySelector<HTMLElement>(".palette-results")!;
  let selectedIdx = 0;

  type WsItem = { idx: number; name: string; group: string; branch: string };
  const allItems: WsItem[] = appState.workspaces.map((ws, i) => ({
    idx: i,
    name: ws.info.name,
    group: ws.info.group || "",
    branch: ws.info.branch,
  }));

  let filtered = allItems;

  function renderResults() {
    results.innerHTML = "";
    filtered.forEach((item, i) => {
      const el = document.createElement("div");
      el.className = `palette-item${i === selectedIdx ? " selected" : ""}`;
      const isCurrent = item.idx === appState.activeWorkspace;

      el.innerHTML = `
        <span class="palette-category">${item.idx < 9 ? item.idx + 1 : ""}</span>
        <span class="palette-label">
          ${isCurrent ? "● " : ""}${highlightMatch(item.name, input.value)}
          ${item.group ? ` <span style="color:var(--text-muted)">(${escapeHtml(item.group)})</span>` : ""}
        </span>
        <span class="palette-key">⎇ ${escapeHtml(item.branch)}</span>
      `;

      el.addEventListener("click", () => switchTo(item.idx));
      el.addEventListener("mouseenter", () => {
        selectedIdx = i;
        renderResults();
      });
      results.appendChild(el);
    });

    if (filtered.length === 0) {
      results.innerHTML = '<div class="palette-empty">No matching workspaces</div>';
    }
  }

  function filter() {
    const q = input.value.toLowerCase();
    if (!q) {
      filtered = allItems;
    } else {
      filtered = allItems.filter(
        (item) =>
          item.name.toLowerCase().includes(q) ||
          item.group.toLowerCase().includes(q) ||
          item.branch.toLowerCase().includes(q),
      );
    }
    selectedIdx = 0;
    renderResults();
  }

  async function switchTo(idx: number) {
    closeWorkspaceSwitcher();
    try {
      const detail = await ipc.switchWorkspace(idx);
      appState.setActiveWorkspace(idx, detail);
    } catch (err) {
      console.error("Switch failed:", err);
    }
  }

  input.addEventListener("input", filter);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, filtered.length - 1);
      renderResults();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      renderResults();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filtered[selectedIdx]) switchTo(filtered[selectedIdx].idx);
    } else if (e.key === "Escape") {
      closeWorkspaceSwitcher();
    }
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeWorkspaceSwitcher();
  });

  renderResults();
  input.focus();
}

export function closeWorkspaceSwitcher() {
  switcherEl?.remove();
  switcherEl = null;
}

function highlightMatch(text: string, query: string): string {
  if (!query) return escapeHtml(text);
  const lower = text.toLowerCase();
  const idx = lower.indexOf(query.toLowerCase());
  if (idx === -1) return escapeHtml(text);
  const before = text.slice(0, idx);
  const match = text.slice(idx, idx + query.length);
  const after = text.slice(idx + query.length);
  return `${escapeHtml(before)}<strong>${escapeHtml(match)}</strong>${escapeHtml(after)}`;
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
