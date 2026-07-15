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

  // Family = workspaces sharing `source_repo` (mirrors the sidebar's
  // worktree-family grouping). Families with more than one loaded member get
  // a section header showing the repo folder name; everything else is
  // ungrouped and sorts first, same as before.
  type WsItem = {
    idx: number;
    name: string;
    sourceRepo: string;
    branch: string;
    order: number;
  };
  const allItems: WsItem[] = appState.workspaces.map((ws, i) => ({
    idx: i,
    name: ws.info.name,
    sourceRepo: ws.info.source_repo,
    branch: ws.info.branch,
    order: ws.info.order,
  }));

  function folderName(sourceRepo: string): string {
    return sourceRepo.replace(/\/+$/, "").split("/").pop() || sourceRepo;
  }

  let filtered = allItems;
  let renderedItems: WsItem[] = [];

  function groupAndSort(items: WsItem[]): { group: string; items: WsItem[] }[] {
    const bySourceRepo = new Map<string, WsItem[]>();
    for (const item of items) {
      if (!bySourceRepo.has(item.sourceRepo)) bySourceRepo.set(item.sourceRepo, []);
      bySourceRepo.get(item.sourceRepo)!.push(item);
    }
    // Family label is "" (ungrouped) when it's the only loaded workspace for
    // that source_repo; otherwise the repo folder name becomes the header.
    const groups = new Map<string, WsItem[]>();
    for (const [sourceRepo, members] of bySourceRepo) {
      const label = members.length > 1 ? folderName(sourceRepo) : "";
      if (!groups.has(label)) groups.set(label, []);
      groups.get(label)!.push(...members);
    }
    return [...groups.entries()]
      .sort(([a], [b]) => {
        if (a === "" && b !== "") return -1;
        if (a !== "" && b === "") return 1;
        return a.localeCompare(b);
      })
      .map(([group, items]) => ({
        group,
        items: items.sort((a, b) => a.order - b.order),
      }));
  }

  function renderResults() {
    results.innerHTML = "";
    renderedItems = [];
    const grouped = groupAndSort(filtered);
    let flatIdx = 0;

    for (const section of grouped) {
      if (section.group) {
        const header = document.createElement("div");
        header.className = "palette-group-header";
        header.textContent = section.group;
        results.appendChild(header);
      }

      for (const item of section.items) {
        renderedItems.push(item);
        const idx = flatIdx++;
        const el = document.createElement("div");
        el.className = `palette-item${idx === selectedIdx ? " selected" : ""}`;
        const isCurrent = item.idx === appState.activeWorkspace;

        el.innerHTML = `
          <span class="palette-category">${item.idx < 9 ? item.idx + 1 : ""}</span>
          <span class="palette-label">
            ${isCurrent ? "● " : ""}${highlightMatch(item.name, input.value)}
          </span>
          <span class="palette-key">⎇ ${escapeHtml(item.branch)}</span>
        `;

        el.addEventListener("click", () => switchTo(item.idx));
        el.addEventListener("mouseenter", () => {
          if (selectedIdx === idx) return;
          selectedIdx = idx;
          updateSelection();
        });
        results.appendChild(el);
      }
    }

    if (filtered.length === 0) {
      results.innerHTML = '<div class="palette-empty">No matching workspaces</div>';
    }
  }

  function updateSelection() {
    results.querySelectorAll<HTMLElement>(".palette-item").forEach((el, i) => {
      el.classList.toggle("selected", i === selectedIdx);
    });
  }

  function filter() {
    const q = input.value.toLowerCase();
    if (!q) {
      filtered = allItems;
    } else {
      filtered = allItems.filter(
        (item) =>
          item.name.toLowerCase().includes(q) ||
          folderName(item.sourceRepo).toLowerCase().includes(q) ||
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
      selectedIdx = Math.min(selectedIdx + 1, renderedItems.length - 1);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (renderedItems[selectedIdx]) switchTo(renderedItems[selectedIdx].idx);
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
