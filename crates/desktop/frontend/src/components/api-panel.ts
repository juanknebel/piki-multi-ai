import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getShortcutKey } from "../shortcuts";
import type { ApiResponseResult, ApiHistoryEntryDto } from "../ipc";

interface ApiInstance {
  tabId: string;
  wsIdx: number;
  element: HTMLDivElement;
  editorEl: HTMLTextAreaElement;
  responseEl: HTMLDivElement;
  responseToolbar: HTMLDivElement;
  statusEl: HTMLSpanElement;
  loading: boolean;
  responses: ApiResponseResult[];
  searchActive: boolean;
  jqActive: boolean;
  jqFilter: string;
}

const instances = new Map<string, ApiInstance>();
let mainContent: HTMLElement;

export function initApiPanel(container: HTMLElement) {
  mainContent = container;
}

export function hideApiPanels() {
  for (const inst of instances.values()) {
    inst.element.style.display = "none";
  }
}

export function destroyApiPanel(tabId: string) {
  const inst = instances.get(tabId);
  if (inst) {
    inst.element.remove();
    instances.delete(tabId);
  }
}

export function showApiPanel(tabId: string, wsIdx: number) {
  let inst = instances.get(tabId);
  if (!inst) {
    inst = createApiPanel(tabId, wsIdx);
    instances.set(tabId, inst);
  }
  inst.wsIdx = wsIdx;
  inst.element.style.display = "flex";
}

function createApiPanel(tabId: string, wsIdx: number): ApiInstance {
  const el = document.createElement("div");
  el.className = "api-panel";

  el.innerHTML = `
    <div class="api-toolbar">
      <button class="api-btn api-send-btn" title="Send Request (Ctrl+S)">Send</button>
      <button class="api-btn api-history-btn" title="Request History (Ctrl+H)">History</button>
      <span class="api-status"></span>
    </div>
    <div class="api-split">
      <div class="api-editor-pane">
        <div class="api-pane-header">Request</div>
        <textarea class="api-editor" spellcheck="false" placeholder="# Enter HTTP request in Hurl syntax\n# Examples:\n\nGET https://httpbin.org/get\n\nPOST https://httpbin.org/post\nContent-Type: application/json\n\n{&quot;key&quot;: &quot;value&quot;}"></textarea>
      </div>
      <div class="api-response-pane">
        <div class="api-pane-header api-response-pane-header">
          <span>Response</span>
          <div class="api-response-actions">
            <button class="api-resp-btn api-copy-btn" title="Copy body (Ctrl+C)">Copy</button>
            <button class="api-resp-btn api-search-btn" title="Search (Ctrl+F)">Search</button>
            <button class="api-resp-btn api-jq-btn" title="jq filter (${getShortcutKey("api-jq-filter")})">jq</button>
          </div>
        </div>
        <div class="api-search-bar" style="display:none">
          <input class="api-search-input" type="text" placeholder="Search in response..." />
          <span class="api-search-count"></span>
          <button class="api-search-prev" title="Previous">&uarr;</button>
          <button class="api-search-next" title="Next">&darr;</button>
          <button class="api-search-close">&times;</button>
        </div>
        <div class="api-jq-bar" style="display:none">
          <input class="api-jq-input" type="text" placeholder="jq filter (e.g. .data[] | .name)" />
          <button class="api-jq-run api-btn">Run</button>
          <button class="api-jq-reset api-resp-btn">Reset</button>
          <button class="api-jq-close">&times;</button>
        </div>
        <div class="api-response-body"><div class="api-response-empty">Send a request to see the response</div></div>
      </div>
    </div>
  `;

  mainContent.appendChild(el);

  const editorEl = el.querySelector<HTMLTextAreaElement>(".api-editor")!;
  const responseEl = el.querySelector<HTMLDivElement>(".api-response-body")!;
  const responseToolbar = el.querySelector<HTMLDivElement>(".api-response-actions")!;
  const statusEl = el.querySelector<HTMLSpanElement>(".api-status")!;

  const inst: ApiInstance = {
    tabId,
    wsIdx,
    element: el,
    editorEl,
    responseEl,
    responseToolbar,
    statusEl,
    loading: false,
    responses: [],
    searchActive: false,
    jqActive: false,
    jqFilter: "",
  };

  // Send button
  el.querySelector(".api-send-btn")!.addEventListener("click", () => sendRequest(inst));
  // History button
  el.querySelector(".api-history-btn")!.addEventListener("click", () => showHistory(inst));
  // Copy button
  el.querySelector(".api-copy-btn")!.addEventListener("click", () => copyResponseBody(inst));
  // Search button
  el.querySelector(".api-search-btn")!.addEventListener("click", () => toggleSearch(inst));
  // jq button
  el.querySelector(".api-jq-btn")!.addEventListener("click", () => toggleJq(inst));

  // Search bar events
  setupSearchBar(inst);
  // jq bar events
  setupJqBar(inst);

  // Keyboard shortcuts on editor
  editorEl.addEventListener("keydown", (e) => {
    if (e.key === "s" && e.ctrlKey) {
      e.preventDefault();
      sendRequest(inst);
    } else if (e.key === "h" && e.ctrlKey) {
      e.preventDefault();
      showHistory(inst);
    } else if (e.key === "Tab") {
      e.preventDefault();
      const start = editorEl.selectionStart;
      const end = editorEl.selectionEnd;
      editorEl.value = editorEl.value.substring(0, start) + "  " + editorEl.value.substring(end);
      editorEl.selectionStart = editorEl.selectionEnd = start + 2;
    }
  });

  // Global keyboard shortcuts on the panel
  el.addEventListener("keydown", (e) => {
    // Don't intercept when typing in editor or overlay inputs
    const tag = (e.target as HTMLElement).tagName;
    const isEditorOrInput = tag === "TEXTAREA" || tag === "INPUT";

    if (e.key === "c" && e.ctrlKey && !isEditorOrInput) {
      e.preventDefault();
      copyResponseBody(inst);
    } else if (e.key === "f" && e.ctrlKey) {
      e.preventDefault();
      toggleSearch(inst);
    }
  });

  // Listen for global jq shortcut
  document.addEventListener("toggle-jq", () => {
    if (inst.element.style.display !== "none") toggleJq(inst);
  });

  return inst;
}

// ── Send request ──────────────────────────────

async function sendRequest(inst: ApiInstance) {
  const text = inst.editorEl.value.trim();
  if (!text || inst.loading) return;

  inst.loading = true;
  inst.statusEl.textContent = "Sending...";
  inst.statusEl.className = "api-status api-status-loading";
  inst.responseEl.innerHTML = '<div class="api-response-loading">Sending request...</div>';

  try {
    const results = await ipc.sendApiRequest(inst.wsIdx, text);
    inst.responses = results;
    renderResponses(inst);
  } catch (err) {
    inst.responseEl.innerHTML = `<div class="api-response-error">Error: ${escapeHtml(String(err))}</div>`;
    inst.statusEl.textContent = "Error";
    inst.statusEl.className = "api-status api-status-error";
  } finally {
    inst.loading = false;
  }
}

function renderResponses(inst: ApiInstance, bodyOverride?: string) {
  const el = inst.responseEl;
  el.innerHTML = "";

  if (inst.responses.length === 0) {
    el.innerHTML = '<div class="api-response-empty">No response</div>';
    inst.statusEl.textContent = "";
    return;
  }

  for (let i = 0; i < inst.responses.length; i++) {
    const r = inst.responses[i];
    const section = document.createElement("div");
    section.className = "api-response-section";

    // Header
    const header = document.createElement("div");
    header.className = "api-response-header";
    const statusClass = statusCssClass(r.status);
    header.innerHTML = `
      <span class="api-response-badge ${statusClass}">${r.status || "ERR"}</span>
      <span class="api-response-method">${escapeHtml(r.method)}</span>
      <span class="api-response-url">${escapeHtml(r.url)}</span>
      <span class="api-response-time">${r.elapsed_ms}ms</span>
    `;
    section.appendChild(header);

    // Headers toggle
    if (r.headers) {
      const headersToggle = document.createElement("details");
      headersToggle.className = "api-response-headers-toggle";
      headersToggle.innerHTML = `<summary>Headers</summary><pre class="api-response-headers-content">${escapeHtml(r.headers)}</pre>`;
      section.appendChild(headersToggle);
    }

    // jq badge when filtered
    if (i === 0 && bodyOverride !== undefined && inst.jqFilter) {
      const badge = document.createElement("div");
      badge.className = "api-jq-badge";
      badge.textContent = `jq: ${inst.jqFilter}`;
      section.appendChild(badge);
    }

    // Body (use override for jq-filtered output, only for first response)
    const displayBody = (i === 0 && bodyOverride !== undefined) ? bodyOverride : r.body;
    const body = document.createElement("pre");
    body.className = "api-response-content";
    body.innerHTML = highlightJson(displayBody);
    section.appendChild(body);

    el.appendChild(section);
  }

  // Update status bar with first response
  const first = inst.responses[0];
  const sc = statusCssClass(first.status);
  inst.statusEl.textContent = `${first.status || "Error"} - ${first.elapsed_ms}ms`;
  inst.statusEl.className = `api-status ${sc}`;
}

function statusCssClass(status: number): string {
  if (status >= 200 && status < 300) return "api-status-2xx";
  if (status >= 400 && status < 500) return "api-status-4xx";
  if (status >= 500) return "api-status-5xx";
  return "api-status-error";
}

// ── Copy response ─────────────────────────────

function copyResponseBody(inst: ApiInstance) {
  if (inst.responses.length === 0) return;
  const allBodies = inst.responses.map((r) => r.body).join("\n\n---\n\n");
  writeText(allBodies)
    .then(() => toast("Response copied", "success"))
    .catch(() => {});
}

// ── Search in response ────────────────────────

function toggleSearch(inst: ApiInstance) {
  const bar = inst.element.querySelector<HTMLDivElement>(".api-search-bar")!;
  if (inst.searchActive) {
    bar.style.display = "none";
    inst.searchActive = false;
    clearSearchHighlights(inst);
  } else {
    bar.style.display = "flex";
    inst.searchActive = true;
    // Close jq if open
    if (inst.jqActive) {
      inst.element.querySelector<HTMLDivElement>(".api-jq-bar")!.style.display = "none";
      inst.jqActive = false;
    }
    bar.querySelector<HTMLInputElement>(".api-search-input")!.focus();
  }
}

function setupSearchBar(inst: ApiInstance) {
  const bar = inst.element.querySelector<HTMLDivElement>(".api-search-bar")!;
  const input = bar.querySelector<HTMLInputElement>(".api-search-input")!;
  const countEl = bar.querySelector<HTMLSpanElement>(".api-search-count")!;
  let matches: Element[] = [];
  let currentIdx = -1;

  function doSearch() {
    clearSearchHighlights(inst);
    matches = [];
    currentIdx = -1;
    const query = input.value.trim().toLowerCase();
    if (!query) {
      countEl.textContent = "";
      return;
    }

    // Find all text nodes in response content and wrap matches
    const contentEls = inst.responseEl.querySelectorAll<HTMLPreElement>(".api-response-content");
    contentEls.forEach((pre) => {
      const text = pre.textContent || "";
      const lowerText = text.toLowerCase();
      let idx = 0;
      let found = false;
      while ((idx = lowerText.indexOf(query, idx)) !== -1) {
        found = true;
        idx += query.length;
      }
      if (found) {
        // Re-render with highlights
        const html = pre.innerHTML;
        const parts: string[] = [];
        const lowerHtml = html.toLowerCase();
        let pos = 0;
        let searchIdx = 0;
        // Simple approach: highlight in the rendered text
        // Use textContent for matching, then highlight in innerHTML
        // For simplicity, do a case-insensitive replace on the visible text
        const escaped = escapeRegExp(query);
        const re = new RegExp(`(${escaped})`, "gi");
        pre.innerHTML = html.replace(re, '<mark class="api-search-match">$1</mark>');
      }
    });

    matches = Array.from(inst.responseEl.querySelectorAll(".api-search-match"));
    if (matches.length > 0) {
      currentIdx = 0;
      highlightCurrent();
    }
    countEl.textContent = matches.length > 0 ? `${currentIdx + 1}/${matches.length}` : "0 matches";
  }

  function highlightCurrent() {
    matches.forEach((m) => m.classList.remove("api-search-current"));
    if (currentIdx >= 0 && currentIdx < matches.length) {
      matches[currentIdx].classList.add("api-search-current");
      matches[currentIdx].scrollIntoView({ block: "nearest" });
      countEl.textContent = `${currentIdx + 1}/${matches.length}`;
    }
  }

  function nextMatch() {
    if (matches.length === 0) return;
    currentIdx = (currentIdx + 1) % matches.length;
    highlightCurrent();
  }

  function prevMatch() {
    if (matches.length === 0) return;
    currentIdx = (currentIdx - 1 + matches.length) % matches.length;
    highlightCurrent();
  }

  let debounce: ReturnType<typeof setTimeout> | null = null;
  input.addEventListener("input", () => {
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(doSearch, 200);
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) prevMatch(); else nextMatch();
    } else if (e.key === "Escape") {
      e.preventDefault();
      toggleSearch(inst);
    }
  });

  bar.querySelector(".api-search-next")!.addEventListener("click", nextMatch);
  bar.querySelector(".api-search-prev")!.addEventListener("click", prevMatch);
  bar.querySelector(".api-search-close")!.addEventListener("click", () => toggleSearch(inst));
}

function clearSearchHighlights(inst: ApiInstance) {
  // Re-render responses to remove <mark> tags
  if (inst.responses.length > 0) {
    renderResponses(inst);
  }
}

// ── jq filter ─────────────────────────────────

function toggleJq(inst: ApiInstance) {
  const bar = inst.element.querySelector<HTMLDivElement>(".api-jq-bar")!;
  if (inst.jqActive) {
    bar.style.display = "none";
    inst.jqActive = false;
  } else {
    bar.style.display = "flex";
    inst.jqActive = true;
    // Close search if open
    if (inst.searchActive) {
      inst.element.querySelector<HTMLDivElement>(".api-search-bar")!.style.display = "none";
      inst.searchActive = false;
      clearSearchHighlights(inst);
    }
    bar.querySelector<HTMLInputElement>(".api-jq-input")!.focus();
  }
}

function setupJqBar(inst: ApiInstance) {
  const bar = inst.element.querySelector<HTMLDivElement>(".api-jq-bar")!;
  const input = bar.querySelector<HTMLInputElement>(".api-jq-input")!;

  const runBtn = bar.querySelector<HTMLButtonElement>(".api-jq-run")!;

  async function runJq() {
    if (inst.responses.length === 0) return;
    const filter = input.value.trim();
    if (!filter) {
      inst.jqFilter = "";
      renderResponses(inst);
      return;
    }

    runBtn.disabled = true;
    runBtn.textContent = "Running...";

    const body = inst.responses[0].body;
    try {
      const result = await ipc.jqFilter(body, filter);
      inst.jqFilter = filter;
      renderResponses(inst, result);
    } catch (err) {
      toast(`jq error: ${err}`, "error");
    } finally {
      runBtn.disabled = false;
      runBtn.textContent = "Run";
    }
  }

  function resetJq() {
    input.value = "";
    inst.jqFilter = "";
    renderResponses(inst);
  }

  bar.querySelector(".api-jq-run")!.addEventListener("click", runJq);
  bar.querySelector(".api-jq-reset")!.addEventListener("click", resetJq);
  bar.querySelector(".api-jq-close")!.addEventListener("click", () => toggleJq(inst));

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      runJq();
    } else if (e.key === "Escape") {
      e.preventDefault();
      toggleJq(inst);
    }
  });
}

// ── History overlay ───────────────────────────

async function showHistory(inst: ApiInstance) {
  inst.element.querySelector(".api-history-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "api-history-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "api-history-dialog";

  dialog.innerHTML = `
    <div class="api-history-header">
      <span class="api-history-title">API History</span>
      <button class="api-history-close">&times;</button>
    </div>
    <input class="api-history-search" type="text" placeholder="Search history (FTS)..." autofocus />
    <div class="api-history-list"></div>
    <div class="api-history-footer">Enter = load | Delete = remove | Esc = close</div>
  `;

  backdrop.appendChild(dialog);
  inst.element.appendChild(backdrop);

  const searchInput = dialog.querySelector<HTMLInputElement>(".api-history-search")!;
  const listEl = dialog.querySelector<HTMLDivElement>(".api-history-list")!;
  let entries: ApiHistoryEntryDto[] = [];
  let selectedIdx = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  const close = () => backdrop.remove();
  dialog.querySelector(".api-history-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });

  async function loadEntries(query?: string) {
    try {
      entries = (query && query.trim())
        ? await ipc.searchApiHistory(inst.wsIdx, query.trim(), 50)
        : await ipc.loadApiHistory(inst.wsIdx, 50);
    } catch {
      entries = [];
    }
    selectedIdx = 0;
    renderEntries();
  }

  function renderEntries() {
    listEl.innerHTML = "";
    if (entries.length === 0) {
      listEl.innerHTML = '<div class="api-history-empty">No history entries</div>';
      return;
    }

    entries.forEach((entry, i) => {
      const el = document.createElement("div");
      el.className = `api-history-entry${i === selectedIdx ? " selected" : ""}`;
      const sc = statusCssClass(entry.status);
      const ts = entry.created_at ? entry.created_at.replace("T", " ").substring(0, 19) : "";

      el.innerHTML = `
        <span class="api-history-badge ${sc}">${entry.status || "ERR"}</span>
        <span class="api-history-method">${escapeHtml(entry.method)}</span>
        <span class="api-history-url">${escapeHtml(entry.url)}</span>
        <span class="api-history-time">${entry.elapsed_ms}ms</span>
        <span class="api-history-date">${ts}</span>
      `;

      el.addEventListener("click", () => loadEntry(entry));
      el.addEventListener("mouseenter", () => {
        listEl.querySelector(".api-history-entry.selected")?.classList.remove("selected");
        el.classList.add("selected");
        selectedIdx = i;
      });
      listEl.appendChild(el);
    });
  }

  function loadEntry(entry: ApiHistoryEntryDto) {
    inst.editorEl.value = entry.request_text;
    inst.responses = [{
      status: entry.status, elapsed_ms: entry.elapsed_ms,
      body: entry.response_body, headers: entry.response_headers,
      method: entry.method, url: entry.url,
    }];
    renderResponses(inst);
    close();
  }

  searchInput.addEventListener("input", () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => loadEntries(searchInput.value), 300);
  });

  function updateSelection(newIdx: number) {
    listEl.querySelector(".api-history-entry.selected")?.classList.remove("selected");
    selectedIdx = newIdx;
    const items = listEl.querySelectorAll(".api-history-entry");
    if (items[selectedIdx]) {
      items[selectedIdx].classList.add("selected");
      items[selectedIdx].scrollIntoView({ block: "nearest" });
    }
  }

  searchInput.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      updateSelection(Math.min(selectedIdx + 1, entries.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      updateSelection(Math.max(selectedIdx - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (entries[selectedIdx]) loadEntry(entries[selectedIdx]);
    } else if (e.key === "Delete") {
      e.preventDefault();
      if (entries[selectedIdx]?.id != null) {
        const id = entries[selectedIdx].id!;
        ipc.deleteApiHistoryEntry(id).then(() => {
          entries.splice(selectedIdx, 1);
          if (selectedIdx >= entries.length) selectedIdx = Math.max(0, entries.length - 1);
          renderEntries();
          toast("Entry deleted", "info");
        }).catch(() => {});
      }
    } else if (e.key === "Escape") {
      close();
    }
  });

  await loadEntries();
  searchInput.focus();
}

// ── JSON syntax highlighting ──────────────────

function highlightJson(text: string): string {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
    return escapeHtml(text);
  }

  return text.split("\n").map((line) => {
    let result = "";
    let i = 0;
    while (i < line.length) {
      const ch = line[i];

      if (ch === '"') {
        let j = i + 1;
        while (j < line.length && line[j] !== '"') {
          if (line[j] === "\\") j++;
          j++;
        }
        j++;
        const str = line.substring(i, j);
        const rest = line.substring(j).trimStart();
        if (rest.startsWith(":")) {
          result += `<span class="json-key">${escapeHtml(str)}</span>`;
        } else {
          result += `<span class="json-string">${escapeHtml(str)}</span>`;
        }
        i = j;
        continue;
      }

      if ((ch >= "0" && ch <= "9") || ch === "-") {
        let j = i + 1;
        while (j < line.length && /[\d.eE+\-]/.test(line[j])) j++;
        result += `<span class="json-number">${escapeHtml(line.substring(i, j))}</span>`;
        i = j;
        continue;
      }

      if (line.substring(i, i + 4) === "true" || line.substring(i, i + 5) === "false") {
        const len = line[i] === "t" ? 4 : 5;
        result += `<span class="json-bool">${line.substring(i, i + len)}</span>`;
        i += len;
        continue;
      }
      if (line.substring(i, i + 4) === "null") {
        result += `<span class="json-null">null</span>`;
        i += 4;
        continue;
      }

      result += escapeHtml(ch);
      i++;
    }
    return result;
  }).join("\n");
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
