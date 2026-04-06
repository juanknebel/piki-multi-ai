import { appState } from "../state";
import * as ipc from "../ipc";
import { toast } from "./toast";
import { showDispatchDialog } from "./dialogs/dispatch-dialog";
import type { KanbanBoard, KanbanCard } from "../types";
import { PRIORITY_CSS } from "../types";

interface KanbanInstance {
  tabId: string;
  element: HTMLDivElement;
  board: KanbanBoard | null;
}

// ── Column colors ────────────────────────────────

const DEFAULT_COLUMN_COLORS: Record<string, string> = {
  todo: "#39bae6",
  in_progress: "#e6a730",
  in_review: "#7b61ff",
  done: "#3fb950",
};

const COLOR_PALETTE = [
  "#39bae6", "#e6a730", "#7b61ff", "#3fb950",
  "#f85149", "#f778ba", "#d2a8ff", "#79c0ff",
  "#56d4dd", "#a5d6ff", "#ffa657", "#ff7b72",
  "#8b949e", "#c9d1d9", "#e3b341", "#7ee787",
];

const STORAGE_KEY = "kanban-column-colors";

function loadColumnColors(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return {};
}

function saveColumnColors(colors: Record<string, string>) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(colors));
}

function getColumnColor(colId: string): string {
  const saved = loadColumnColors();
  return saved[colId] ?? DEFAULT_COLUMN_COLORS[colId] ?? "";
}

const instances = new Map<string, KanbanInstance>();
let mainContent: HTMLElement;

export function initKanbanPanel(container: HTMLElement) {
  mainContent = container;
}

export function hideKanbanPanels() {
  for (const inst of instances.values()) {
    inst.element.style.display = "none";
  }
}

export async function showKanbanPanel(tabId: string) {
  let inst = instances.get(tabId);
  if (!inst) {
    const el = document.createElement("div");
    el.className = "kanban-board";
    mainContent.appendChild(el);
    inst = { tabId, element: el, board: null };
    instances.set(tabId, inst);
  }

  inst.element.style.display = "flex";
  await loadAndRender(inst);
}

export function destroyKanbanPanel(tabId: string) {
  const inst = instances.get(tabId);
  if (inst) {
    inst.element.remove();
    instances.delete(tabId);
  }
}

async function loadAndRender(inst: KanbanInstance) {
  const wsIdx = appState.activeWorkspace;
  try {
    inst.board = await ipc.kanbanLoadBoard(wsIdx);
  } catch (err) {
    inst.element.innerHTML = `
      <div class="kanban-empty">
        <span class="kanban-empty-icon">B</span>
        <p>Could not load kanban board</p>
        <p class="kanban-empty-detail">${esc(String(err))}</p>
        <button class="kanban-btn" onclick="this.closest('.kanban-board').querySelector('.kanban-refresh')?.click()">Retry</button>
      </div>`;
    return;
  }
  renderBoard(inst);
}

function renderBoard(inst: KanbanInstance) {
  const board = inst.board;
  if (!board) return;
  const wsIdx = appState.activeWorkspace;

  const el = inst.element;
  el.innerHTML = "";

  // Toolbar
  const toolbar = document.createElement("div");
  toolbar.className = "kanban-toolbar";
  toolbar.innerHTML = `
    <span class="kanban-toolbar-title">Kanban Board</span>
    <button class="kanban-btn kanban-refresh" title="Refresh">Refresh</button>
  `;
  toolbar.querySelector(".kanban-refresh")!.addEventListener("click", () => loadAndRender(inst));
  el.appendChild(toolbar);

  // Columns container
  const colsContainer = document.createElement("div");
  colsContainer.className = "kanban-columns";

  board.columns.forEach((col, colIdx) => {
    const colEl = document.createElement("div");
    colEl.className = "kanban-column";

    // Apply column color
    const colColor = getColumnColor(col.id);
    if (colColor) {
      colEl.style.setProperty("--kanban-col-color", colColor);
    }

    // Column header
    const header = document.createElement("div");
    header.className = "kanban-column-header";

    const COLUMN_LABELS: Record<string, string> = {
      todo: "TO DO",
      in_progress: "IN PROGRESS",
      in_review: "IN REVIEW",
      done: "DONE",
    };

    header.innerHTML = `
      <span>
        <span class="kanban-column-title">${esc(COLUMN_LABELS[col.id] ?? col.id)}</span>
        <span class="kanban-column-count">${col.cards.length}</span>
      </span>
      <button class="kanban-column-add" title="Add card">+</button>
    `;

    // Right-click header to pick column color
    header.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      showColorPicker(inst, col.id, colEl, e.clientX, e.clientY);
    });

    header.querySelector(".kanban-column-add")!.addEventListener("click", async () => {
      try {
        const cardId = await ipc.kanbanCreateCard(wsIdx, col.id);
        await loadAndRender(inst);
        // Open edit for the new card
        const newCard = inst.board?.columns
          .flatMap((c) => c.cards)
          .find((c) => c.id === cardId);
        if (newCard) showEditModal(inst, newCard);
      } catch (err) {
        toast(`Failed to create card: ${err}`, "error");
      }
    });
    colEl.appendChild(header);

    // Cards container
    const cardsEl = document.createElement("div");
    cardsEl.className = "kanban-cards";

    col.cards.forEach((card) => {
      const cardEl = renderCard(inst, card, colIdx, board);
      cardsEl.appendChild(cardEl);
    });

    // Drag-and-drop: column as drop target
    colEl.addEventListener("dragover", (e) => {
      e.preventDefault();
      e.dataTransfer!.dropEffect = "move";
      colEl.classList.add("kanban-column-drop-over");
    });
    colEl.addEventListener("dragleave", (e) => {
      // Only remove if leaving the column itself, not entering a child
      if (!colEl.contains(e.relatedTarget as Node)) {
        colEl.classList.remove("kanban-column-drop-over");
      }
    });
    colEl.addEventListener("drop", async (e) => {
      e.preventDefault();
      colEl.classList.remove("kanban-column-drop-over");
      const raw = e.dataTransfer!.getData("text/plain");
      if (!raw) return;
      try {
        const { cardId, fromCol } = JSON.parse(raw) as { cardId: string; fromCol: string };
        if (fromCol === col.id) return; // Dropped in same column
        await ipc.kanbanMoveCard(wsIdx, cardId, col.id);
        await loadAndRender(inst);
      } catch (err) {
        toast(`Move failed: ${err}`, "error");
      }
    });

    colEl.appendChild(cardsEl);
    colsContainer.appendChild(colEl);
  });

  el.appendChild(colsContainer);
}

function renderCard(
  inst: KanbanInstance,
  card: KanbanCard,
  colIdx: number,
  board: KanbanBoard,
): HTMLDivElement {
  const wsIdx = appState.activeWorkspace;
  const el = document.createElement("div");
  el.className = "kanban-card";
  el.draggable = true;

  // Drag-and-drop: start
  el.addEventListener("dragstart", (e) => {
    el.classList.add("kanban-card-dragging");
    e.dataTransfer!.effectAllowed = "move";
    e.dataTransfer!.setData("text/plain", JSON.stringify({ cardId: card.id, fromCol: board.columns[colIdx].id }));
  });
  el.addEventListener("dragend", () => {
    el.classList.remove("kanban-card-dragging");
    // Clean up all drop indicators
    el.closest(".kanban-columns")?.querySelectorAll(".kanban-column-drop-over").forEach((c) => c.classList.remove("kanban-column-drop-over"));
  });

  const prioClass = PRIORITY_CSS[card.priority] ?? "priority-medium";
  const shortId = card.id.length > 18 ? card.id.slice(0, 18) + "..." : card.id;

  el.innerHTML = `
    <div class="kanban-card-header">
      <span class="kanban-card-title">${esc(card.title)}</span>
      <span class="kanban-priority ${prioClass}">${esc(card.priority)}</span>
    </div>
    ${card.assignee ? `<div class="kanban-card-assignee">${esc(card.assignee)}</div>` : ""}
    <div class="kanban-card-id">${esc(shortId)}</div>
    <div class="kanban-card-actions">
      ${colIdx > 0 ? `<button class="kanban-card-btn kanban-move-left" title="Move left">&larr;</button>` : ""}
      <button class="kanban-card-btn kanban-edit" title="Edit">Edit</button>
      <button class="kanban-card-btn kanban-dispatch" title="Dispatch agent">Dispatch</button>
      <button class="kanban-card-btn kanban-delete" title="Delete">Del</button>
      ${colIdx < board.columns.length - 1 ? `<button class="kanban-card-btn kanban-move-right" title="Move right">&rarr;</button>` : ""}
    </div>
  `;

  // Move left
  el.querySelector(".kanban-move-left")?.addEventListener("click", async (e) => {
    e.stopPropagation();
    try {
      await ipc.kanbanMoveCard(wsIdx, card.id, board.columns[colIdx - 1].id);
      await loadAndRender(inst);
    } catch (err) {
      toast(`Move failed: ${err}`, "error");
    }
  });

  // Move right
  el.querySelector(".kanban-move-right")?.addEventListener("click", async (e) => {
    e.stopPropagation();
    try {
      await ipc.kanbanMoveCard(wsIdx, card.id, board.columns[colIdx + 1].id);
      await loadAndRender(inst);
    } catch (err) {
      toast(`Move failed: ${err}`, "error");
    }
  });

  // Edit
  el.querySelector(".kanban-edit")!.addEventListener("click", (e) => {
    e.stopPropagation();
    showEditModal(inst, card);
  });

  // Dispatch
  el.querySelector(".kanban-dispatch")!.addEventListener("click", (e) => {
    e.stopPropagation();
    showDispatchDialog({
      id: card.id,
      title: card.title,
      description: card.description,
      priority: card.priority,
    }).then(() => loadAndRender(inst));
  });

  // Delete
  el.querySelector(".kanban-delete")!.addEventListener("click", (e) => {
    e.stopPropagation();
    showDeleteConfirm(el, inst, card);
  });

  return el;
}

function showDeleteConfirm(cardEl: HTMLDivElement, inst: KanbanInstance, card: KanbanCard) {
  const wsIdx = appState.activeWorkspace;
  const actions = cardEl.querySelector(".kanban-card-actions") as HTMLElement;
  if (!actions) return;
  const original = actions.innerHTML;

  actions.innerHTML = `
    <span class="kanban-confirm-text">Delete?</span>
    <button class="kanban-card-btn kanban-confirm-yes">Yes</button>
    <button class="kanban-card-btn kanban-confirm-no">No</button>
  `;

  actions.querySelector(".kanban-confirm-yes")!.addEventListener("click", async (e) => {
    e.stopPropagation();
    try {
      await ipc.kanbanDeleteCard(wsIdx, card.id);
      await loadAndRender(inst);
    } catch (err) {
      toast(`Delete failed: ${err}`, "error");
    }
  });

  actions.querySelector(".kanban-confirm-no")!.addEventListener("click", (e) => {
    e.stopPropagation();
    actions.innerHTML = original;
  });
}

function showEditModal(inst: KanbanInstance, card: KanbanCard) {
  const wsIdx = appState.activeWorkspace;
  // Remove existing modal if any
  document.querySelector(".kanban-edit-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "kanban-edit-backdrop";

  const modal = document.createElement("div");
  modal.className = "kanban-edit-modal";
  modal.innerHTML = `
    <div class="kanban-edit-header">
      <span>Edit Card</span>
      <button class="kanban-edit-close">&times;</button>
    </div>
    <div class="kanban-edit-body">
      <label class="kanban-edit-label">Title</label>
      <input class="kanban-edit-input" id="ke-title" type="text" value="${escAttr(card.title)}" />

      <label class="kanban-edit-label">Priority</label>
      <select class="kanban-edit-select" id="ke-priority">
        <option value="Bug"${card.priority === "Bug" ? " selected" : ""}>Bug</option>
        <option value="High"${card.priority === "High" ? " selected" : ""}>High</option>
        <option value="Medium"${card.priority === "Medium" ? " selected" : ""}>Medium</option>
        <option value="Low"${card.priority === "Low" ? " selected" : ""}>Low</option>
        <option value="Wishlist"${card.priority === "Wishlist" ? " selected" : ""}>Wishlist</option>
      </select>

      <label class="kanban-edit-label">Assignee</label>
      <input class="kanban-edit-input" id="ke-assignee" type="text" value="${escAttr(card.assignee)}" />

      <label class="kanban-edit-label">Description</label>
      <textarea class="kanban-edit-textarea" id="ke-desc" rows="6">${esc(card.description)}</textarea>
    </div>
    <div class="kanban-edit-footer">
      <button class="kanban-btn kanban-edit-cancel">Cancel</button>
      <button class="kanban-btn kanban-btn-primary kanban-edit-save">Save</button>
    </div>
  `;

  backdrop.appendChild(modal);
  document.body.appendChild(backdrop);

  // Focus title input
  const titleInput = modal.querySelector("#ke-title") as HTMLInputElement;
  titleInput.focus();
  titleInput.select();

  // Close handlers
  const close = () => backdrop.remove();
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  modal.querySelector(".kanban-edit-close")!.addEventListener("click", close);
  modal.querySelector(".kanban-edit-cancel")!.addEventListener("click", close);

  // Save
  modal.querySelector(".kanban-edit-save")!.addEventListener("click", async () => {
    const title = (modal.querySelector("#ke-title") as HTMLInputElement).value.trim();
    const priority = (modal.querySelector("#ke-priority") as HTMLSelectElement).value;
    const assignee = (modal.querySelector("#ke-assignee") as HTMLInputElement).value.trim();
    const desc = (modal.querySelector("#ke-desc") as HTMLTextAreaElement).value;

    if (!title) {
      toast("Title cannot be empty", "error");
      return;
    }

    try {
      await ipc.kanbanUpdateCard(wsIdx, card.id, title, desc, priority, assignee);
      close();
      await loadAndRender(inst);
    } catch (err) {
      toast(`Save failed: ${err}`, "error");
    }
  });

  // Ctrl+Enter to save
  modal.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      close();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      modal.querySelector<HTMLButtonElement>(".kanban-edit-save")!.click();
    }
  });
}

function showColorPicker(
  inst: KanbanInstance,
  colId: string,
  colEl: HTMLDivElement,
  x: number,
  y: number,
) {
  // Remove any existing picker
  document.querySelector(".kanban-color-picker")?.remove();

  const picker = document.createElement("div");
  picker.className = "kanban-color-picker";
  picker.style.left = `${x}px`;
  picker.style.top = `${y}px`;

  const currentColor = getColumnColor(colId);

  for (const color of COLOR_PALETTE) {
    const swatch = document.createElement("div");
    swatch.className = "kanban-color-swatch";
    if (color === currentColor) swatch.classList.add("active");
    swatch.style.background = color;
    swatch.addEventListener("click", () => {
      const colors = loadColumnColors();
      colors[colId] = color;
      saveColumnColors(colors);
      colEl.style.setProperty("--kanban-col-color", color);
      picker.remove();
    });
    picker.appendChild(swatch);
  }

  const resetBtn = document.createElement("button");
  resetBtn.className = "kanban-color-reset";
  resetBtn.textContent = "Reset to default";
  resetBtn.addEventListener("click", () => {
    const colors = loadColumnColors();
    delete colors[colId];
    saveColumnColors(colors);
    const def = DEFAULT_COLUMN_COLORS[colId] ?? "";
    if (def) {
      colEl.style.setProperty("--kanban-col-color", def);
    } else {
      colEl.style.removeProperty("--kanban-col-color");
    }
    picker.remove();
  });
  picker.appendChild(resetBtn);

  document.body.appendChild(picker);

  // Close on click outside
  const close = (e: MouseEvent) => {
    if (!picker.contains(e.target as Node)) {
      picker.remove();
      document.removeEventListener("mousedown", close);
    }
  };
  setTimeout(() => document.addEventListener("mousedown", close), 0);
}

function esc(s: string): string {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}

function escAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
