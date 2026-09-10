import { appState } from "../state";
import type { PaneId } from "../pane-tree";
import { toast } from "./toast";
import { icon } from "./icons";
import { createDropdown, type DropdownHandle } from "./dropdown";
import { PORT_PRESETS, normalizeUrl, isLocalUrl, probeUrl } from "./web-preview-presets";

type ProbeStatus = "idle" | "checking" | "ok" | "fail";

interface WebPreviewInstance {
  tabId: string;
  element: HTMLDivElement;
  iframeHost: HTMLDivElement;
  urlInput: HTMLInputElement;
  statusDot: HTMLSpanElement;
  warningBanner: HTMLDivElement;
  presetDropdown: DropdownHandle;
  currentUrl: string;
  nonce: number;
  probeAbort: AbortController | null;
}

const instances = new Map<string, WebPreviewInstance>();
/** URL to load when a (restored) preview mounts for the first time. */
const pendingUrls = new Map<string, string>();
let mainContent: HTMLElement;

export function initWebPreviewPanel(container: HTMLElement) {
  mainContent = container;
}

/** Focus the WebPreview of the active workspace, or create one: as a new
 *  top-level tab, or into the blank pane `paneId` when given. Frontend-only:
 *  synthesizes the tab id and uses `appState.addTab` / `setPaneContent`
 *  directly (no `ipc.spawnTab`), same pattern as the editor tabs. It is a
 *  singleton; an existing one elsewhere is handled by the caller
 *  (`open-content.ts` offers Move here / Go there) — this only dedups the
 *  tab-level path. */
export function openWebPreviewTab(opts: { paneId?: PaneId; url?: string } = {}) {
  const ws = appState.activeWs;
  if (!ws) {
    toast("Create a workspace first", "error");
    return;
  }
  const existingIdx = ws.tabs.findIndex((t) => t.provider === "WebPreview");
  if (existingIdx >= 0 && !opts.paneId) {
    appState.setActiveTab(existingIdx);
    return;
  }
  const tabId = `web-${Date.now()}`;
  if (opts.url) registerWebPreview(tabId, opts.url);
  const tab = { id: tabId, provider: "WebPreview" as const, alive: true };
  if (opts.paneId) appState.setPaneContent(opts.paneId, tab);
  else appState.addTabToRoot(appState.activeWorkspace, tab);
}

/** Pre-register the URL a preview should load on first mount (layout
 *  restore); a no-op for an id whose panel already exists. */
export function registerWebPreview(tabId: string, url: string) {
  if (instances.has(tabId) || !url) return;
  pendingUrls.set(tabId, url);
}

/** The URL a preview currently shows (or was registered to show), "" if
 *  none — what the layout snapshot persists. */
export function getWebPreviewUrl(tabId: string): string | null {
  const inst = instances.get(tabId);
  if (inst) return inst.currentUrl;
  return pendingUrls.get(tabId) ?? null;
}

export function mountWebPreviewInto(tabId: string, host: HTMLElement) {
  let inst = instances.get(tabId);
  if (!inst) {
    inst = createPanel(tabId);
    instances.set(tabId, inst);
    const url = pendingUrls.get(tabId);
    pendingUrls.delete(tabId);
    if (url) void tryLoad(inst, url);
  }
  if (inst.element.parentElement !== host) {
    host.appendChild(inst.element);
  }
  inst.element.style.display = "flex";
}

export function unmountWebPreview(tabId: string) {
  const inst = instances.get(tabId);
  if (inst) inst.element.style.display = "none";
}

export function destroyWebPreviewPanel(tabId: string) {
  const inst = instances.get(tabId);
  if (!inst) return;
  inst.probeAbort?.abort();
  const ifr = inst.iframeHost.querySelector("iframe");
  if (ifr) {
    ifr.src = "about:blank";
    ifr.remove();
  }
  inst.element.remove();
  instances.delete(tabId);
  pendingUrls.delete(tabId);
}

function setStatus(inst: WebPreviewInstance, status: ProbeStatus) {
  inst.statusDot.dataset.status = status;
  const titles: Record<ProbeStatus, string> = {
    idle: "Idle — pick a port or type a URL",
    checking: "Checking…",
    ok: "Server responding",
    fail: "No response",
  };
  inst.statusDot.title = titles[status];
}

function replaceIframe(inst: WebPreviewInstance, url: string) {
  inst.nonce += 1;
  const old = inst.iframeHost.querySelector("iframe");
  if (old) {
    old.src = "about:blank";
    old.remove();
  }
  const ifr = document.createElement("iframe");
  ifr.className = "web-preview-iframe";
  ifr.setAttribute("allow", "clipboard-read; clipboard-write; fullscreen");
  ifr.setAttribute("referrerpolicy", "no-referrer");
  ifr.dataset.nonce = String(inst.nonce);
  ifr.src = url;
  inst.iframeHost.appendChild(ifr);
}

async function tryLoad(inst: WebPreviewInstance, raw: string) {
  const url = normalizeUrl(raw);
  if (!url) return;
  inst.urlInput.value = url;
  inst.warningBanner.hidden = isLocalUrl(url);

  inst.probeAbort?.abort();
  const abort = new AbortController();
  inst.probeAbort = abort;
  setStatus(inst, "checking");
  try {
    await probeUrl(url, 900, abort.signal);
    if (abort.signal.aborted) return;
    setStatus(inst, "ok");
    inst.currentUrl = url;
    replaceIframe(inst, url);
  } catch (err) {
    if (abort.signal.aborted && abort !== inst.probeAbort) return;
    setStatus(inst, "fail");
    const host = (() => { try { return new URL(url).host; } catch { return url; } })();
    toast(`No response from ${host}`, "error");
  }
}

function createPanel(tabId: string): WebPreviewInstance {
  const el = document.createElement("div");
  el.className = "web-preview-panel";
  el.innerHTML = `
    <div class="web-preview-toolbar">
      <span class="web-preview-status-dot" data-status="idle" title="Idle"></span>
      <input class="web-preview-url-input ui-input" type="text" spellcheck="false"
             placeholder="localhost:3000 or https://example.com" />
      <button data-variant="secondary" class="web-preview-go ui-btn" title="Load URL">Go</button>
      <div class="web-preview-preset-wrap"></div>
      <button data-variant="secondary" data-icon data-size="md" class="web-preview-reload ui-btn" title="Reload" aria-label="Reload">${icon("refresh")}</button>
    </div>
    <div class="web-preview-warning-banner" hidden>
      Non-local URL — many sites refuse to embed (X-Frame-Options).
    </div>
    <div class="web-preview-iframe-host">
      <div class="web-preview-empty ui-empty">Pick a port from the dropdown, or type a URL and press Enter.</div>
    </div>
  `;

  const inst: WebPreviewInstance = {
    tabId,
    element: el,
    iframeHost: el.querySelector<HTMLDivElement>(".web-preview-iframe-host")!,
    urlInput: el.querySelector<HTMLInputElement>(".web-preview-url-input")!,
    statusDot: el.querySelector<HTMLSpanElement>(".web-preview-status-dot")!,
    warningBanner: el.querySelector<HTMLDivElement>(".web-preview-warning-banner")!,
    presetDropdown: createDropdown(
      [{ value: "", label: "Presets…" }, ...PORT_PRESETS.map((p) => ({
        value: String(p.port),
        label: p.label,
      }))],
      "",
      "min-width:170px",
    ),
    currentUrl: "",
    nonce: 0,
    probeAbort: null,
  };

  el.querySelector<HTMLDivElement>(".web-preview-preset-wrap")!.appendChild(inst.presetDropdown.container);

  // Preset selection
  let presetDebounce: ReturnType<typeof setTimeout> | null = null;
  inst.presetDropdown.container.addEventListener("change", () => {
    const port = inst.presetDropdown.value;
    if (!port) return;
    if (presetDebounce) clearTimeout(presetDebounce);
    presetDebounce = setTimeout(() => {
      void tryLoad(inst, `localhost:${port}`);
      // Reset dropdown label so the same preset can be re-picked.
      inst.presetDropdown.value = "";
    }, 80);
  });

  // Go button + Enter
  el.querySelector<HTMLButtonElement>(".web-preview-go")!.addEventListener("click", () => {
    void tryLoad(inst, inst.urlInput.value);
  });
  inst.urlInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void tryLoad(inst, inst.urlInput.value);
    }
  });

  // Reload
  el.querySelector<HTMLButtonElement>(".web-preview-reload")!.addEventListener("click", () => {
    if (inst.currentUrl) replaceIframe(inst, inst.currentUrl);
  });

  return inst;
}
