import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";
import type { AgentInfo } from "../../ipc";

export async function showDispatchDialog() {
  document.querySelector(".dialog-backdrop")?.remove();

  const wsIdx = appState.activeWorkspace;
  let agents: AgentInfo[];
  try {
    agents = await ipc.listAgents(wsIdx);
  } catch {
    agents = [];
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.innerHTML = `
    <div class="dialog" style="max-width:520px">
      <div class="dialog-header">
        <span class="dialog-title">Dispatch Agent</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Agent</label>
          <select class="dialog-select" id="dp-agent">
            <option value="">(None — raw provider)</option>
            ${agents.map((a) => `<option value="${a.id}" data-provider="${esc(a.provider)}" data-role="${esc(a.role)}">${esc(a.name)} (${esc(a.provider)})</option>`).join("")}
          </select>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Provider</label>
          <select class="dialog-select" id="dp-provider">
            <option value="Claude Code">Claude Code</option>
            <option value="Gemini">Gemini</option>
            <option value="OpenCode">OpenCode</option>
            <option value="Kilo">Kilo</option>
            <option value="Codex">Codex</option>
          </select>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Additional prompt</label>
          <textarea class="dialog-textarea" id="dp-prompt" rows="4" placeholder="Optional: additional instructions"></textarea>
        </div>
        <div class="dialog-field">
          <label style="display:flex;align-items:center;gap:8px;cursor:pointer">
            <input type="checkbox" id="dp-worktree" />
            <span class="dialog-label" style="margin:0">Create new worktree workspace</span>
          </label>
        </div>
        <div class="dialog-field" id="dp-name-field" style="display:none">
          <label class="dialog-label">Workspace name</label>
          <input class="dialog-input" id="dp-ws-name" placeholder="feature/agent-task" />
        </div>
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-secondary" id="dp-cancel">Cancel</button>
        <button class="dialog-btn dialog-btn-primary" id="dp-dispatch">Dispatch</button>
      </div>
    </div>
  `;

  document.body.appendChild(backdrop);

  const agentSelect = backdrop.querySelector<HTMLSelectElement>("#dp-agent")!;
  const providerSelect = backdrop.querySelector<HTMLSelectElement>("#dp-provider")!;
  const worktreeCheck = backdrop.querySelector<HTMLInputElement>("#dp-worktree")!;
  const nameField = backdrop.querySelector<HTMLElement>("#dp-name-field")!;

  // When agent is selected, auto-set provider
  agentSelect.addEventListener("change", () => {
    const opt = agentSelect.selectedOptions[0];
    if (opt?.dataset.provider) {
      // Map provider label to select value
      providerSelect.value = opt.dataset.provider;
    }
  });

  worktreeCheck.addEventListener("change", () => {
    nameField.style.display = worktreeCheck.checked ? "" : "none";
  });

  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#dp-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  agentSelect.focus();

  backdrop.querySelector("#dp-dispatch")!.addEventListener("click", async () => {
    const agentOpt = agentSelect.selectedOptions[0];
    const agentRole = agentOpt?.dataset.role || "";
    const provider = providerSelect.value;
    const additionalPrompt = (backdrop.querySelector("#dp-prompt") as HTMLTextAreaElement).value.trim();
    const createWorktree = worktreeCheck.checked;
    const wsName = (backdrop.querySelector("#dp-ws-name") as HTMLInputElement).value.trim() || undefined;

    // Compose prompt
    let prompt = "";
    if (agentRole) {
      prompt = agentRole;
      if (additionalPrompt) prompt += "\n\n" + additionalPrompt;
    } else {
      prompt = additionalPrompt;
    }

    const btn = backdrop.querySelector<HTMLButtonElement>("#dp-dispatch")!;
    btn.disabled = true;
    btn.textContent = "Dispatching...";

    try {
      const tabId = await ipc.dispatchAgent(wsIdx, provider, prompt, createWorktree, wsName);

      if (createWorktree) {
        // Reload workspaces to pick up the new one
        const workspaces = await ipc.listWorkspaces();
        appState.setWorkspaces(workspaces);
        // Switch to the new workspace (last one)
        const newIdx = workspaces.length - 1;
        const detail = await ipc.switchWorkspace(newIdx);
        appState.setActiveWorkspace(newIdx, detail);
      } else {
        // Tab was added to current workspace — update state
        const ws = appState.activeWs;
        if (ws) {
          const agentName = agentOpt?.textContent?.split(" (")[0] || provider;
          appState.addTab(wsIdx, {
            id: tabId,
            provider: mapProviderToAI(provider),
            alive: true,
          });
        }
      }

      toast("Agent dispatched", "success");
      close();
    } catch (err) {
      toast(`Dispatch failed: ${err}`, "error");
      btn.disabled = false;
      btn.textContent = "Dispatch";
    }
  });
}

function mapProviderToAI(label: string): import("../../types").AIProvider {
  const map: Record<string, import("../../types").AIProvider> = {
    "Claude Code": "Claude",
    "Gemini": "Gemini",
    "OpenCode": "OpenCode",
    "Kilo": "Kilo",
    "Codex": "Codex",
  };
  return map[label] || "Claude";
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
