import * as ipc from "../../ipc";
import { toast } from "../toast";
import type { AgentRow } from "../../types";

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function encodeBase64Utf8(data: string): string {
  const bytes = new TextEncoder().encode(data);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary);
}

/** Broadcast one input line to several agent tabs at once (e.g. `/stop`).
 *  Desktop counterpart of the TUI's Broadcast dialog: checklist over
 *  `list_agent_rows`, a text line, and an append-Enter toggle. Sends run
 *  through the existing `write_pty` command per tab — no backend change. */
export async function showBroadcastDialog() {
  document.querySelector(".broadcast-backdrop")?.remove();

  let rows: AgentRow[];
  try {
    rows = await ipc.listAgentRows();
  } catch (err) {
    toast(`Couldn't list agents: ${err}`, "error");
    return;
  }
  if (rows.length === 0) {
    toast("No running agents to broadcast to", "info");
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop broadcast-backdrop";
  backdrop.innerHTML = `
    <div class="dialog ui-surface" style="max-width:520px">
      <div class="ui-header">
        <span class="ui-header-title">Broadcast to Agents</span>
        <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Agents (<a href="#" id="bc-all">all</a> / <a href="#" id="bc-none">none</a>)</label>
          <div id="bc-rows" style="display:flex;flex-direction:column;gap:4px;max-height:220px;overflow-y:auto">
            ${rows
              .map(
                (r, i) => `
              <label style="display:flex;align-items:center;gap:8px;cursor:pointer">
                <input type="checkbox" data-i="${i}" checked />
                <span>${esc(r.workspace_name)} › ${esc(r.label)}</span>
              </label>`,
              )
              .join("")}
          </div>
        </div>
        <div class="dialog-field">
          <label class="dialog-label">Line to send</label>
          <textarea class="ui-input" id="bc-text" rows="3" placeholder="/stop"></textarea>
        </div>
        <div class="dialog-field">
          <label style="display:flex;align-items:center;gap:8px;cursor:pointer">
            <input type="checkbox" id="bc-enter" checked />
            <span class="dialog-label" style="margin:0">Append Enter (execute it)</span>
          </label>
        </div>
      </div>
      <div class="dialog-footer">
        <button data-variant="secondary" class="ui-btn" id="bc-cancel">Cancel</button>
        <button data-variant="primary" class="ui-btn" id="bc-send">Send</button>
      </div>
    </div>
  `;

  document.body.appendChild(backdrop);
  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#bc-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("mousedown", (e) => {
    if (e.target === backdrop) close();
  });
  const boxes = () =>
    [...backdrop.querySelectorAll<HTMLInputElement>("#bc-rows input[type=checkbox]")];
  backdrop.querySelector("#bc-all")!.addEventListener("click", (e) => {
    e.preventDefault();
    for (const b of boxes()) b.checked = true;
  });
  backdrop.querySelector("#bc-none")!.addEventListener("click", (e) => {
    e.preventDefault();
    for (const b of boxes()) b.checked = false;
  });

  const textArea = backdrop.querySelector<HTMLTextAreaElement>("#bc-text")!;
  textArea.focus();
  textArea.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });

  backdrop.querySelector("#bc-send")!.addEventListener("click", async () => {
    const text = textArea.value;
    if (!text.trim()) {
      toast("Type a line to send first", "error");
      return;
    }
    const appendEnter =
      backdrop.querySelector<HTMLInputElement>("#bc-enter")!.checked;
    const payload = encodeBase64Utf8(appendEnter ? text + "\n" : text);
    const targets = boxes()
      .filter((b) => b.checked)
      .map((b) => rows[Number(b.dataset.i)]);
    if (targets.length === 0) {
      toast("Check at least one agent", "error");
      return;
    }
    let failed = 0;
    for (const row of targets) {
      try {
        await ipc.writePty(row.tab_id, payload);
      } catch {
        failed++;
      }
    }
    close();
    if (failed > 0) {
      toast(`Broadcast reached ${targets.length - failed}/${targets.length} agent(s)`, "error");
    } else {
      toast(`Broadcast to ${targets.length} agent(s)`, "success");
    }
  });
}
