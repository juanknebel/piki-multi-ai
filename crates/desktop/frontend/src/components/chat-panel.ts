import * as ipc from "../ipc";
import { settingsStore } from "../settings";
import { activityBarWidth, clampChatWidth, visibleSidebarWidth } from "../layout-budget";
import { showConfirm } from "./confirm";
import { toast } from "./toast";
import { renderMarkdown } from "./markdown-viewer";
import { createDropdown, type DropdownHandle } from "./dropdown";
import { openContextMenu } from "./context-menu";
import { reportError } from "./toast";
import { icon, type IconName } from "./icons";
import { appState } from "../state";
import { allLeaves } from "../pane-tree";
import { getTabLabel } from "../types";
import { activeTerminalInstance } from "./terminal-panel";
import { getCodeEditorFilePath, getCodeEditorSelection } from "./code-editor-panel";
import { getMarkdownEditorFilePath } from "./markdown-editor-panel";
import {
  RESULT_COLLAPSE_LINES,
  appendToDraft,
  contextChoices,
  diffLinesToText,
  fenceBlock,
  formatDurationMs,
  parseToolMessage,
  prettyJson,
  type ContextKind,
  type ToolCard,
} from "../chat-context";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface ChatMsg {
  role: "user" | "assistant" | "tool";
  content: string;
  /** Structured card data for `role: "tool"` (chat-context.ts). */
  tool?: ToolCard;
}

let container: HTMLElement;
let messagesEl: HTMLDivElement;
let inputEl: HTMLTextAreaElement;
let sendBtn: HTMLButtonElement;
let modelDropdown: DropdownHandle | null = null;
let modelBarEl: HTMLDivElement;
let streamingEl: HTMLDivElement | null = null;
let unlistenToken: UnlistenFn | null = null;
let unlistenAgent: UnlistenFn | null = null;
let agentToggleBtn: HTMLButtonElement;
let messages: ChatMsg[] = [];
let streaming = false;
let agentMode = false;
let modelsRequested = false;
let currentConfig: ipc.ChatConfig = {
  provider: "ollama",
  server_type: "Ollama",
  model: "",
  base_url: "http://localhost:11434",
  system_prompt: null,
};

export async function initChatPanel(el: HTMLElement) {
  container = el;

  // Header
  const header = document.createElement("div");
  header.className = "chat-header";
  header.innerHTML = `
    <span class="chat-header-title">AI Chat</span>
    <button data-variant="ghost" data-icon class="chat-header-btn ui-btn chat-settings-btn" title="Chat settings">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
        <path d="M8 10a2 2 0 100-4 2 2 0 000 4z" stroke="currentColor" stroke-width="1.2"/>
        <path d="M13.5 8c0-.3-.2-.6-.4-.8l1-1.6-.8-1.4-1.8.4c-.4-.3-.8-.6-1.3-.7L9.8 2H8.2l-.4 1.9c-.5.1-.9.4-1.3.7l-1.8-.4-.8 1.4 1 1.6c-.2.2-.4.5-.4.8s.2.6.4.8l-1 1.6.8 1.4 1.8-.4c.4.3.8.6 1.3.7l.4 1.9h1.6l.4-1.9c.5-.1.9-.4 1.3-.7l1.8.4.8-1.4-1-1.6c.2-.2.4-.5.4-.8z" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </button>
    <button data-variant="ghost" data-icon class="chat-header-btn ui-btn chat-clear-btn" title="Clear conversation">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
        <path d="M2 4h12M5 4V3a1 1 0 011-1h4a1 1 0 011 1v1m2 0v9a1 1 0 01-1 1H4a1 1 0 01-1-1V4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
      </svg>
    </button>
  `;
  // Agent mode toggle button
  agentToggleBtn = document.createElement("button");
  agentToggleBtn.className = "chat-header-btn chat-agent-btn ui-btn";
  agentToggleBtn.dataset.variant = "ghost";
  agentToggleBtn.dataset.size = "sm";
  agentToggleBtn.title = "Toggle Agent mode (tool-use)";
  agentToggleBtn.textContent = "Agent";
  agentToggleBtn.addEventListener("click", toggleAgentMode);
  header.insertBefore(agentToggleBtn, header.querySelector(".chat-settings-btn")!);

  header.querySelector(".chat-settings-btn")!.addEventListener("click", showChatSettings);
  header.querySelector(".chat-clear-btn")!.addEventListener("click", clearChat);
  container.appendChild(header);

  // Model selector bar
  modelBarEl = document.createElement("div");
  modelBarEl.className = "chat-model-bar";

  const modelLabel = document.createElement("span");
  modelLabel.className = "chat-model-label";
  modelLabel.textContent = "Model";
  modelBarEl.appendChild(modelLabel);

  // Placeholder dropdown — replaced when models load
  modelDropdown = createDropdown(
    [{ value: "", label: "Loading\u2026" }],
    "",
    "flex:1;min-width:0",
  );
  modelBarEl.appendChild(modelDropdown.container);

  const refreshBtn = document.createElement("button");
  refreshBtn.className = "chat-model-refresh ui-btn";
  refreshBtn.dataset.variant = "ghost";
  refreshBtn.dataset.icon = "";
  refreshBtn.title = "Refresh models";
  refreshBtn.textContent = "\u21BB";
  refreshBtn.addEventListener("click", loadModels);
  modelBarEl.appendChild(refreshBtn);

  container.appendChild(modelBarEl);

  // Messages area
  messagesEl = document.createElement("div");
  messagesEl.className = "chat-messages";
  renderEmpty();
  container.appendChild(messagesEl);

  // Input area
  const inputArea = document.createElement("div");
  inputArea.className = "chat-input-area";

  const addCtxBtn = document.createElement("button");
  addCtxBtn.className = "chat-add-context-btn ui-btn";
  addCtxBtn.dataset.variant = "ghost";
  addCtxBtn.dataset.icon = "";
  addCtxBtn.dataset.size = "md";
  addCtxBtn.title = "Add context to chat";
  addCtxBtn.setAttribute("aria-label", "Add context to chat");
  addCtxBtn.innerHTML = icon("plus");
  addCtxBtn.addEventListener("click", () => {
    const r = addCtxBtn.getBoundingClientRect();
    openContextChooser(r.left, r.top - 4);
  });
  inputArea.appendChild(addCtxBtn);

  inputEl = document.createElement("textarea");
  inputEl.className = "chat-input ui-input";
  inputEl.placeholder = "Ask a question\u2026";
  inputEl.rows = 1;
  inputEl.addEventListener("keydown", onInputKeydown);
  inputEl.addEventListener("input", autoResize);
  inputArea.appendChild(inputEl);

  sendBtn = document.createElement("button");
  sendBtn.className = "chat-send-btn ui-btn";
  sendBtn.dataset.variant = "primary";
  sendBtn.dataset.icon = "";
  sendBtn.dataset.size = "md";
  sendBtn.title = "Send (Enter)";
  sendBtn.innerHTML = `<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
    <path d="M2 8l10-5-3 5 3 5z" fill="currentColor"/>
  </svg>`;
  sendBtn.addEventListener("click", () => {
    if (streaming) {
      // The button doubles as Stop while a reply streams.
      ipc.chatStop().catch((err) => reportError("Stop failed", err));
      settlePendingApprovals("denied");
      onStreamEnd();
    } else {
      void sendMessage();
    }
  });
  inputArea.appendChild(sendBtn);

  container.appendChild(inputArea);

  // Load config and models
  try {
    currentConfig = await ipc.chatGetConfig();
  } catch {
    // Use defaults
  }

  // Load existing messages from backend
  try {
    const existing = await ipc.chatGetMessages();
    for (const msg of existing) {
      if (msg.role === "User" || msg.role === "Assistant" || msg.role === "Tool") {
        const role = msg.role === "User" ? "user" : msg.role === "Tool" ? "tool" : "assistant";
        messages.push({ role, content: msg.content, tool: role === "tool" ? toolCardFromHistory(msg.content) : undefined });
      }
    }
    if (messages.length > 0) {
      renderMessages();
    }
  } catch {
    // No messages
  }

  // Models are fetched lazily on the first open of the panel (see
  // `toggleChatPanel`): probing Ollama/llama.cpp at startup logged a
  // "Failed to list models" error on every launch for users without a
  // local LLM server, for a panel they had not opened.

  // Load agent mode state in the background
  ipc.chatGetAgentMode().then((enabled) => {
    agentMode = enabled;
    updateAgentButton();
  }).catch(() => {});

  // Subscribe to streaming tokens and structured agent activity
  unlistenToken = await ipc.onChatToken(onToken);
  unlistenAgent = await ipc.onChatAgentEvent(onAgentEvent);
  void unlistenToken;
  void unlistenAgent;
}

async function loadModels() {
  modelsRequested = true;
  // Remove old dropdown, show placeholder
  replaceDropdown([{ value: "", label: "Loading\u2026" }], "");

  try {
    const models = await ipc.chatListModels(currentConfig.base_url, currentConfig.server_type);

    if (models.length === 0) {
      replaceDropdown([{ value: "", label: "No models found" }], "");
      return;
    }

    const options = models.map((m) => ({
      value: m.name,
      label: m.size > 0 ? `${m.name} (${formatSize(m.size)})` : m.name,
    }));

    // Determine initial selection
    let initial = "";
    if (currentConfig.model && models.some((m) => m.name === currentConfig.model)) {
      initial = currentConfig.model;
    } else {
      initial = models[0].name;
      currentConfig.model = initial;
      saveConfig();
    }

    replaceDropdown(options, initial);
  } catch {
    const serverLabel = currentConfig.server_type === "LlamaCpp" ? "llama.cpp" : "Ollama";
    replaceDropdown([{ value: "", label: `${serverLabel} not available` }], "");
  }
}

function replaceDropdown(
  options: { value: string; label: string }[],
  initial: string,
) {
  if (modelDropdown) {
    modelDropdown.container.remove();
  }
  modelDropdown = createDropdown(options, initial, "flex:1;min-width:0");
  modelDropdown.container.addEventListener("change", () => {
    currentConfig.model = modelDropdown!.value;
    saveConfig();
  });
  // Insert before the refresh button (last child of modelBarEl)
  const refreshBtn = modelBarEl.querySelector(".chat-model-refresh");
  if (refreshBtn) {
    modelBarEl.insertBefore(modelDropdown.container, refreshBtn);
  } else {
    modelBarEl.appendChild(modelDropdown.container);
  }
}

function saveConfig() {
  ipc.chatSetConfig(currentConfig).catch(() => {});
}

async function sendMessage() {
  const text = inputEl.value.trim();
  if (!text || streaming) return;

  if (!currentConfig.model) {
    toast("Select a model first", "error");
    return;
  }

  // Add user message
  settlePendingApprovals("denied");
  liveCards.clear();
  executingQueue = [];
  messages.push({ role: "user", content: text });
  inputEl.value = "";
  autoResize();
  renderMessages();

  // Start streaming — the send button becomes a Stop button.
  streaming = true;
  sendBtn.title = "Stop";
  sendBtn.classList.add("streaming");
  sendBtn.innerHTML = `<svg width="16" height="16" viewBox="0 0 16 16"><rect x="4" y="4" width="8" height="8" fill="currentColor"/></svg>`;
  armWatchdog();

  // Create streaming placeholder
  streamingEl = document.createElement("div");
  streamingEl.className = "chat-msg assistant";
  streamingEl.innerHTML = `
    <span class="chat-msg-role">assistant</span>
    <div class="chat-msg-content"><span class="chat-streaming-cursor"></span></div>
  `;
  messagesEl.appendChild(streamingEl);
  scrollToBottom();

  try {
    if (agentMode) {
      await ipc.chatSendAgentMessage(text);
    } else {
      await ipc.chatSendMessage(text);
    }
  } catch (err) {
    onStreamEnd();
    toast(`Chat error: ${err}`, "error");
  }
}

function onToken(event: { content: string; done: boolean }) {
  if (event.done) {
    // Finalize the streamed message
    if (streamingEl) {
      const contentEl = streamingEl.querySelector(".chat-msg-content")!;
      const cursor = contentEl.querySelector(".chat-streaming-cursor");
      if (cursor) cursor.remove();

      // Add to our local messages
      const text = contentEl.textContent ?? "";
      messages.push({ role: "assistant", content: text });
    }
    onStreamEnd();
    // Re-render so the finished reply gets its markdown formatting.
    renderMessages();
    return;
  }

  armWatchdog();
  if (streamingEl) {
    const contentEl = streamingEl.querySelector(".chat-msg-content")!;
    // Insert text before the cursor
    const cursor = contentEl.querySelector(".chat-streaming-cursor");
    if (cursor) {
      const textNode = document.createTextNode(event.content);
      contentEl.insertBefore(textNode, cursor);
    } else {
      contentEl.textContent += event.content;
    }
    scrollToBottom();
  }
}

/** No tokens for this long while streaming → assume the stream died. */
const STREAM_TIMEOUT_MS = 60_000;
let watchdogTimer: ReturnType<typeof setTimeout> | null = null;

function armWatchdog() {
  if (watchdogTimer) clearTimeout(watchdogTimer);
  watchdogTimer = setTimeout(() => {
    if (!streaming) return;
    // Waiting on the user, not on the model: the agent loop has its own
    // 300 s approval timeout (auto-deny), so re-check later instead.
    if (hasPendingApproval()) {
      armWatchdog();
      return;
    }
    onStreamEnd();
    toast("Chat stream timed out", "error");
  }, STREAM_TIMEOUT_MS);
}

function onStreamEnd() {
  streaming = false;
  if (watchdogTimer) {
    clearTimeout(watchdogTimer);
    watchdogTimer = null;
  }
  sendBtn.disabled = false;
  sendBtn.title = "Send (Enter)";
  sendBtn.classList.remove("streaming");
  sendBtn.innerHTML = `<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
    <path d="M2 8l10-5-3 5 3 5z" fill="currentColor"/>
  </svg>`;
  streamingEl = null;
  inputEl.focus();
}

function clearChat() {
  if (messages.length === 0) return;
  const n = messages.length;
  showConfirm({
    bodyHtml: `
      <p>Clear conversation?</p>
      <p class="ws-delete-hint">This permanently removes ${n} message${n === 1 ? "" : "s"}.</p>
    `,
    actions: [
      {
        label: "Clear",
        kind: "danger",
        isDefault: true,
        onSelect: async () => {
          settlePendingApprovals("denied");
          messages = [];
          renderEmpty();
          try {
            await ipc.chatClear();
          } catch {
            // ignore
          }
        },
      },
      { label: "Cancel", kind: "secondary" },
    ],
  });
}

function renderMessages() {
  messagesEl.innerHTML = "";
  if (messages.length === 0) {
    renderEmpty();
    return;
  }
  for (const msg of messages) {
    if (msg.role === "tool") {
      messagesEl.appendChild(renderToolCard(msg.tool ?? toolCardFromHistory(msg.content)));
      continue;
    }
    const el = document.createElement("div");
    el.className = `chat-msg ${msg.role}`;
    const body =
      msg.role === "assistant" ? renderMarkdown(msg.content) : escapeHtml(msg.content);
    el.innerHTML = `
      <span class="chat-msg-role">${msg.role}</span>
      <div class="chat-msg-content">${body}</div>
    `;
    if (msg.role === "assistant") addCopyButtons(el);
    messagesEl.appendChild(el);
  }
  scrollToBottom();
}

/** Hover copy button on each fenced code block of an assistant message. */
function addCopyButtons(el: HTMLElement) {
  el.querySelectorAll("pre").forEach((pre) => {
    const btn = document.createElement("button");
    btn.className = "chat-code-copy";
    btn.title = "Copy code";
    btn.textContent = "Copy";
    btn.addEventListener("click", () => {
      const code = pre.querySelector("code")?.textContent ?? pre.textContent ?? "";
      ipc
        .clipboardCopy(code)
        .then(() => toast("Copied to clipboard", "success"))
        .catch(() => {});
    });
    pre.appendChild(btn);
  });
}

function renderEmpty() {
  messagesEl.innerHTML = `
    <div class="ui-empty" data-fill>
      <div class="ui-empty-icon">\u{1F4AC}</div>
      <div class="chat-empty-text">
        Chat with a local AI model.<br>
        Select a model above and start typing.
      </div>
    </div>
  `;
}

function scrollToBottom() {
  requestAnimationFrame(() => {
    messagesEl.scrollTop = messagesEl.scrollHeight;
  });
}

function autoResize() {
  inputEl.style.height = "auto";
  inputEl.style.height = Math.min(inputEl.scrollHeight, 120) + "px";
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    sendMessage();
  }
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return (bytes / Math.pow(1024, i)).toFixed(1) + " " + units[i];
}

// ── Settings dialog ────────────────────────────────

function showChatSettings() {
  document.querySelector(".chat-settings-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop chat-settings-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog ui-surface";
  dialog.style.maxWidth = "480px";

  // Header
  const header = document.createElement("div");
  header.className = "ui-header";
  header.innerHTML = `
    <span class="ui-header-title">Chat Settings</span>
    <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">&times;</button>
  `;

  // Body
  const body = document.createElement("div");
  body.className = "dialog-body";
  body.style.padding = "16px";

  // Server type field
  const serverRow = document.createElement("div");
  serverRow.className = "chat-settings-row";
  serverRow.innerHTML = `<label class="chat-settings-label">Server</label>`;
  const serverDefaults: Record<ipc.ChatServerType, string> = {
    Ollama: "http://localhost:11434",
    LlamaCpp: "http://localhost:8080",
  };
  const serverDropdown = createDropdown(
    [
      { value: "Ollama", label: "Ollama" },
      { value: "LlamaCpp", label: "llama.cpp" },
    ],
    currentConfig.server_type,
  );
  serverDropdown.container.addEventListener("change", () => {
    const newType = serverDropdown.value as ipc.ChatServerType;
    const oldDefault = serverDefaults[currentConfig.server_type];
    // If URL matches old default, update to new default
    if (urlInput.value.trim() === oldDefault || urlInput.value.trim() === "") {
      urlInput.value = serverDefaults[newType];
      urlInput.placeholder = serverDefaults[newType];
    }
  });
  serverRow.appendChild(serverDropdown.container);
  body.appendChild(serverRow);

  // Base URL field
  const urlRow = document.createElement("div");
  urlRow.className = "chat-settings-row";
  urlRow.innerHTML = `<label class="chat-settings-label">Base URL</label>`;
  const urlInput = document.createElement("input");
  urlInput.className = "ui-input";
  urlInput.type = "text";
  urlInput.value = currentConfig.base_url;
  urlInput.placeholder = serverDefaults[currentConfig.server_type];
  urlRow.appendChild(urlInput);
  body.appendChild(urlRow);

  // System prompt field
  const promptRow = document.createElement("div");
  promptRow.className = "chat-settings-row";
  promptRow.innerHTML = `<label class="chat-settings-label">System prompt</label>`;
  const promptInput = document.createElement("textarea");
  promptInput.className = "ui-input chat-settings-textarea";
  promptInput.value = currentConfig.system_prompt ?? "";
  promptInput.placeholder = "Optional instructions prepended to every conversation";
  promptInput.rows = 4;
  promptRow.appendChild(promptInput);
  body.appendChild(promptRow);

  // Footer buttons
  const footer = document.createElement("div");
  footer.className = "dialog-footer";
  footer.innerHTML = `
    <button data-variant="secondary" class="ui-btn chat-settings-cancel">Cancel</button>
    <button data-variant="primary" class="ui-btn chat-settings-save">Save</button>
  `;

  dialog.appendChild(header);
  dialog.appendChild(body);
  dialog.appendChild(footer);
  backdrop.appendChild(dialog);

  // Close handlers
  const close = () => backdrop.remove();
  header.querySelector(".dialog-close")!.addEventListener("click", close);
  footer.querySelector(".chat-settings-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });

  // Save handler
  footer.querySelector(".chat-settings-save")!.addEventListener("click", async () => {
    const newUrl = urlInput.value.trim();
    const newPrompt = promptInput.value.trim();
    const newServerType = serverDropdown.value as ipc.ChatServerType;
    const serverChanged = newServerType !== currentConfig.server_type;
    const urlChanged = newUrl !== currentConfig.base_url;

    currentConfig.server_type = newServerType;
    currentConfig.base_url = newUrl || serverDefaults[newServerType];
    currentConfig.system_prompt = newPrompt || null;

    if (serverChanged) {
      // Clear model since model names differ between servers
      currentConfig.model = "";
    }

    await saveConfig();
    close();

    // Reload models if URL or server type changed
    if (urlChanged || serverChanged) {
      await loadModels();
    }
  });

  document.body.appendChild(backdrop);
  urlInput.focus();
}

// ── Agent mode ────────────────────────────────────

async function toggleAgentMode() {
  agentMode = !agentMode;
  await ipc.chatSetAgentMode(agentMode).catch(() => {});
  updateAgentButton();
  toast(agentMode ? "Agent mode ON" : "Agent mode OFF", "info");
}

function updateAgentButton() {
  if (agentToggleBtn) {
    agentToggleBtn.classList.toggle("active", agentMode);
    agentToggleBtn.title = agentMode
      ? "Agent mode ON (tool-use enabled)"
      : "Agent mode OFF (plain chat)";
  }
}

// ── Toggle ─────────────────────────────────────────

export function toggleChatPanel() {
  const app = document.getElementById("app")!;
  app.classList.toggle("chat-visible");
  if (app.classList.contains("chat-visible")) {
    if (!modelsRequested) loadModels().catch(() => {});
    inputEl?.focus();
  }
}

/** Open the panel if it is hidden (below 1000px it floats over the editor —
 *  same class, `layout.css` decides where it goes). */
function ensureChatVisible() {
  const app = document.getElementById("app")!;
  if (!app.classList.contains("chat-visible")) toggleChatPanel();
}

// ── Add context ────────────────────────────────────

/** Content id + provider of the active pane's tab, if any. */
function activeContent(): { id: string; provider: string; label: string } | null {
  const ws = appState.activeWs;
  const wt = appState.activeTabTree;
  if (!ws || !wt) return null;
  const leaf = allLeaves(wt.paneTree).find((l) => l.id === wt.activePaneId);
  const id = leaf?.contentId;
  if (!id) return null;
  const tab = ws.tabs.find((t) => t.id === id);
  if (!tab) return null;
  const provider = typeof tab.provider === "string" ? tab.provider : "Custom";
  return { id, provider, label: getTabLabel(tab, appState.getTabShellState(id)?.title) };
}

/** Workspace-relative path of the active editor/markdown tab, or null. */
function activeFilePath(): string | null {
  const c = activeContent();
  if (!c) return null;
  if (c.provider === "CodeEditor") return getCodeEditorFilePath(c.id);
  if (c.provider === "Markdown") return getMarkdownEditorFilePath(c.id);
  return null;
}

function terminalSelection(): { text: string; label: string } | null {
  const inst = activeTerminalInstance();
  const text = inst?.terminal.getSelection() ?? "";
  if (!inst || text.trim().length === 0) return null;
  return { text, label: activeContent()?.label ?? "Terminal" };
}

function editorSelection(): { text: string; path: string; fromLine: number; toLine: number } | null {
  const c = activeContent();
  if (!c || c.provider !== "CodeEditor") return null;
  const path = getCodeEditorFilePath(c.id);
  const sel = getCodeEditorSelection(c.id);
  return path && sel ? { path, ...sel } : null;
}

/** Put a block into the composer (panel opened if needed) and focus it. */
function injectIntoComposer(block: string) {
  if (!block) return;
  ensureChatVisible();
  inputEl.value = appendToDraft(inputEl.value, block);
  autoResize();
  inputEl.focus();
  inputEl.setSelectionRange(inputEl.value.length, inputEl.value.length);
}

async function buildContextBlock(kind: ContextKind): Promise<string> {
  switch (kind) {
    case "terminal": {
      const sel = terminalSelection();
      return sel ? fenceBlock("terminal", { name: sel.label }, sel.text) : "";
    }
    case "editor-selection": {
      const sel = editorSelection();
      return sel
        ? fenceBlock("editor-selection", { name: sel.path, lines: { from: sel.fromLine, to: sel.toLine } }, sel.text)
        : "";
    }
    case "file": {
      const path = activeFilePath();
      if (!path) return "";
      // A selection beats the whole file (which is capped at 200 lines anyway).
      const sel = editorSelection();
      if (sel) return fenceBlock("file", { name: path, lines: { from: sel.fromLine, to: sel.toLine } }, sel.text);
      const content = await ipc.readFileContent(appState.activeWorkspace, path);
      return fenceBlock("file", { name: path }, content);
    }
    case "diff": {
      const path = activeFilePath();
      if (!path) return "";
      const lines = await ipc.getFileDiff(appState.activeWorkspace, path, false);
      const text = diffLinesToText(lines);
      if (text.trim().length === 0) {
        toast("No unstaged changes in the active file", "info");
        return "";
      }
      return fenceBlock("diff", { name: path }, text);
    }
  }
}

async function injectContext(kind: ContextKind) {
  try {
    const block = await buildContextBlock(kind);
    if (!block) {
      toast("Nothing to add", "info");
      return;
    }
    injectIntoComposer(block);
  } catch (err) {
    reportError("Add context failed", err);
  }
}

function openContextChooser(x: number, y: number) {
  const rows = contextChoices({
    terminalSelection: terminalSelection() !== null,
    activeFile: activeFilePath() !== null,
    editorSelection: editorSelection() !== null,
  });
  openContextMenu(
    x,
    y,
    rows.map((r) => ({ label: r.label, disabled: r.disabled, action: () => void injectContext(r.kind) })),
  );
}

/**
 * "Add Context to Chat" (`Ctrl+Shift+I`, palette, Chat menu, the `+` in the
 * composer): a terminal selection is injected straight away — select, chord,
 * done — otherwise a chooser offers the active file, its diff and the editor
 * selection.
 */
export async function addContextToChat() {
  if (terminalSelection()) {
    await injectContext("terminal");
    return;
  }
  const r = inputEl.getBoundingClientRect();
  // The composer may be off-screen while the panel is hidden: open it first
  // so the chooser anchors to something the user can see.
  ensureChatVisible();
  requestAnimationFrame(() => {
    const rr = inputEl.getBoundingClientRect();
    openContextChooser(rr.left || r.left, (rr.top || r.top) - 4);
  });
}

// ── Tool cards ─────────────────────────────────────

/** Live cards by tool call id (the DOM node re-rendered in place). */
const liveCards = new Map<string, { card: ToolCard; el: HTMLElement; startedAt: number }>();
/** Cards created before a result arrives, matched to `tool-executing` by name order. */
let executingQueue: string[] = [];

function toolCardFromHistory(content: string): ToolCard {
  const p = parseToolMessage(content);
  return { id: "", name: p.name, args: "", result: p.result, status: p.isError ? "error" : "ok" };
}

function hasPendingApproval(): boolean {
  for (const { card } of liveCards.values()) if (card.status === "approval") return true;
  return false;
}

/** Mark every card still waiting on the user as `status` (Stop / Clear / new
 *  message): the Rust side drops the senders, which the loop reads as Deny. */
function settlePendingApprovals(status: "denied") {
  for (const entry of liveCards.values()) {
    if (entry.card.status === "approval") {
      entry.card.status = status;
      refreshToolCard(entry);
    }
  }
}

function onAgentEvent(ev: ipc.ChatAgentEvent) {
  armWatchdog();
  switch (ev.kind) {
    case "tool-calls":
      for (const c of ev.calls) {
        const card: ToolCard = { id: c.id, name: c.name, args: prettyJson(c.arguments), result: "", status: "running" };
        const el = renderToolCard(card);
        if (streamingEl) messagesEl.insertBefore(el, streamingEl);
        else messagesEl.appendChild(el);
        liveCards.set(c.id, { card, el, startedAt: performance.now() });
        executingQueue.push(c.id);
      }
      scrollToBottom();
      return;
    case "tool-executing": {
      // Restart the clock when execution really begins (after an approval).
      const idx = executingQueue.findIndex((id) => liveCards.get(id)?.card.name === ev.name);
      if (idx >= 0) {
        const entry = liveCards.get(executingQueue[idx]);
        executingQueue.splice(idx, 1);
        if (entry) entry.startedAt = performance.now();
      }
      return;
    }
    case "approval-required": {
      const entry = liveCards.get(ev.tool_call_id);
      if (!entry) return;
      entry.card.status = "approval";
      if (!entry.card.args) entry.card.args = ev.description;
      refreshToolCard(entry);
      scrollToBottom();
      return;
    }
    case "tool-result": {
      const entry = liveCards.get(ev.tool_call_id);
      const status = ev.is_error ? "error" : "ok";
      if (entry) {
        entry.card.result = ev.result;
        entry.card.status = ev.is_error && entry.card.status === "denied" ? "denied" : status;
        entry.card.durationMs = performance.now() - entry.startedAt;
        refreshToolCard(entry);
        liveCards.delete(ev.tool_call_id);
        messages.push({ role: "tool", content: ev.result, tool: entry.card });
      } else {
        const card: ToolCard = { id: ev.tool_call_id, name: ev.name, args: "", result: ev.result, status };
        const el = renderToolCard(card);
        if (streamingEl) messagesEl.insertBefore(el, streamingEl);
        else messagesEl.appendChild(el);
        messages.push({ role: "tool", content: ev.result, tool: card });
      }
      scrollToBottom();
      return;
    }
  }
}

function refreshToolCard(entry: { card: ToolCard; el: HTMLElement }) {
  const next = renderToolCard(entry.card, entry.el.hasAttribute("open"));
  entry.el.replaceWith(next);
  entry.el = next;
}

function statusView(status: ToolCard["status"]): { icon: IconName; label: string } {
  switch (status) {
    case "running":
      return { icon: "clock", label: "running" };
    case "ok":
      return { icon: "check", label: "done" };
    case "error":
      return { icon: "warning", label: "error" };
    case "approval":
      return { icon: "warning", label: "needs approval" };
    case "approved":
      return { icon: "play", label: "approved" };
    case "denied":
      return { icon: "close", label: "denied" };
  }
}

/** Collapsible card for one tool call: name + status + duration in the
 *  summary, args and result as `<pre>` in the body, long results folded
 *  behind "Show more", Approve / Deny inline while the loop waits. */
function renderToolCard(card: ToolCard, open?: boolean): HTMLElement {
  const details = document.createElement("details");
  details.className = "chat-msg tool chat-tool-card";
  details.dataset.status = card.status;
  if (open ?? card.status === "approval") details.open = true;
  const v = statusView(card.status);
  const duration = card.durationMs !== undefined ? `<span class="chat-tool-duration">${formatDurationMs(card.durationMs)}</span>` : "";
  details.innerHTML = `
    <summary class="chat-tool-summary">
      <span class="chat-tool-status" title="${v.label}">${icon(v.icon, { label: v.label })}</span>
      <span class="chat-tool-name">${escapeHtml(card.name)}</span>
      <span class="chat-tool-state">${v.label}</span>
      ${duration}
    </summary>
    <div class="chat-tool-body"></div>
  `;
  const body = details.querySelector<HTMLElement>(".chat-tool-body")!;
  if (card.args) body.appendChild(toolSection("Arguments", card.args));
  if (card.status === "approval") {
    const row = document.createElement("div");
    row.className = "chat-tool-approval";
    const approve = document.createElement("button");
    approve.className = "ui-btn";
    approve.dataset.variant = "primary";
    approve.dataset.size = "sm";
    approve.textContent = "Approve";
    const deny = document.createElement("button");
    deny.className = "ui-btn";
    deny.dataset.variant = "danger";
    deny.dataset.size = "sm";
    deny.textContent = "Deny";
    approve.addEventListener("click", () => void answerApproval(card.id, "allow"));
    deny.addEventListener("click", () => void answerApproval(card.id, "deny"));
    row.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        void answerApproval(card.id, "deny");
      }
    });
    row.append(approve, deny);
    body.appendChild(row);
    requestAnimationFrame(() => approve.focus());
  }
  if (card.result) body.appendChild(toolSection("Result", card.result));
  return details;
}

function toolSection(title: string, text: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "chat-tool-section";
  const lines = text.split("\n");
  const long = lines.length > RESULT_COLLAPSE_LINES;
  const pre = document.createElement("pre");
  pre.className = "chat-tool-pre";
  pre.textContent = long ? lines.slice(0, RESULT_COLLAPSE_LINES).join("\n") : text;
  const label = document.createElement("span");
  label.className = "chat-tool-section-title";
  label.textContent = title;
  wrap.append(label, pre);
  if (long) {
    const more = document.createElement("button");
    more.className = "ui-btn";
    more.dataset.variant = "ghost";
    more.dataset.size = "sm";
    more.textContent = `Show more (${lines.length - RESULT_COLLAPSE_LINES} lines)`;
    more.addEventListener("click", () => {
      const expanded = more.dataset.expanded === "1";
      pre.textContent = expanded ? lines.slice(0, RESULT_COLLAPSE_LINES).join("\n") : text;
      more.dataset.expanded = expanded ? "0" : "1";
      more.textContent = expanded ? `Show more (${lines.length - RESULT_COLLAPSE_LINES} lines)` : "Show less";
    });
    wrap.appendChild(more);
  }
  return wrap;
}

async function answerApproval(toolCallId: string, decision: ipc.ChatApprovalDecision) {
  const entry = liveCards.get(toolCallId);
  if (!entry || entry.card.status !== "approval") return;
  entry.card.status = decision === "deny" ? "denied" : "approved";
  if (decision !== "deny") entry.startedAt = performance.now();
  refreshToolCard(entry);
  armWatchdog();
  try {
    await ipc.chatApprove(toolCallId, decision);
  } catch (err) {
    reportError("Approval failed", err);
  }
  inputEl.focus();
}

// ── Chat resize handle ─────────────────────────────

export function initChatResize() {
  const handle = document.getElementById("chat-resize-v");
  if (!handle) return;

  let dragging = false;
  let startX = 0;
  let startWidth = 0;
  const root = document.documentElement;

  const savedWidth = settingsStore.get<number>("chatPanelWidth");
  if (savedWidth) root.style.setProperty("--chat-panel-width", `${savedWidth}px`);

  handle.addEventListener("mousedown", (e: MouseEvent) => {
    dragging = true;
    startX = e.clientX;
    startWidth = parseInt(getComputedStyle(root).getPropertyValue("--chat-panel-width")) || 360;
    handle.classList.add("dragging");
    e.preventDefault();
  });

  document.addEventListener("mousemove", (e: MouseEvent) => {
    if (!dragging) return;
    // Chat is on the right, so dragging left increases width
    const delta = startX - e.clientX;
    const newWidth = clampChatWidth(startWidth + delta, window.innerWidth, visibleSidebarWidth(), activityBarWidth());
    root.style.setProperty("--chat-panel-width", `${newWidth}px`);
  });

  document.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false;
    handle.classList.remove("dragging");
    // Persist width
    const width = parseInt(getComputedStyle(root).getPropertyValue("--chat-panel-width"));
    if (width) settingsStore.patch("chatPanelWidth", width);
  });
}
