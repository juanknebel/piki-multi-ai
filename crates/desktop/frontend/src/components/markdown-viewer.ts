import * as ipc from "../ipc";
import { appState } from "../state";
import { toast } from "./toast";
import { registerMarkdownFile } from "./markdown-editor-panel";
import { modCtrl, formatShortcut } from "../shortcuts";

let overlayEl: HTMLElement | null = null;

export async function showMarkdown(filePath: string) {
  if (overlayEl) { overlayEl.remove(); overlayEl = null; }

  const wsIdx = appState.activeWorkspace;
  let content: string;
  try {
    content = await ipc.readMarkdownFile(wsIdx, filePath);
  } catch (err) {
    console.error("Failed to read markdown:", err);
    return;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";
  backdrop.style.paddingTop = "3vh";

  const viewer = document.createElement("div");
  viewer.className = "diff-viewer";
  viewer.style.maxWidth = "800px";

  const header = document.createElement("div");
  header.className = "diff-header";

  const fileName = filePath.split("/").pop() || filePath;

  const close = () => { overlayEl?.remove(); overlayEl = null; };
  let editing = false;

  function renderViewMode() {
    header.innerHTML = `
      <span class="diff-title">${esc(fileName)}<span style="color:var(--text-muted);font-weight:400;margin-left:8px;font-size:11px">${esc(filePath)}</span></span>
      <div style="display:flex;gap:4px;align-items:center">
        <button class="file-viewer-btn md-quick-edit" title="Quick Edit (${formatShortcut("Ctrl+I")})">Quick Edit</button>
        <button class="file-viewer-btn md-edit" title="Open in $EDITOR (${formatShortcut("Ctrl+E")})">Edit</button>
        <button class="file-viewer-btn md-copy" title="Copy to clipboard">Copy</button>
        <button class="dialog-close">×</button>
      </div>
    `;

    const body = viewer.querySelector(".md-body") as HTMLElement;
    body.innerHTML = "";
    const mdContent = document.createElement("div");
    mdContent.className = "md-content";
    mdContent.innerHTML = renderMarkdown(content);
    body.appendChild(mdContent);

    header.querySelector(".dialog-close")!.addEventListener("click", close);
    header.querySelector(".md-quick-edit")!.addEventListener("click", enterEditMode);
    header.querySelector(".md-edit")!.addEventListener("click", () => {
      close();
      const tabId = `md-${Date.now()}`;
      registerMarkdownFile(tabId, filePath);
      appState.addTab(wsIdx, { id: tabId, provider: "Markdown", alive: true });
    });
    header.querySelector(".md-copy")!.addEventListener("click", () => {
      ipc.clipboardCopy(content).then(() => toast("Copied to clipboard", "success")).catch(() => {});
    });
  }

  function enterEditMode() {
    if (editing) return;
    editing = true;

    header.innerHTML = `
      <span class="diff-title">${esc(fileName)} <span style="color:var(--text-muted);font-weight:400;font-size:11px">(editing)</span></span>
      <div style="display:flex;gap:4px;align-items:center">
        <button class="file-viewer-btn md-save">Save</button>
        <button class="file-viewer-btn md-cancel">Cancel</button>
      </div>
    `;

    const body = viewer.querySelector(".md-body") as HTMLElement;
    body.innerHTML = "";
    const textarea = document.createElement("textarea");
    textarea.className = "file-viewer-textarea";
    textarea.value = content;
    textarea.spellcheck = false;
    body.appendChild(textarea);
    textarea.focus();

    header.querySelector(".md-save")!.addEventListener("click", async () => {
      try {
        await ipc.writeFileContent(wsIdx, filePath, textarea.value);
        content = textarea.value;
        toast("File saved", "success");
        editing = false;
        renderViewMode();
      } catch (err) {
        toast(`Failed to save: ${err}`, "error");
      }
    });
    header.querySelector(".md-cancel")!.addEventListener("click", () => {
      editing = false;
      renderViewMode();
    });
  }

  viewer.appendChild(header);

  const body = document.createElement("div");
  body.className = "md-body";
  body.style.cssText = "flex:1;overflow:auto;min-height:0";
  viewer.appendChild(body);

  backdrop.appendChild(viewer);
  document.body.appendChild(backdrop);
  overlayEl = backdrop;

  renderViewMode();

  backdrop.addEventListener("click", (e) => { if (e.target === backdrop && !editing) close(); });
  backdrop.addEventListener("keydown", (e) => {
    if (editing) {
      if (e.key === "s" && modCtrl(e)) {
        e.preventDefault();
        (header.querySelector(".md-save") as HTMLButtonElement)?.click();
      }
      if (e.key === "Escape") { e.preventDefault(); editing = false; renderViewMode(); }
      return;
    }
    if (e.key === "Escape") close();
    if (e.key === "i" && modCtrl(e)) { e.preventDefault(); enterEditMode(); }
    if (e.key === "e" && modCtrl(e)) {
      e.preventDefault();
      close();
      ipc.spawnEditorTab(wsIdx, filePath).then((tabId) => {
        appState.addTab(wsIdx, { id: tabId, provider: "Shell", alive: true });
      }).catch((err) => toast(`Failed to open editor: ${err}`, "error"));
    }
  });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

/** Simple markdown to HTML renderer — handles headers, code blocks, bold, italic, lists, links, tables */
function renderMarkdown(src: string): string {
  const lines = src.split("\n");
  const html: string[] = [];
  let inCode = false;
  let codeLines: string[] = [];
  let inList = false;
  let inTable = false;
  let tableHeaderDone = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Code blocks
    if (line.startsWith("```")) {
      if (inCode) {
        html.push(`<pre class="md-code"><code>${esc(codeLines.join("\n"))}</code></pre>`);
        codeLines = [];
        inCode = false;
      } else {
        if (inList) { html.push("</ul>"); inList = false; }
        if (inTable) { html.push("</tbody></table>"); inTable = false; tableHeaderDone = false; }
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      codeLines.push(line);
      continue;
    }

    // Table: detect rows starting and ending with |
    const trimmed = line.trim();
    if (trimmed.startsWith("|") && trimmed.endsWith("|")) {
      // Check if this is a separator row (|---|---|)
      const isSeparator = /^\|[\s\-:|]+\|$/.test(trimmed);

      if (!inTable) {
        // Start a new table — this line is the header row
        if (inList) { html.push("</ul>"); inList = false; }
        inTable = true;
        tableHeaderDone = false;
        html.push('<table class="md-table"><thead><tr>');
        const cells = parsePipeCells(trimmed);
        for (const cell of cells) {
          html.push(`<th>${inline(cell.trim())}</th>`);
        }
        html.push("</tr></thead>");
        continue;
      }

      if (isSeparator) {
        // Separator row after header — skip it, start tbody
        tableHeaderDone = true;
        html.push("<tbody>");
        continue;
      }

      // Regular data row
      if (!tableHeaderDone) {
        // No separator seen yet — treat as body anyway
        tableHeaderDone = true;
        html.push("<tbody>");
      }
      html.push("<tr>");
      const cells = parsePipeCells(trimmed);
      for (const cell of cells) {
        html.push(`<td>${inline(cell.trim())}</td>`);
      }
      html.push("</tr>");
      continue;
    }

    // If we were in a table and hit a non-table line, close it
    if (inTable) {
      if (!tableHeaderDone) html.push("<tbody>");
      html.push("</tbody></table>");
      inTable = false;
      tableHeaderDone = false;
    }

    // Headers
    const headerMatch = line.match(/^(#{1,6})\s+(.+)/);
    if (headerMatch) {
      if (inList) { html.push("</ul>"); inList = false; }
      const level = headerMatch[1].length;
      html.push(`<h${level} class="md-h">${inline(headerMatch[2])}</h${level}>`);
      continue;
    }

    // Unordered list
    if (line.match(/^\s*[-*+]\s+/)) {
      if (!inList) { html.push("<ul class='md-list'>"); inList = true; }
      html.push(`<li>${inline(line.replace(/^\s*[-*+]\s+/, ""))}</li>`);
      continue;
    }

    // Ordered list
    if (line.match(/^\s*\d+\.\s+/)) {
      if (!inList) { html.push("<ol class='md-list'>"); inList = true; }
      html.push(`<li>${inline(line.replace(/^\s*\d+\.\s+/, ""))}</li>`);
      continue;
    }

    if (inList && line.trim() === "") {
      html.push("</ul>");
      inList = false;
      continue;
    }

    // Horizontal rule
    if (line.match(/^---+$/)) {
      html.push("<hr class='md-hr'/>");
      continue;
    }

    // Inline HTML — pass safe tags through unescaped
    if (/^\s*<\/?(?:p|div|img|br|hr|span|center|b|i|em|strong|a|table|tr|td|th|thead|tbody|h[1-6]|ul|ol|li|blockquote|pre|code|details|summary|picture|source|figure|figcaption|sub|sup|small|mark|del|ins|kbd|abbr|dl|dt|dd)[\s>/]/i.test(trimmed)) {
      html.push(line);
      continue;
    }

    // Empty line
    if (line.trim() === "") {
      html.push("<br/>");
      continue;
    }

    // Paragraph
    html.push(`<p class="md-p">${inline(line)}</p>`);
  }

  if (inCode) {
    html.push(`<pre class="md-code"><code>${esc(codeLines.join("\n"))}</code></pre>`);
  }
  if (inList) html.push("</ul>");
  if (inTable) {
    if (!tableHeaderDone) html.push("<tbody>");
    html.push("</tbody></table>");
  }

  return html.join("\n");
}

/** Parse pipe-delimited table cells: |a|b|c| → ["a","b","c"] */
function parsePipeCells(line: string): string[] {
  return line.slice(1, -1).split("|");
}

/** Inline markdown: bold, italic, code, links */
function inline(text: string): string {
  let s = esc(text);
  s = s.replace(/`([^`]+)`/g, '<code class="md-inline-code">$1</code>');
  s = s.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
  s = s.replace(/\*(.+?)\*/g, "<em>$1</em>");
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a class="md-link" href="$2">$1</a>');
  return s;
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
