import * as ipc from "../ipc";
import { toast } from "./toast";
import { createDropdown, type DropdownHandle } from "./dropdown";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface ChatMsg {
  role: "user" | "assistant";
  content: string;
}

let container: HTMLElement;
let messagesEl: HTMLDivElement;
let inputEl: HTMLTextAreaElement;
let sendBtn: HTMLButtonElement;
let modelDropdown: DropdownHandle | null = null;
let modelBarEl: HTMLDivElement;
let streamingEl: HTMLDivElement | null = null;
let unlistenToken: UnlistenFn | null = null;
let messages: ChatMsg[] = [];
let streaming = false;
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
    <button class="chat-header-btn chat-settings-btn" title="Chat settings">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
        <path d="M8 10a2 2 0 100-4 2 2 0 000 4z" stroke="currentColor" stroke-width="1.2"/>
        <path d="M13.5 8c0-.3-.2-.6-.4-.8l1-1.6-.8-1.4-1.8.4c-.4-.3-.8-.6-1.3-.7L9.8 2H8.2l-.4 1.9c-.5.1-.9.4-1.3.7l-1.8-.4-.8 1.4 1 1.6c-.2.2-.4.5-.4.8s.2.6.4.8l-1 1.6.8 1.4 1.8-.4c.4.3.8.6 1.3.7l.4 1.9h1.6l.4-1.9c.5-.1.9-.4 1.3-.7l1.8.4.8-1.4-1-1.6c.2-.2.4-.5.4-.8z" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </button>
    <button class="chat-header-btn chat-clear-btn" title="Clear conversation">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
        <path d="M2 4h12M5 4V3a1 1 0 011-1h4a1 1 0 011 1v1m2 0v9a1 1 0 01-1 1H4a1 1 0 01-1-1V4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
      </svg>
    </button>
  `;
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
  refreshBtn.className = "chat-model-refresh";
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

  inputEl = document.createElement("textarea");
  inputEl.className = "chat-input";
  inputEl.placeholder = "Ask a question\u2026";
  inputEl.rows = 1;
  inputEl.addEventListener("keydown", onInputKeydown);
  inputEl.addEventListener("input", autoResize);
  inputArea.appendChild(inputEl);

  sendBtn = document.createElement("button");
  sendBtn.className = "chat-send-btn";
  sendBtn.title = "Send (Enter)";
  sendBtn.innerHTML = `<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
    <path d="M2 8l10-5-3 5 3 5z" fill="currentColor"/>
  </svg>`;
  sendBtn.addEventListener("click", sendMessage);
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
      if (msg.role === "User" || msg.role === "Assistant") {
        messages.push({
          role: msg.role === "User" ? "user" : "assistant",
          content: msg.content,
        });
      }
    }
    if (messages.length > 0) {
      renderMessages();
    }
  } catch {
    // No messages
  }

  await loadModels();

  // Subscribe to streaming tokens
  unlistenToken = await ipc.onChatToken(onToken);
}

async function loadModels() {
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
  messages.push({ role: "user", content: text });
  inputEl.value = "";
  autoResize();
  renderMessages();

  // Start streaming
  streaming = true;
  sendBtn.disabled = true;

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
    await ipc.chatSendMessage(text);
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
    return;
  }

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

function onStreamEnd() {
  streaming = false;
  sendBtn.disabled = false;
  streamingEl = null;
  inputEl.focus();
}

async function clearChat() {
  messages = [];
  renderEmpty();
  try {
    await ipc.chatClear();
  } catch {
    // ignore
  }
}

function renderMessages() {
  messagesEl.innerHTML = "";
  if (messages.length === 0) {
    renderEmpty();
    return;
  }
  for (const msg of messages) {
    const el = document.createElement("div");
    el.className = `chat-msg ${msg.role}`;
    el.innerHTML = `
      <span class="chat-msg-role">${msg.role}</span>
      <div class="chat-msg-content">${escapeHtml(msg.content)}</div>
    `;
    messagesEl.appendChild(el);
  }
  scrollToBottom();
}

function renderEmpty() {
  messagesEl.innerHTML = `
    <div class="chat-empty">
      <div class="chat-empty-icon">\u{1F4AC}</div>
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
  dialog.className = "dialog";
  dialog.style.maxWidth = "480px";

  // Header
  const header = document.createElement("div");
  header.className = "dialog-header";
  header.innerHTML = `
    <span class="dialog-title">Chat Settings</span>
    <button class="dialog-close">&times;</button>
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
  urlInput.className = "dialog-input";
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
  promptInput.className = "dialog-input chat-settings-textarea";
  promptInput.value = currentConfig.system_prompt ?? "";
  promptInput.placeholder = "Optional instructions prepended to every conversation";
  promptInput.rows = 4;
  promptRow.appendChild(promptInput);
  body.appendChild(promptRow);

  // Footer buttons
  const footer = document.createElement("div");
  footer.className = "dialog-footer";
  footer.innerHTML = `
    <button class="dialog-btn dialog-btn-secondary chat-settings-cancel">Cancel</button>
    <button class="dialog-btn dialog-btn-primary chat-settings-save">Save</button>
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

// ── Toggle ─────────────────────────────────────────

export function toggleChatPanel() {
  const app = document.getElementById("app")!;
  app.classList.toggle("chat-visible");
  if (app.classList.contains("chat-visible")) {
    inputEl?.focus();
  }
}

// ── Chat resize handle ─────────────────────────────

export function initChatResize() {
  const handle = document.getElementById("chat-resize-v");
  if (!handle) return;

  let dragging = false;
  let startX = 0;
  let startWidth = 0;
  const root = document.documentElement;

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
    const newWidth = Math.max(240, Math.min(800, startWidth + delta));
    root.style.setProperty("--chat-panel-width", `${newWidth}px`);
  });

  document.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false;
    handle.classList.remove("dragging");
    // Persist width
    const width = parseInt(getComputedStyle(root).getPropertyValue("--chat-panel-width"));
    if (width) {
      ipc.getSettings().then((raw: string | null) => {
        const settings = raw ? JSON.parse(raw) : {};
        settings.chatPanelWidth = width;
        ipc.setSettings(JSON.stringify(settings)).catch(() => {});
      }).catch(() => {});
    }
  });
}
