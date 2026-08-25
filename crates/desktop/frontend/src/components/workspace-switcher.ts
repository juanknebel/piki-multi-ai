// Ctrl+Space workspace switcher. Ranking is the pure `rankItems` (mru.ts):
// empty query → most recently used first (the list `appState` keeps under
// the `workspaceMru` settings key on every switch), then sidebar order;
// otherwise fuzzy across name / repo folder / branch ("wsauth" finds
// "ws-auth"), recency as the tie-break. Each row carries a status glyph:
// the worst agent state of that workspace (from `appState.agentRows`) or a
// dirty-git marker.

import { appState, WORKSPACE_MRU_KEY } from "../state";
import { reportError } from "./toast";
import * as ipc from "../ipc";
import { settingsStore } from "../settings";
import { formatShortcut } from "../shortcuts";
import { rankItems } from "../mru";
import { fuzzyScore } from "./fuzzy";
import { branchLabel } from "../labels";
import { agentStatusSeverity, cliAgentStatusView, type CliAgentStatus } from "../types";

let switcherEl: HTMLElement | null = null;

type WsItem = {
  key: string;
  idx: number;
  name: string;
  folder: string;
  branch: string | null;
  texts: string[];
  order: number;
};

/** Worst (status, attention) among the live agents of workspace `idx`. */
function agentRollup(idx: number): { status: CliAgentStatus; attention: boolean } | null {
  let best: { status: CliAgentStatus; attention: boolean } | null = null;
  let bestSev = -1;
  for (const row of appState.agentRows) {
    if (row.workspace_idx !== idx || !row.status) continue;
    const sev = agentStatusSeverity(row.status, row.attention);
    if (sev > bestSev) {
      best = { status: row.status, attention: row.attention };
      bestSev = sev;
    }
  }
  return best;
}

/** Status column for a row: agent state first (it is what needs a human),
 *  else uncommitted changes. */
function statusGlyph(idx: number): string {
  const agent = agentRollup(idx);
  if (agent) {
    const v = cliAgentStatusView(agent.status, agent.attention);
    return `<span class="palette-status" style="color:${v.color}" title="Agent ${escapeHtml(v.label)}">${v.glyph}</span>`;
  }
  const changes = appState.workspaces[idx]?.changedFiles.length ?? 0;
  if (changes > 0) {
    return `<span class="palette-status dirty" title="${changes} uncommitted change${changes === 1 ? "" : "s"}">●</span>`;
  }
  return `<span class="palette-status"></span>`;
}

function folderName(sourceRepo: string): string {
  return sourceRepo.replace(/\/+$/, "").split("/").pop() || sourceRepo;
}

export function openWorkspaceSwitcher() {
  if (switcherEl) {
    closeWorkspaceSwitcher();
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";

  const palette = document.createElement("div");
  palette.className = "palette ui-surface";

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

  const allItems: WsItem[] = appState.workspaces.map((ws, i) => {
    const folder = folderName(ws.info.source_repo);
    return {
      key: String(ws.info.path),
      idx: i,
      name: ws.info.name,
      folder,
      branch: ws.branch,
      texts: [ws.info.name, folder, ws.branch ?? ""].filter((t) => t.length > 0),
      order: ws.info.order,
    };
  });
  const mru = settingsStore.get<string[]>(WORKSPACE_MRU_KEY) ?? [];

  let ranked: WsItem[] = rankItems(allItems, "", mru);

  function renderResults() {
    results.innerHTML = "";
    const q = input.value.trim();

    ranked.forEach((item, idx) => {
      const el = document.createElement("div");
      el.className = `palette-item${idx === selectedIdx ? " selected" : ""}`;
      const isCurrent = item.idx === appState.activeWorkspace;
      // Secondary text: the repo folder when the query hit it (or the name
      // doesn't already say it), always the branch.
      const showFolder = item.folder !== item.name && (!q || fuzzyScore(q, item.folder) !== null);
      const sub = [showFolder ? item.folder : null, item.branch ? `⎇ ${branchLabel(item.branch)}` : null]
        .filter((s): s is string => !!s)
        .join(" · ");

      el.innerHTML = `
        <span class="palette-category">${item.idx < 9 ? formatShortcut(`Alt+${item.idx + 1}`) : ""}</span>
        ${statusGlyph(item.idx)}
        <span class="palette-label">
          ${isCurrent ? "● " : ""}${highlightMatch(item.name, q)}${sub ? `<span class="palette-sub">${escapeHtml(sub)}</span>` : ""}
        </span>
      `;
      el.title = `${item.name}${item.branch ? ` · ⎇ ${item.branch}` : ""}\n${item.key}`;

      el.addEventListener("click", () => switchTo(item.idx));
      el.addEventListener("mouseenter", () => {
        if (selectedIdx === idx) return;
        selectedIdx = idx;
        updateSelection();
      });
      results.appendChild(el);
    });

    if (ranked.length === 0) {
      results.innerHTML = '<div class="ui-empty">No matching workspaces</div>';
    }
  }

  function updateSelection() {
    results.querySelectorAll<HTMLElement>(".palette-item").forEach((el, i) => {
      el.classList.toggle("selected", i === selectedIdx);
    });
  }

  function filter() {
    ranked = rankItems(allItems, input.value, mru);
    selectedIdx = 0;
    renderResults();
  }

  async function switchTo(idx: number) {
    closeWorkspaceSwitcher();
    try {
      const detail = await ipc.switchWorkspace(idx);
      appState.setActiveWorkspace(idx, detail);
    } catch (err) {
      reportError("Workspace switch failed", err);
    }
  }

  input.addEventListener("input", filter);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, ranked.length - 1);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (ranked[selectedIdx]) switchTo(ranked[selectedIdx].idx);
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

/** Bold the matched characters (fuzzy: each query char in order). */
function highlightMatch(text: string, query: string): string {
  if (!query) return escapeHtml(text);
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  let out = "";
  let qi = 0;
  for (let i = 0; i < text.length; i++) {
    if (qi < q.length && t[i] === q[qi]) {
      out += `<strong>${escapeHtml(text[i])}</strong>`;
      qi++;
    } else {
      out += escapeHtml(text[i]);
    }
  }
  return qi === q.length ? out : escapeHtml(text);
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
