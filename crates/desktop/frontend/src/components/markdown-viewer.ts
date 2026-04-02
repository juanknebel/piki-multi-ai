import * as ipc from "../ipc";
import { appState } from "../state";

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
  header.innerHTML = `
    <span class="diff-title">${esc(filePath)}</span>
    <button class="dialog-close">×</button>
  `;
  viewer.appendChild(header);

  const body = document.createElement("div");
  body.className = "md-content";
  body.innerHTML = renderMarkdown(content);
  viewer.appendChild(body);

  backdrop.appendChild(viewer);
  document.body.appendChild(backdrop);
  overlayEl = backdrop;

  const close = () => { overlayEl?.remove(); overlayEl = null; };
  header.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

/** Simple markdown to HTML renderer — handles headers, code blocks, bold, italic, lists, links */
function renderMarkdown(src: string): string {
  const lines = src.split("\n");
  const html: string[] = [];
  let inCode = false;
  let codeLang = "";
  let codeLines: string[] = [];
  let inList = false;

  for (const line of lines) {
    // Code blocks
    if (line.startsWith("```")) {
      if (inCode) {
        html.push(`<pre class="md-code"><code>${esc(codeLines.join("\n"))}</code></pre>`);
        codeLines = [];
        inCode = false;
      } else {
        if (inList) { html.push("</ul>"); inList = false; }
        codeLang = line.slice(3).trim();
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      codeLines.push(line);
      continue;
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

  return html.join("\n");
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
