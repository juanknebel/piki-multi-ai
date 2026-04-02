import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";

let overlayEl: HTMLElement | null = null;

export async function showCodeReview() {
  if (overlayEl) { overlayEl.remove(); overlayEl = null; }

  const wsIdx = appState.activeWorkspace;
  let prDetail: ipc.PrDetail | null;
  try {
    prDetail = await ipc.getPrInfo(wsIdx);
  } catch (err) {
    toast(`Failed to load PR: ${err}`, "error");
    return;
  }

  if (!prDetail) {
    toast("No open PR found for this branch", "info");
    return;
  }

  const { info, files } = prDetail;

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.style.paddingTop = "2vh";

  const panel = document.createElement("div");
  panel.className = "diff-viewer";
  panel.style.width = "95vw";
  panel.style.maxHeight = "94vh";

  // Header
  const header = document.createElement("div");
  header.className = "diff-header";
  header.innerHTML = `
    <span class="diff-title">
      <span style="color:var(--text-accent)">PR #${info.number}</span>
      ${esc(info.title)}
      <span style="color:var(--text-muted);font-size:11px;margin-left:8px">
        ${esc(info.base_ref_name)} ← ${esc(info.head_ref_name)}
        <span style="color:var(--git-added)">+${info.additions}</span>
        <span style="color:var(--git-deleted)">-${info.deletions}</span>
      </span>
    </span>
    <span style="display:flex;gap:8px;align-items:center">
      <select class="dialog-select" id="cr-verdict" style="font-size:11px;padding:3px 6px">
        <option value="comment">Comment</option>
        <option value="approve">Approve</option>
        <option value="request_changes">Request Changes</option>
      </select>
      <button class="dialog-btn dialog-btn-primary" id="cr-submit" style="font-size:11px;padding:4px 10px">Submit</button>
      <button class="dialog-close">×</button>
    </span>
  `;
  panel.appendChild(header);

  // Body: file list left, diff right
  const body = document.createElement("div");
  body.style.cssText = "display:flex;flex:1;overflow:hidden;";

  // File list
  const fileList = document.createElement("div");
  fileList.style.cssText = "width:220px;flex-shrink:0;overflow-y:auto;border-right:1px solid var(--border-primary);background:var(--bg-secondary);";

  const fileListHeader = document.createElement("div");
  fileListHeader.className = "sidebar-header";
  fileListHeader.textContent = `FILES (${files.length})`;
  fileList.appendChild(fileListHeader);

  let selectedFile = 0;
  const diffArea = document.createElement("div");
  diffArea.style.cssText = "flex:1;overflow:auto;font-family:'JetBrainsMono NF Mono',monospace;font-size:12px;";

  function renderFileList() {
    // Clear items but keep header
    fileList.querySelectorAll(".file-item").forEach((el) => el.remove());

    files.forEach((f, i) => {
      const item = document.createElement("div");
      item.className = `file-item${i === selectedFile ? " selected" : ""}`;
      item.style.padding = "4px 8px";
      item.innerHTML = `
        <span class="file-path" style="font-size:11px">${esc(f.path.split("/").pop() || f.path)}</span>
        <span style="margin-left:auto;font-size:10px">
          <span style="color:var(--git-added)">+${f.additions}</span>
          <span style="color:var(--git-deleted)">-${f.deletions}</span>
        </span>
      `;
      item.addEventListener("click", async () => {
        selectedFile = i;
        renderFileList();
        await loadFileDiff(f.path);
      });
      fileList.appendChild(item);
    });
  }

  async function loadFileDiff(filePath: string) {
    diffArea.innerHTML = '<div style="padding:16px;color:var(--text-muted)">Loading...</div>';
    try {
      const result = await ipc.getPrFileDiff(wsIdx, filePath, info.base_ref_name);
      diffArea.innerHTML = "";
      for (const line of result.lines) {
        const el = document.createElement("div");
        el.style.cssText = "padding:0 8px;white-space:pre;min-height:1.5em;line-height:1.5;";
        if (line.line_type === "Addition") {
          el.style.background = "rgba(35,134,54,0.12)";
          el.style.color = "var(--git-added)";
        } else if (line.line_type === "Deletion") {
          el.style.background = "rgba(218,54,51,0.12)";
          el.style.color = "var(--git-deleted)";
        } else if (line.line_type === "HunkHeader") {
          el.style.background = "rgba(0,122,204,0.08)";
          el.style.color = "var(--text-accent)";
        } else if (line.line_type === "FileHeader") {
          el.style.color = "var(--text-muted)";
        } else {
          el.style.color = "var(--text-secondary)";
        }

        const lineNum = line.new_line ?? line.old_line ?? "";
        el.textContent = `${String(lineNum).padStart(4)} │ ${line.content}`;
        diffArea.appendChild(el);
      }
    } catch (err) {
      diffArea.innerHTML = `<div style="padding:16px;color:var(--git-deleted)">${esc(String(err))}</div>`;
    }
  }

  body.appendChild(fileList);
  body.appendChild(diffArea);
  panel.appendChild(body);

  backdrop.appendChild(panel);
  document.body.appendChild(backdrop);
  overlayEl = backdrop;

  renderFileList();
  if (files.length > 0) loadFileDiff(files[0].path);

  // Submit review
  panel.querySelector("#cr-submit")!.addEventListener("click", async () => {
    const verdict = (panel.querySelector("#cr-verdict") as HTMLSelectElement).value;
    try {
      const msg = await ipc.submitPrReview(wsIdx, info.number, verdict, "", []);
      toast(msg || "Review submitted", "success");
      close();
    } catch (err) {
      toast(`Submit failed: ${err}`, "error");
    }
  });

  const close = () => { overlayEl?.remove(); overlayEl = null; };
  panel.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
