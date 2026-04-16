import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";
import { createDropdown, type DropdownOption } from "../dropdown";
import type { AgentInfo } from "../../ipc";

export interface CardContext {
  id: string;
  title: string;
  description: string;
  priority: string;
  project: string;
}

export async function showDispatchDialog(cardContext?: CardContext) {
  document.querySelector(".dialog-backdrop")?.remove();

  const wsIdx = appState.activeWorkspace;
  let agents: AgentInfo[];
  try {
    agents = await ipc.listAgents(wsIdx);
  } catch {
    agents = [];
  }

  // Build dropdown options
  const agentOptions: DropdownOption[] = [
    { value: "", label: "(None — raw provider)", data: { provider: "", role: "" } },
    ...agents.map((a) => ({
      value: String(a.id ?? ""),
      label: `${a.name} (${a.provider})`,
      data: { provider: a.provider, role: a.role },
    })),
  ];

  const providerOptions: DropdownOption[] = [
    { value: "Claude Code", label: "Claude Code" },
    { value: "Gemini", label: "Gemini" },
    { value: "OpenCode", label: "OpenCode" },
    { value: "Kilo", label: "Kilo" },
    { value: "Codex", label: "Codex" },
  ];

  const agentDropdown = createDropdown(agentOptions, "");
  const providerDropdown = createDropdown(providerOptions, "Claude Code");

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.innerHTML = `
    <div class="dialog" style="max-width:520px">
      <div class="dialog-header">
        <span class="dialog-title">Dispatch Agent${cardContext ? ` — ${esc(cardContext.title)}` : ""}</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Agent</label>
          <span id="dp-agent-slot"></span>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Provider</label>
          <span id="dp-provider-slot"></span>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Additional prompt</label>
          <textarea class="dialog-textarea" id="dp-prompt" rows="4" placeholder="Optional: additional instructions">${cardContext ? esc(`Task: ${cardContext.title}\n\n${cardContext.description}`) : ""}</textarea>
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

  // Mount custom dropdowns into their slots
  backdrop.querySelector("#dp-agent-slot")!.replaceWith(agentDropdown.container);
  backdrop.querySelector("#dp-provider-slot")!.replaceWith(providerDropdown.container);

  const worktreeCheck = backdrop.querySelector<HTMLInputElement>("#dp-worktree")!;
  const nameField = backdrop.querySelector<HTMLElement>("#dp-name-field")!;

  // When agent is selected, auto-set provider
  agentDropdown.container.addEventListener("change", () => {
    const data = agentDropdown.getData();
    if (data.provider) {
      providerDropdown.value = data.provider;
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
  agentDropdown.container.querySelector("button")!.focus();

  backdrop.querySelector("#dp-dispatch")!.addEventListener("click", async () => {
    const agentData = agentDropdown.getData();
    const agentRole = agentData.role || "";
    const provider = providerDropdown.value;
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
      // Derive display name from selected agent
      const agentLabel = agentOptions.find((o) => o.value === agentDropdown.value)?.label ?? provider;
      const agentName = agentLabel.split(" (")[0];

      const currentGroup = appState.activeWs?.info.group || undefined;
      const sourceKanban = appState.activeWs?.info.kanban_path || undefined;
      const tabId = await ipc.dispatchAgent(
        wsIdx, provider, prompt, createWorktree, wsName, currentGroup,
        cardContext?.id, sourceKanban, agentName, cardContext?.title,
      );

      if (createWorktree) {
        const workspaces = await ipc.listWorkspaces();
        appState.setWorkspaces(workspaces);
        const newIdx = workspaces.length - 1;
        const detail = await ipc.switchWorkspace(newIdx);
        appState.setActiveWorkspace(newIdx, detail);
      } else {
        const ws = appState.activeWs;
        if (ws) {
          appState.addTab(wsIdx, {
            id: tabId,
            provider: mapProviderToAI(provider),
            alive: true,
          });
        }
      }

      // Update kanban card if dispatched from card
      if (cardContext) {
        try {
          await ipc.kanbanUpdateCard(wsIdx, cardContext.id, cardContext.title, cardContext.description, cardContext.priority, agentName, cardContext.project);
          await ipc.kanbanMoveCard(wsIdx, cardContext.id, "in_progress");
        } catch {
          // Non-critical: card update failure shouldn't block dispatch
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
