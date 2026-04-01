import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";

export async function showStashDialog() {
  document.querySelector(".dialog-backdrop")?.remove();

  const wsIdx = appState.activeWorkspace;
  let entries: ipc.StashEntry[];
  try {
    entries = await ipc.gitStashList(wsIdx);
  } catch (err) {
    toast(`Failed to load stashes: ${err}`, "error");
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";

  function render() {
    const existing = backdrop.querySelector(".dialog");
    if (existing) existing.remove();

    const dialog = document.createElement("div");
    dialog.className = "dialog";
    dialog.style.maxWidth = "520px";
    dialog.innerHTML = `
      <div class="dialog-header">
        <span class="dialog-title">Git Stash</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <div class="dialog-field">
          <label class="dialog-label">Save new stash</label>
          <div style="display:flex;gap:6px">
            <input class="dialog-input" id="stash-msg" placeholder="Stash message" style="flex:1" />
            <button class="dialog-btn dialog-btn-primary" id="stash-save">Save</button>
          </div>
        </div>
        <div style="margin-top:8px">
          <label class="dialog-label">Stashes (${entries.length})</label>
          <div class="stash-list" style="margin-top:4px;max-height:300px;overflow-y:auto">
            ${entries.length === 0 ? '<div class="empty-message">No stashes</div>' : ""}
            ${entries
              .map(
                (e) => `
              <div class="stash-entry" data-idx="${e.index}">
                <span class="stash-id">${escapeHtml(e.id)}</span>
                <span class="stash-msg">${escapeHtml(e.message)}</span>
                <span class="stash-actions">
                  <button class="dialog-btn dialog-btn-secondary stash-btn" data-action="pop" title="Pop">Pop</button>
                  <button class="dialog-btn dialog-btn-secondary stash-btn" data-action="apply" title="Apply">Apply</button>
                  <button class="dialog-btn dialog-btn-danger stash-btn" data-action="drop" title="Drop" style="padding:3px 6px;font-size:11px">×</button>
                </span>
              </div>
            `,
              )
              .join("")}
          </div>
        </div>
      </div>
    `;

    dialog.querySelector(".dialog-close")!.addEventListener("click", close);

    // Save stash
    const saveBtn = dialog.querySelector<HTMLButtonElement>("#stash-save")!;
    const msgInput = dialog.querySelector<HTMLInputElement>("#stash-msg")!;
    saveBtn.addEventListener("click", async () => {
      const msg = msgInput.value.trim() || "WIP";
      saveBtn.disabled = true;
      try {
        await ipc.gitStashSave(wsIdx, msg);
        toast(`Stashed: ${msg}`, "success");
        entries = await ipc.gitStashList(wsIdx);
        render();
        refreshFiles();
      } catch (err) {
        toast(`Stash save failed: ${err}`, "error");
        saveBtn.disabled = false;
      }
    });

    msgInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") saveBtn.click();
    });

    // Stash action buttons
    dialog.querySelectorAll<HTMLButtonElement>(".stash-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const entry = btn.closest<HTMLElement>(".stash-entry")!;
        const idx = parseInt(entry.dataset.idx!, 10);
        const action = btn.dataset.action!;
        btn.disabled = true;

        try {
          if (action === "pop") {
            await ipc.gitStashPop(wsIdx, idx);
            toast("Stash popped", "success");
          } else if (action === "apply") {
            await ipc.gitStashApply(wsIdx, idx);
            toast("Stash applied", "success");
          } else if (action === "drop") {
            await ipc.gitStashDrop(wsIdx, idx);
            toast("Stash dropped", "info");
          }
          entries = await ipc.gitStashList(wsIdx);
          render();
          refreshFiles();
        } catch (err) {
          toast(`Stash ${action} failed: ${err}`, "error");
          btn.disabled = false;
        }
      });
    });

    backdrop.appendChild(dialog);
    msgInput.focus();
  }

  const close = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });

  document.body.appendChild(backdrop);
  render();
}

async function refreshFiles() {
  const wsIdx = appState.activeWorkspace;
  try {
    const files = await ipc.getChangedFiles(wsIdx);
    appState.updateFiles(wsIdx, files, appState.activeWs?.aheadBehind ?? null);
  } catch {
    /* ignore */
  }
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}
