import * as ipc from "../ipc";
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

  dialog.querySelector(".file-viewer-close")!.addEventListener("click", close);
  dialog.querySelector(".file-viewer-copy")!.addEventListener("click", () => {
    writeText(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
  });

  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
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
