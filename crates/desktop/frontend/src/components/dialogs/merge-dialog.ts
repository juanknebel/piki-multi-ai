import { appState } from "../../state";
import * as ipc from "../../ipc";
import { toast } from "../toast";

export function showMergeDialog() {
  document.querySelector(".dialog-backdrop")?.remove();

  const ws = appState.activeWs;
  if (!ws) return;

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.innerHTML = `
    <div class="dialog" style="max-width:480px">
      <div class="dialog-header">
        <span class="dialog-title">Merge / Rebase</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <p style="font-size:13px;color:var(--text-primary);margin-bottom:8px">
          Merge <strong>${escapeHtml(ws.info.branch)}</strong> into main branch.
        </p>
        <div class="dialog-field">
          <label class="dialog-label">Strategy</label>
          <select class="dialog-select" id="merge-strategy">
            <option value="merge" selected>Merge (creates merge commit)</option>
            <option value="rebase">Rebase (linear history)</option>
          </select>
        </div>
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-secondary" id="merge-cancel">Cancel</button>
        <button class="dialog-btn dialog-btn-primary" id="merge-submit">Merge</button>
      </div>
    </div>
  `;

  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();
  backdrop.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.querySelector("#merge-cancel")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });

  const strategySelect = backdrop.querySelector<HTMLSelectElement>("#merge-strategy")!;
  const submitBtn = backdrop.querySelector<HTMLButtonElement>("#merge-submit")!;

  strategySelect.addEventListener("change", () => {
    submitBtn.textContent = strategySelect.value === "rebase" ? "Rebase" : "Merge";
  });

  submitBtn.addEventListener("click", async () => {
    const strategy = strategySelect.value as "merge" | "rebase";
    submitBtn.disabled = true;
    submitBtn.textContent = strategy === "rebase" ? "Rebasing..." : "Merging...";

    try {
      const result = await ipc.gitMerge(appState.activeWorkspace, strategy);

      if (result.success) {
        toast(result.message, "success");
        close();
        // Refresh files
        const files = await ipc.getChangedFiles(appState.activeWorkspace);
        appState.updateFiles(appState.activeWorkspace, files, ws.aheadBehind);
      } else if (result.conflicts.length > 0) {
        close();
        showConflictResolution(result.conflicts);
      } else {
        toast(result.message, "error");
        submitBtn.disabled = false;
        submitBtn.textContent = strategy === "rebase" ? "Rebase" : "Merge";
      }
    } catch (err) {
      toast(`Merge failed: ${err}`, "error");
      submitBtn.disabled = false;
      submitBtn.textContent = strategy === "rebase" ? "Rebase" : "Merge";
    }
  });
}

function showConflictResolution(conflicts: string[]) {
  document.querySelector(".dialog-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";

  function render() {
    const dialog = backdrop.querySelector(".dialog");
    if (dialog) dialog.remove();

    const el = document.createElement("div");
    el.className = "dialog";
    el.style.maxWidth = "560px";
    el.innerHTML = `
      <div class="dialog-header">
        <span class="dialog-title" style="color:var(--git-conflicted)">Conflict Resolution</span>
        <button class="dialog-close">×</button>
      </div>
      <div class="dialog-body">
        <p style="font-size:13px;color:var(--text-primary);margin-bottom:12px">
          ${conflicts.length} file${conflicts.length > 1 ? "s" : ""} with conflicts. Resolve each file:
        </p>
        <div class="conflict-file-list">
          ${conflicts
            .map(
              (f) => `
            <div class="conflict-file-item" data-path="${escapeAttr(f)}">
              <span class="file-status conflicted" style="color:var(--git-conflicted)">C</span>
              <span class="conflict-file-path">${escapeHtml(f)}</span>
              <span class="conflict-actions">
                <button class="dialog-btn dialog-btn-secondary conflict-btn" data-resolution="ours" title="Keep our version">Ours</button>
                <button class="dialog-btn dialog-btn-secondary conflict-btn" data-resolution="theirs" title="Keep their version">Theirs</button>
              </span>
            </div>
          `,
            )
            .join("")}
        </div>
      </div>
      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn-danger" id="conflict-abort">Abort</button>
        <span style="flex:1"></span>
        <button class="dialog-btn dialog-btn-primary" id="conflict-continue" ${conflicts.length > 0 ? "disabled" : ""}>
          Continue (${conflicts.length} remaining)
        </button>
      </div>
    `;

    el.querySelector(".dialog-close")!.addEventListener("click", closeDialog);

    // Wire resolve buttons
    el.querySelectorAll<HTMLButtonElement>(".conflict-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const item = btn.closest(".conflict-file-item") as HTMLElement;
        const filePath = item.dataset.path!;
        const resolution = btn.dataset.resolution as "ours" | "theirs";

        btn.disabled = true;
        btn.textContent = "...";

        try {
          await ipc.gitResolveConflict(
            appState.activeWorkspace,
            filePath,
            resolution,
          );
          // Remove from conflicts list
          const idx = conflicts.indexOf(filePath);
          if (idx !== -1) conflicts.splice(idx, 1);
          render();

          if (conflicts.length === 0) {
            toast("All conflicts resolved", "success");
          }
        } catch (err) {
          toast(`Resolve failed: ${err}`, "error");
          btn.disabled = false;
          btn.textContent = resolution === "ours" ? "Ours" : "Theirs";
        }
      });
    });

    // Abort
    el.querySelector("#conflict-abort")!.addEventListener("click", async () => {
      try {
        await ipc.gitAbortMerge(appState.activeWorkspace);
        toast("Merge aborted", "info");
        closeDialog();
      } catch (err) {
        toast(`Abort failed: ${err}`, "error");
      }
    });

    // Continue (all resolved)
    const continueBtn = el.querySelector<HTMLButtonElement>("#conflict-continue")!;
    if (conflicts.length === 0) {
      continueBtn.disabled = false;
      continueBtn.textContent = "Continue";
    }
    continueBtn.addEventListener("click", async () => {
      if (conflicts.length > 0) return;
      continueBtn.disabled = true;
      continueBtn.textContent = "Completing...";
      try {
        const msg = await ipc.gitContinueMerge(appState.activeWorkspace);
        toast(msg, "success");
        closeDialog();
        const files = await ipc.getChangedFiles(appState.activeWorkspace);
        appState.updateFiles(
          appState.activeWorkspace,
          files,
          appState.activeWs?.aheadBehind ?? null,
        );
      } catch (err) {
        toast(`Continue failed: ${err}`, "error");
        continueBtn.disabled = false;
        continueBtn.textContent = "Continue";
      }
    });

    backdrop.appendChild(el);
  }

  const closeDialog = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeDialog();
  });

  document.body.appendChild(backdrop);
  render();
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}
