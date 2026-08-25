// Branch switcher: a palette-style list of the workspace's branches (local
// first, then remote-tracking ones without a local counterpart), fuzzy
// filterable, the current one marked. Picking a row runs `git checkout`
// (`--track` for a remote); git's own refusal — dirty worktree that would be
// clobbered, branch used by another worktree — is shown as the error.
// Opened from the status bar's `⎇ branch`, the Git menu, the palette and
// the `switch-branch` shortcut.

import { appState } from "../../state";
import * as ipc from "../../ipc";
import { reportError, toast } from "../toast";
import { fuzzyScore } from "../fuzzy";
import { runExclusive } from "../../in-flight";

let pickerEl: HTMLElement | null = null;

export const checkoutKey = (wsIdx: number) => `git-checkout:${wsIdx}`;

export function openBranchPicker() {
  if (pickerEl) {
    closeBranchPicker();
    return;
  }
  const ws = appState.activeWs;
  if (!ws) {
    toast("Open a workspace to switch branches", "info");
    return;
  }
  if (ws.info.workspace_type === "Project" || ws.info.origin?.kind === "Local") {
    toast("Source control is unavailable for this workspace", "info");
    return;
  }
  const wsIdx = appState.activeWorkspace;

  const backdrop = document.createElement("div");
  backdrop.className = "palette-backdrop";
  const palette = document.createElement("div");
  palette.className = "palette ui-surface";
  palette.innerHTML = `
    <input class="palette-input" type="text" placeholder="Switch branch…" autofocus />
    <div class="palette-results"><div class="ui-empty">Loading branches…</div></div>
  `;
  backdrop.appendChild(palette);
  document.body.appendChild(backdrop);
  pickerEl = backdrop;

  const input = palette.querySelector<HTMLInputElement>(".palette-input")!;
  const results = palette.querySelector<HTMLElement>(".palette-results")!;
  let all: ipc.BranchInfo[] = [];
  let aheadBehind: [number, number] | null = null;
  let shown: ipc.BranchInfo[] = [];
  let selectedIdx = 0;
  let loaded = false;

  function rank(): ipc.BranchInfo[] {
    const q = input.value.trim();
    if (!q) return all;
    return all
      .map((b) => ({ b, score: fuzzyScore(q, b.name) }))
      .filter((x): x is { b: ipc.BranchInfo; score: number } => x.score !== null)
      .sort((a, b) => b.score - a.score)
      .map((x) => x.b);
  }

  function renderResults() {
    if (!loaded) return;
    results.innerHTML = "";
    const q = input.value.trim();
    shown = rank();
    shown.forEach((b, idx) => {
      const el = document.createElement("div");
      el.className = `palette-item${idx === selectedIdx ? " selected" : ""}`;
      const sync = b.current && aheadBehind && (aheadBehind[0] > 0 || aheadBehind[1] > 0)
        ? `${aheadBehind[0] > 0 ? `↑${aheadBehind[0]}` : ""}${aheadBehind[1] > 0 ? ` ↓${aheadBehind[1]}` : ""}`.trim()
        : "";
      const sub = [b.upstream ? `→ ${b.upstream}` : null, sync || null]
        .filter((s): s is string => !!s)
        .join(" · ");
      el.innerHTML = `
        <span class="palette-category">${b.remote ? "remote" : b.current ? "current" : ""}</span>
        <span class="palette-label">
          ${b.current ? "● " : ""}${highlightMatch(b.name, q)}${sub ? `<span class="palette-sub">${escapeHtml(sub)}</span>` : ""}
        </span>
      `;
      el.title = b.remote ? `Check out ${b.name} as a new tracking branch` : b.name;
      el.addEventListener("click", () => void checkout(b));
      el.addEventListener("mouseenter", () => {
        if (selectedIdx === idx) return;
        selectedIdx = idx;
        updateSelection();
      });
      results.appendChild(el);
    });
    if (shown.length === 0) {
      results.innerHTML = '<div class="ui-empty">No matching branches</div>';
    }
  }

  function updateSelection() {
    results.querySelectorAll<HTMLElement>(".palette-item").forEach((el, i) => {
      el.classList.toggle("selected", i === selectedIdx);
    });
  }

  async function checkout(b: ipc.BranchInfo) {
    if (b.current) {
      closeBranchPicker();
      return;
    }
    closeBranchPicker();
    const ran = await runExclusive(checkoutKey(wsIdx), async () => {
      try {
        const result = await ipc.gitCheckoutBranch(wsIdx, b.name, b.remote);
        appState.updateFiles(wsIdx, result.files, result.ahead_behind, result.branch);
        toast(`Switched to ${result.branch ?? b.name}`, "success");
      } catch (err) {
        reportError("Checkout failed", err);
      }
      return true;
    });
    if (!ran) toast("A checkout is already in progress", "info");
  }

  input.addEventListener("input", () => {
    selectedIdx = 0;
    renderResults();
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, Math.max(shown.length - 1, 0));
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      updateSelection();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (shown[selectedIdx]) void checkout(shown[selectedIdx]);
    } else if (e.key === "Escape") {
      closeBranchPicker();
    }
  });
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeBranchPicker();
  });
  input.focus();

  void (async () => {
    try {
      const list = await ipc.gitListBranches(wsIdx, true);
      if (pickerEl !== backdrop) return; // closed while loading
      all = list.branches;
      aheadBehind = list.ahead_behind;
      loaded = true;
      // Current branch first so Enter on an empty query is a no-op, not a surprise.
      selectedIdx = Math.max(0, all.findIndex((b) => b.current));
      renderResults();
      results.querySelector(".palette-item.selected")?.scrollIntoView({ block: "nearest" });
    } catch (err) {
      closeBranchPicker();
      reportError("Could not list branches", err);
    }
  })();
}

export function closeBranchPicker() {
  pickerEl?.remove();
  pickerEl = null;
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
