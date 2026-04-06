import type { SideBySideDiff, DiffHunk, DiffPair, ConflictDiff } from "../ipc";

export interface DiffPanelOptions {
  mode: "side-by-side" | "three-way";
  onAcceptOurs?: (regionIdx: number) => void;
  onAcceptTheirs?: (regionIdx: number) => void;
  onAcceptBoth?: (regionIdx: number) => void;
}

/**
 * Reusable diff panel component.
 * Renders side-by-side diffs or 3-way merge conflict views.
 */
export class DiffPanel {
  private container: HTMLElement;
  private opts: DiffPanelOptions;

  constructor(container: HTMLElement, opts: DiffPanelOptions) {
    this.container = container;
    this.opts = opts;
  }

  /** Render a side-by-side diff */
  renderSideBySide(diff: SideBySideDiff) {
    this.container.innerHTML = "";
    this.container.className = "dp-root";

    // Stats bar
    const stats = document.createElement("div");
    stats.className = "dp-stats";
    stats.innerHTML = `
      <span class="dp-stat-add">+${diff.stats.additions}</span>
      <span class="dp-stat-del">-${diff.stats.deletions}</span>
    `;
    this.container.appendChild(stats);

    // Scrollable content
    const scroll = document.createElement("div");
    scroll.className = "dp-scroll";

    const table = document.createElement("div");
    table.className = "dp-table";

    // Column headers as first row of the table (same grid)
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
      // Hunk header row
      const hunkRow = document.createElement("div");
      hunkRow.className = "dp-row dp-hunk-row";
      hunkRow.innerHTML = `<div class="dp-hunk-header" colspan="4">${esc(hunk.header)}</div>`;
      table.appendChild(hunkRow);

      for (const pair of hunk.pairs) {
        table.appendChild(this.createPairRow(pair));
      }
    }

    if (diff.hunks.length === 0) {
      table.innerHTML = '<div class="dp-empty">No changes</div>';
    }

    scroll.appendChild(table);
    this.container.appendChild(scroll);

    // Synchronized scrolling is handled by CSS (single scroll container)
  }

  /** Render multiple file diffs (for commit view) */
  renderMultiFile(diffs: SideBySideDiff[]) {
    this.container.innerHTML = "";
    this.container.className = "dp-root";

    const scroll = document.createElement("div");
    scroll.className = "dp-scroll";

    for (const diff of diffs) {
      // File header
      const fileHeader = document.createElement("div");
      fileHeader.className = "dp-file-header";
      fileHeader.innerHTML = `
        <span class="dp-file-path">${esc(diff.file_path)}</span>
        <span class="dp-stat-add">+${diff.stats.additions}</span>
        <span class="dp-stat-del">-${diff.stats.deletions}</span>
      `;
      scroll.appendChild(fileHeader);

      const table = document.createElement("div");
      table.className = "dp-table";

      for (const hunk of diff.hunks) {
        const hunkRow = document.createElement("div");
        hunkRow.className = "dp-row dp-hunk-row";
        hunkRow.innerHTML = `<div class="dp-hunk-header">${esc(hunk.header)}</div>`;
        table.appendChild(hunkRow);

        for (const pair of hunk.pairs) {
          table.appendChild(this.createPairRow(pair));
        }
      }

      scroll.appendChild(table);
    }

    this.container.appendChild(scroll);
  }

  /** Render 3-way merge conflict view */
  renderConflict(conflict: ConflictDiff) {
    this.container.innerHTML = "";
    this.container.className = "dp-root dp-three-way";

    const scroll = document.createElement("div");
    scroll.className = "dp-scroll";

    // 3-way header row
    const headerRow = document.createElement("div");
    headerRow.className = "dp-3way-row dp-header-row";
    headerRow.innerHTML = `
      <div class="dp-3way-cell dp-col-header">${esc(conflict.ours_title)}</div>
      <div class="dp-3way-cell dp-col-header dp-center-header">RESULT</div>
      <div class="dp-3way-cell dp-col-header">${esc(conflict.theirs_title)}</div>
    `;
    scroll.appendChild(headerRow);

    conflict.regions.forEach((region, regionIdx) => {
      if (region.region_type === "common") {
        for (const line of region.ours_lines) {
          const row = document.createElement("div");
          row.className = "dp-row dp-3way-row dp-context-row";
          row.innerHTML = `
            <div class="dp-3way-cell dp-context">${esc(line)}</div>
            <div class="dp-3way-cell dp-context">${esc(line)}</div>
            <div class="dp-3way-cell dp-context">${esc(line)}</div>
          `;
          scroll.appendChild(row);
        }
      } else {
        // Conflict region
        const block = document.createElement("div");
        block.className = "dp-conflict-block";

        // Action bar
        const actions = document.createElement("div");
        actions.className = "dp-conflict-actions";
        actions.innerHTML = `
          <button class="dp-accept-btn dp-accept-ours" data-idx="${regionIdx}">Accept Ours ►</button>
          <button class="dp-accept-btn dp-accept-both" data-idx="${regionIdx}">Accept Both</button>
          <button class="dp-accept-btn dp-accept-theirs" data-idx="${regionIdx}">◄ Accept Theirs</button>
        `;
        block.appendChild(actions);

        // Lines
        const maxLines = Math.max(region.ours_lines.length, region.theirs_lines.length);
        for (let i = 0; i < maxLines; i++) {
          const oursLine = region.ours_lines[i] ?? "";
          const theirsLine = region.theirs_lines[i] ?? "";
          const row = document.createElement("div");
          row.className = "dp-row dp-3way-row dp-conflict-row";
          row.innerHTML = `
            <div class="dp-3way-cell dp-ours">${i < region.ours_lines.length ? esc(oursLine) : ""}</div>
            <div class="dp-3way-cell dp-result">???</div>
            <div class="dp-3way-cell dp-theirs">${i < region.theirs_lines.length ? esc(theirsLine) : ""}</div>
          `;
          block.appendChild(row);
        }

        // Wire accept buttons
        block.querySelector(".dp-accept-ours")?.addEventListener("click", () => {
          this.opts.onAcceptOurs?.(regionIdx);
          this.resolveConflictVisual(block, region.ours_lines);
        });
        block.querySelector(".dp-accept-theirs")?.addEventListener("click", () => {
          this.opts.onAcceptTheirs?.(regionIdx);
          this.resolveConflictVisual(block, region.theirs_lines);
        });
        block.querySelector(".dp-accept-both")?.addEventListener("click", () => {
          this.opts.onAcceptBoth?.(regionIdx);
          this.resolveConflictVisual(block, [...region.ours_lines, ...region.theirs_lines]);
        });

        scroll.appendChild(block);
      }
    });

    this.container.appendChild(scroll);
  }

  private resolveConflictVisual(block: HTMLElement, resolvedLines: string[]) {
    block.className = "dp-conflict-block dp-resolved";
    block.innerHTML = "";
    for (const line of resolvedLines) {
      const row = document.createElement("div");
      row.className = "dp-row dp-3way-row dp-resolved-row";
      row.innerHTML = `
        <div class="dp-3way-cell dp-context">${esc(line)}</div>
        <div class="dp-3way-cell dp-resolved-content">${esc(line)}</div>
        <div class="dp-3way-cell dp-context">${esc(line)}</div>
      `;
      block.appendChild(row);
    }
  }

  private createPairRow(pair: DiffPair): HTMLElement {
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
      const [lh, rh] = charDiffHighlight(leftContent, rightContent);
      leftHtml = lh;
      rightHtml = rh;
    }

    row.innerHTML = `
      <div class="dp-gutter dp-gutter-left">${leftNum}</div>
      <div class="${leftClass}">${leftHtml || "&nbsp;"}</div>
      <div class="dp-gutter dp-gutter-right">${rightNum}</div>
      <div class="${rightClass}">${rightHtml || "&nbsp;"}</div>
    `;

    return row;
  }
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

function esc(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
