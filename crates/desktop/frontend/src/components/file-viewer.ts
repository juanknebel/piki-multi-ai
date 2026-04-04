import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function showFileViewer(workspaceIdx: number, path: string) {
  // Remove existing viewer
  document.querySelector(".file-viewer-backdrop")?.remove();

  let content: string;
  try {
    content = await ipc.readFileContent(workspaceIdx, path);
  } catch (err) {
    toast(`Failed to read file: ${err}`, "error");
    return;
  }

  const fileName = path.split("/").pop() || path;

  const backdrop = document.createElement("div");
  backdrop.className = "file-viewer-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "file-viewer-dialog";

  dialog.innerHTML = `
    <div class="file-viewer-header">
      <span class="file-viewer-title" title="${escapeAttr(path)}">${escapeHtml(fileName)}<span class="file-viewer-path">${escapeHtml(path)}</span></span>
      <div class="file-viewer-actions">
        <button class="file-viewer-btn file-viewer-inline-edit" title="Quick Edit (Ctrl+I)">Quick Edit</button>
        <button class="file-viewer-btn file-viewer-edit" title="Open in $EDITOR (Ctrl+E)">Edit</button>
        <button class="file-viewer-btn file-viewer-copy" title="Copy to clipboard">Copy</button>
        <button class="file-viewer-btn file-viewer-close">&times;</button>
      </div>
    </div>
    <div class="file-viewer-body">
      <pre class="file-viewer-content"><code></code></pre>
    </div>
  `;

  // Set content via textContent to avoid XSS
  dialog.querySelector("code")!.textContent = content;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();

  const body = dialog.querySelector<HTMLElement>(".file-viewer-body")!;
  const actionsDiv = dialog.querySelector<HTMLElement>(".file-viewer-actions")!;

  // ── View mode actions ──────────────────────

  dialog.querySelector(".file-viewer-close")!.addEventListener("click", close);

  dialog.querySelector(".file-viewer-edit")!.addEventListener("click", async () => {
    close();
    try {
      const tabId = await ipc.spawnEditorTab(workspaceIdx, path);
      appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
    } catch (err) {
      toast(`Failed to open editor: ${err}`, "error");
    }
  });

  dialog.querySelector(".file-viewer-copy")!.addEventListener("click", () => {
    writeText(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
  });

  // ── Inline edit mode ──────────────────────

  let editing = false;

  function enterEditMode() {
    if (editing) return;
    editing = true;

    // Replace <pre><code> with <textarea>
    body.innerHTML = "";
    const textarea = document.createElement("textarea");
    textarea.className = "file-viewer-textarea";
    textarea.value = content;
    textarea.spellcheck = false;
    body.appendChild(textarea);

    // Swap action buttons
    actionsDiv.innerHTML = `
      <button class="file-viewer-btn file-viewer-save">Save</button>
      <button class="file-viewer-btn file-viewer-cancel">Cancel</button>
    `;

    actionsDiv.querySelector(".file-viewer-save")!.addEventListener("click", async () => {
      const newContent = textarea.value;
      try {
        await ipc.writeFileContent(workspaceIdx, path, newContent);
        content = newContent;
        toast("File saved", "success");
        exitEditMode();
      } catch (err) {
        toast(`Failed to save: ${err}`, "error");
      }
    });

    actionsDiv.querySelector(".file-viewer-cancel")!.addEventListener("click", () => {
      exitEditMode();
    });

    textarea.focus();
  }

  function exitEditMode() {
    editing = false;

    // Restore view mode
    body.innerHTML = `<pre class="file-viewer-content"><code></code></pre>`;
    body.querySelector("code")!.textContent = content;

    actionsDiv.innerHTML = `
      <button class="file-viewer-btn file-viewer-inline-edit" title="Quick Edit (Ctrl+I)">Quick Edit</button>
      <button class="file-viewer-btn file-viewer-edit" title="Open in $EDITOR (Ctrl+E)">Edit</button>
      <button class="file-viewer-btn file-viewer-copy" title="Copy to clipboard">Copy</button>
      <button class="file-viewer-btn file-viewer-close">&times;</button>
    `;

    actionsDiv.querySelector(".file-viewer-inline-edit")!.addEventListener("click", enterEditMode);
    actionsDiv.querySelector(".file-viewer-edit")!.addEventListener("click", async () => {
      close();
      try {
        const tabId = await ipc.spawnEditorTab(workspaceIdx, path);
        appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
      } catch (err) {
        toast(`Failed to open editor: ${err}`, "error");
      }
    });
    actionsDiv.querySelector(".file-viewer-copy")!.addEventListener("click", () => {
      writeText(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
    });
    actionsDiv.querySelector(".file-viewer-close")!.addEventListener("click", close);

    backdrop.focus();
  }

  dialog.querySelector(".file-viewer-inline-edit")!.addEventListener("click", enterEditMode);

  // ── Keyboard shortcuts ──────────────────────

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop && !editing) close();
  });

  backdrop.addEventListener("keydown", (e) => {
    if (editing) {
      // Ctrl+S to save while editing
      if (e.key === "s" && e.ctrlKey) {
        e.preventDefault();
        (actionsDiv.querySelector(".file-viewer-save") as HTMLButtonElement)?.click();
      }
      // Escape cancels edit mode
      if (e.key === "Escape") {
        e.preventDefault();
        exitEditMode();
      }
      return;
    }

    if (e.key === "Escape") close();
    if (e.key === "i" && e.ctrlKey) {
      e.preventDefault();
      enterEditMode();
    }
    if (e.key === "e" && e.ctrlKey) {
      e.preventDefault();
      close();
      ipc.spawnEditorTab(workspaceIdx, path).then((tabId) => {
        appState.addTab(workspaceIdx, { id: tabId, provider: "Shell", alive: true });
      }).catch((err) => {
        toast(`Failed to open editor: ${err}`, "error");
      });
    }
  });

  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

function escapeHtml(text: string): string {
  const el = document.createElement("span");
  el.textContent = text;
  return el.innerHTML;
}

function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
