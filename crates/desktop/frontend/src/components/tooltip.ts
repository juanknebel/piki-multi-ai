/** Global custom tooltip — replaces native `title` tooltips with themed ones. */

let tooltipEl: HTMLElement | null = null;
let showTimer: ReturnType<typeof setTimeout> | null = null;
let currentTarget: HTMLElement | null = null;

const DELAY = 500;

function create() {
  tooltipEl = document.createElement("div");
  tooltipEl.className = "custom-tooltip";
  document.body.appendChild(tooltipEl);
}

function show(target: HTMLElement, text: string) {
  if (!tooltipEl) create();

  tooltipEl!.textContent = text;
  tooltipEl!.style.display = "block";

  // Position near the target
  const rect = target.getBoundingClientRect();
  const tip = tooltipEl!;

  // Place below by default
  tip.style.left = "0";
  tip.style.top = "0";
  tip.style.visibility = "hidden";

  requestAnimationFrame(() => {
    const tipRect = tip.getBoundingClientRect();
    let left = rect.left + (rect.width - tipRect.width) / 2;
    let top = rect.bottom + 6;

    // Keep in viewport
    if (left < 4) left = 4;
    if (left + tipRect.width > window.innerWidth - 4) {
      left = window.innerWidth - tipRect.width - 4;
    }
    // Flip above if no room below
    if (top + tipRect.height > window.innerHeight - 4) {
      top = rect.top - tipRect.height - 6;
    }

    tip.style.left = `${left}px`;
    tip.style.top = `${top}px`;
    tip.style.visibility = "visible";
  });
}

function hide() {
  if (showTimer) {
    clearTimeout(showTimer);
    showTimer = null;
  }
  if (tooltipEl) tooltipEl.style.display = "none";
  if (currentTarget) {
    const saved = currentTarget.dataset.title;
    if (saved) {
      currentTarget.setAttribute("title", saved);
      delete currentTarget.dataset.title;
    }
    currentTarget = null;
  }
}

export function initTooltips() {
  document.addEventListener("mouseover", (e) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[title]");
    if (!target) return;

    const text = target.getAttribute("title");
    if (!text) return;

    // Steal the native title to prevent browser tooltip
    target.dataset.title = text;
    target.removeAttribute("title");

    hide();
    currentTarget = target;
    showTimer = setTimeout(() => show(target, text), DELAY);
  });

  document.addEventListener("mouseout", (e) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-title]");
    if (target) hide();
  });

  document.addEventListener("mousedown", hide);
  document.addEventListener("wheel", hide, { passive: true });
}
