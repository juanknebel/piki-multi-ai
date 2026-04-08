import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { createDropdown } from "./dropdown";

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
      <span id="cr-verdict-slot"></span>
      <button class="dialog-btn dialog-btn-primary" id="cr-submit" style="font-size:11px;padding:4px 10px">Submit</button>
      <button class="dialog-close">×</button>
    </span>
  `;
  panel.appendChild(header);

  const verdictDropdown = createDropdown([
    { value: "comment", label: "Comment" },
    { value: "approve", label: "Approve" },
    { value: "request_changes", label: "Request Changes" },
  ], "comment", "font-size:11px;padding:3px 6px");
  header.querySelector("#cr-verdict-slot")!.replaceWith(verdictDropdown.container);

  // Review body textarea
  const reviewBodyArea = document.createElement("div");
  reviewBodyArea.style.cssText = "padding:6px 12px;border-bottom:1px solid var(--border-primary);background:var(--bg-secondary);";
  reviewBodyArea.innerHTML = `
    <textarea class="dialog-textarea" id="cr-body" rows="2" placeholder="Review summary (optional)" style="width:100%;box-sizing:border-box;font-size:12px;resize:vertical"></textarea>
  `;
  panel.appendChild(reviewBodyArea);

  // Body: file list left, side-by-side diff right
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

  // Diff area — side-by-side container
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
      const diff = await ipc.getPrFileSideBySideDiff(wsIdx, filePath, info.baseRefName);
      renderSideBySideDiff(diff, filePath);
    } catch (err) {
      diffArea.innerHTML = `<div style="padding:16px;color:var(--git-deleted)">${esc(String(err))}</div>`;
    }
  }

  function renderSideBySideDiff(diff: ipc.SideBySideDiff, filePath: string) {
    diffArea.innerHTML = "";

    // Stats bar
    const stats = document.createElement("div");
    stats.className = "dp-stats";
    stats.innerHTML = `
      <span class="dp-stat-add">+${diff.stats.additions}</span>
      <span class="dp-stat-del">-${diff.stats.deletions}</span>
      <span style="margin-left:auto;color:var(--text-muted);font-size:10px">${esc(filePath)}</span>
    `;
    diffArea.appendChild(stats);

    const scroll = document.createElement("div");
    scroll.className = "dp-scroll";
    scroll.style.cssText = "flex:1;overflow:auto;min-height:0;";

    const table = document.createElement("div");
    table.className = "dp-table";

    // Column headers
    const headerRow = document.createElement("div");
    headerRow.className = "dp-row dp-header-row";
    headerRow.innerHTML = `
      <div class="dp-gutter dp-col-header"></div>
      <div class="dp-cell dp-col-header">${esc(diff.left_title)}</div>
      <div class="dp-gutter dp-gutter-right dp-col-header"></div>
      <div class="dp-cell dp-col-header">${esc(diff.right_title)}</div>
    `;
    table.appendChild(headerRow);

    for (const hunk of diff.hunks) {
      // Hunk header
      const hunkRow = document.createElement("div");
      hunkRow.className = "dp-row dp-hunk-row";
      hunkRow.innerHTML = `<div class="dp-hunk-header">${esc(hunk.header)}</div>`;
      table.appendChild(hunkRow);

      for (const pair of hunk.pairs) {
        const row = createPairRow(pair);
        table.appendChild(row);

        // Add comment click handler for each side
        attachCommentHandler(row, pair, filePath, table);

        // Show existing comment badges
        showExistingBadges(row, pair, filePath, table);
      }
    }

    if (diff.hunks.length === 0) {
      table.innerHTML = '<div class="dp-empty">No changes</div>';
    }

    scroll.appendChild(table);
    diffArea.appendChild(scroll);
  }

  function createPairRow(pair: ipc.DiffPair): HTMLElement {
    const row = document.createElement("div");
    row.className = `dp-row dp-${pair.pair_type}-row`;

    const leftNum = pair.left ? String(pair.left.line_num) : "";
    const rightNum = pair.right ? String(pair.right.line_num) : "";
    const leftContent = pair.left?.content ?? "";
    const rightContent = pair.right?.content ?? "";

    let leftClass = "dp-cell";
    let rightClass = "dp-cell";

    if (pair.pair_type === "modified") {
      leftClass += " dp-del";
      rightClass += " dp-add";
    } else if (pair.pair_type === "deleted") {
      leftClass += " dp-del";
      rightClass += " dp-empty-cell";
    } else if (pair.pair_type === "added") {
      leftClass += " dp-empty-cell";
      rightClass += " dp-add";
    }

    // Char-level diff highlighting for modified lines
    let leftHtml = esc(leftContent);
    let rightHtml = esc(rightContent);
    if (pair.pair_type === "modified" && leftContent && rightContent) {
      [leftHtml, rightHtml] = charDiffHighlight(leftContent, rightContent);
    }

    row.innerHTML = `
      <div class="dp-gutter dp-gutter-left">${leftNum}</div>
      <div class="${leftClass}">${leftHtml || "&nbsp;"}</div>
      <div class="dp-gutter dp-gutter-right">${rightNum}</div>
      <div class="${rightClass}">${rightHtml || "&nbsp;"}</div>
    `;

    // Make clickable rows show pointer
    if (pair.pair_type !== "context" || pair.left || pair.right) {
      row.style.cursor = "pointer";
      row.title = "Click to add comment";
    }

    return row;
  }

  function attachCommentHandler(row: HTMLElement, pair: ipc.DiffPair, filePath: string, table: HTMLElement) {
    // Determine which line/side to comment on
    const commentLine = pair.right?.line_num ?? pair.left?.line_num;
    if (!commentLine) return;

    const commentSide = (pair.pair_type === "deleted") ? "LEFT" : "RIGHT";

    row.addEventListener("click", () => {
      // Remove any open comment form
      table.querySelector(".cr-comment-form")?.remove();

      const key = commentKey(filePath, commentLine, commentSide);
      const existing = comments.get(key);

      const form = document.createElement("div");
      form.className = "cr-comment-form cr-comment-form-sbs";
      form.innerHTML = `
        <textarea class="cr-comment-textarea" rows="2" placeholder="Add a comment on line ${commentLine}...">${existing ? esc(existing.body) : ""}</textarea>
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
        // Re-render badge
        removeBadgeAfter(row);
        insertBadgeAfter(row, key);
        renderFileList();
      });

      form.querySelector(".cr-comment-cancel")!.addEventListener("click", () => form.remove());

      form.querySelector(".cr-comment-delete")?.addEventListener("click", () => {
        comments.delete(key);
        form.remove();
        removeBadgeAfter(row);
        renderFileList();
      });

      // Insert form after the row
      row.after(form);
      (form.querySelector(".cr-comment-textarea") as HTMLTextAreaElement).focus();
    });
  }

  function showExistingBadges(row: HTMLElement, pair: ipc.DiffPair, filePath: string, _table: HTMLElement) {
    const commentLine = pair.right?.line_num ?? pair.left?.line_num;
    if (!commentLine) return;
    const commentSide = (pair.pair_type === "deleted") ? "LEFT" : "RIGHT";
    const key = commentKey(filePath, commentLine, commentSide);
    if (comments.has(key)) {
      insertBadgeAfter(row, key);
    }
  }

  function insertBadgeAfter(row: HTMLElement, key: string) {
    const comment = comments.get(key);
    if (!comment) return;
    const badge = document.createElement("div");
    badge.className = "cr-comment-badge cr-comment-badge-sbs";
    badge.innerHTML = `<span class="cr-comment-body">${esc(comment.body)}</span>`;
    row.after(badge);
  }

  function removeBadgeAfter(row: HTMLElement) {
    if (row.nextElementSibling?.classList.contains("cr-comment-badge")) {
      row.nextElementSibling.remove();
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
    const verdict = verdictDropdown.value;
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

/** Character-level diff: find common prefix/suffix and highlight the changed middle */
function charDiffHighlight(old: string, neu: string): [string, string] {
  let prefixLen = 0;
  const minLen = Math.min(old.length, neu.length);
  while (prefixLen < minLen && old[prefixLen] === neu[prefixLen]) prefixLen++;

  let suffixLen = 0;
  while (
    suffixLen < minLen - prefixLen &&
    old[old.length - 1 - suffixLen] === neu[neu.length - 1 - suffixLen]
  ) {
    suffixLen++;
  }

  const oldPrefix = esc(old.slice(0, prefixLen));
  const oldChanged = old.slice(prefixLen, old.length - suffixLen);
  const oldSuffix = esc(old.slice(old.length - suffixLen));

  const neuPrefix = esc(neu.slice(0, prefixLen));
  const neuChanged = neu.slice(prefixLen, neu.length - suffixLen);
  const neuSuffix = esc(neu.slice(neu.length - suffixLen));

  const leftHtml = oldChanged
    ? `${oldPrefix}<span class="dp-char-del">${esc(oldChanged)}</span>${oldSuffix}`
    : `${oldPrefix}${oldSuffix}`;
  const rightHtml = neuChanged
    ? `${neuPrefix}<span class="dp-char-add">${esc(neuChanged)}</span>${neuSuffix}`
    : `${neuPrefix}${neuSuffix}`;

  return [leftHtml, rightHtml];
}

function esc(t: string | undefined | null): string {
  return (t ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
