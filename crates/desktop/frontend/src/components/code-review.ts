import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";

interface DraftComment {
  path: string;
  line: number;
  side: string;
  body: string;
}

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
  const comments = new Map<string, DraftComment>();
  let currentFilePath = files.length > 0 ? files[0].path : "";

  function commentKey(path: string, line: number, side: string): string {
    return `${path}:${line}:${side}`;
  }

  function commentCountForFile(path: string): number {
    let count = 0;
    for (const c of comments.values()) {
      if (c.path === path) count++;
    }
    return count;
  }

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
        ${esc(info.baseRefName)} ← ${esc(info.headRefName)}
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

  // Review body textarea
  const reviewBodyArea = document.createElement("div");
  reviewBodyArea.style.cssText = "padding:6px 12px;border-bottom:1px solid var(--border-primary);background:var(--bg-secondary);";
  reviewBodyArea.innerHTML = `
    <textarea class="dialog-textarea" id="cr-body" rows="2" placeholder="Review summary (optional)" style="width:100%;box-sizing:border-box;font-size:12px;resize:vertical"></textarea>
  `;
  panel.appendChild(reviewBodyArea);

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
    fileList.querySelectorAll(".file-item").forEach((el) => el.remove());

    files.forEach((f, i) => {
      const item = document.createElement("div");
      item.className = `file-item${i === selectedFile ? " selected" : ""}`;
      item.style.padding = "4px 8px";
      const cc = commentCountForFile(f.path);
      item.innerHTML = `
        <span class="file-path" style="font-size:11px">${esc(f.path.split("/").pop() || f.path)}</span>
        <span style="margin-left:auto;font-size:10px;display:flex;gap:4px;align-items:center">
          ${cc > 0 ? `<span style="background:var(--accent-primary);color:var(--bg-primary);border-radius:8px;padding:0 5px;font-size:9px;font-weight:700">${cc}</span>` : ""}
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
    currentFilePath = filePath;
    diffArea.innerHTML = '<div style="padding:16px;color:var(--text-muted)">Loading...</div>';
    try {
      const result = await ipc.getPrFileDiff(wsIdx, filePath, info.baseRefName);
      diffArea.innerHTML = "";
      for (const line of result.lines) {
        const lineEl = document.createElement("div");
        lineEl.className = "cr-diff-line";
        if (line.line_type === "Addition") {
          lineEl.style.background = "rgba(35,134,54,0.12)";
          lineEl.style.color = "var(--git-added)";
        } else if (line.line_type === "Deletion") {
          lineEl.style.background = "rgba(218,54,51,0.12)";
          lineEl.style.color = "var(--git-deleted)";
        } else if (line.line_type === "HunkHeader") {
          lineEl.style.background = "rgba(0,122,204,0.08)";
          lineEl.style.color = "var(--text-accent)";
        } else if (line.line_type === "FileHeader") {
          lineEl.style.color = "var(--text-muted)";
        } else {
          lineEl.style.color = "var(--text-secondary)";
        }

        const lineNum = line.new_line ?? line.old_line ?? "";
        lineEl.textContent = `${String(lineNum).padStart(4)} │ ${line.content}`;

        // Determine side and line number for commenting
        const commentLine = line.new_line ?? line.old_line;
        const commentSide = (line.line_type === "Deletion") ? "LEFT" : "RIGHT";

        if (commentLine && (line.line_type === "Addition" || line.line_type === "Deletion" || line.line_type === "Context")) {
          lineEl.style.cursor = "pointer";
          lineEl.title = "Click to add comment";

          lineEl.addEventListener("click", () => {
            // Remove any open comment form
            diffArea.querySelector(".cr-comment-form")?.remove();

            const key = commentKey(filePath, commentLine, commentSide);
            const existing = comments.get(key);

            const form = document.createElement("div");
            form.className = "cr-comment-form";
            form.innerHTML = `
              <textarea class="cr-comment-textarea" rows="2" placeholder="Add a comment...">${existing ? esc(existing.body) : ""}</textarea>
              <div class="cr-comment-form-actions">
                <button class="dialog-btn dialog-btn-primary dialog-btn-sm cr-comment-save">Save</button>
                ${existing ? '<button class="dialog-btn dialog-btn-danger dialog-btn-sm cr-comment-delete">Delete</button>' : ""}
                <button class="dialog-btn dialog-btn-secondary dialog-btn-sm cr-comment-cancel">Cancel</button>
              </div>
            `;

            form.querySelector(".cr-comment-save")!.addEventListener("click", () => {
              const body = (form.querySelector(".cr-comment-textarea") as HTMLTextAreaElement).value.trim();
              if (!body) { form.remove(); return; }
              comments.set(key, { path: filePath, line: commentLine, side: commentSide, body });
              form.remove();
              renderCommentBadge(lineEl, key);
              renderFileList();
            });

            form.querySelector(".cr-comment-cancel")!.addEventListener("click", () => form.remove());

            form.querySelector(".cr-comment-delete")?.addEventListener("click", () => {
              comments.delete(key);
              form.remove();
              // Remove existing badge
              lineEl.nextElementSibling?.classList.contains("cr-comment-badge") && lineEl.nextElementSibling.remove();
              renderFileList();
            });

            // Insert form after the line
            lineEl.after(form);
            (form.querySelector(".cr-comment-textarea") as HTMLTextAreaElement).focus();
          });
        }

        diffArea.appendChild(lineEl);

        // Show existing comment badge
        if (commentLine) {
          const key = commentKey(filePath, commentLine, commentSide);
          if (comments.has(key)) {
            renderCommentBadge(lineEl, key);
          }
        }
      }
    } catch (err) {
      diffArea.innerHTML = `<div style="padding:16px;color:var(--git-deleted)">${esc(String(err))}</div>`;
    }
  }

  function renderCommentBadge(lineEl: HTMLElement, key: string) {
    // Remove existing badge if any
    if (lineEl.nextElementSibling?.classList.contains("cr-comment-badge")) {
      lineEl.nextElementSibling.remove();
    }
    const comment = comments.get(key);
    if (!comment) return;

    const badge = document.createElement("div");
    badge.className = "cr-comment-badge";
    badge.innerHTML = `<span class="cr-comment-body">${esc(comment.body)}</span>`;
    lineEl.after(badge);
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
    const reviewBody = (panel.querySelector("#cr-body") as HTMLTextAreaElement).value.trim();
    const commentsArray = [...comments.values()].map(c => ({
      path: c.path,
      line: c.line,
      side: c.side,
      body: c.body,
    }));

    const btn = panel.querySelector<HTMLButtonElement>("#cr-submit")!;
    btn.disabled = true;
    btn.textContent = "Submitting...";

    try {
      const msg = await ipc.submitPrReview(wsIdx, info.number, verdict, reviewBody, commentsArray);
      toast(msg || "Review submitted", "success");
      close();
    } catch (err) {
      toast(`Submit failed: ${err}`, "error");
      btn.disabled = false;
      btn.textContent = "Submit";
    }
  });

  const close = () => { overlayEl?.remove(); overlayEl = null; };
  panel.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

function esc(t: string | undefined | null): string {
  return (t ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
