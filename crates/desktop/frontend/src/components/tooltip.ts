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

/** While visible, watch for the target being detached by a re-render so the
 *  tooltip doesn't get stranded on screen. */
let connectedWatch: ReturnType<typeof setInterval> | null = null;

function watchConnected() {
  if (connectedWatch) return;
  connectedWatch = setInterval(() => {
    if (currentTarget && !currentTarget.isConnected) hide();
  }, 300);
}

function hide() {
  if (showTimer) {
    clearTimeout(showTimer);
    showTimer = null;
  }
  if (connectedWatch) {
    clearInterval(connectedWatch);
    connectedWatch = null;
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

function beginShow(target: HTMLElement, delay: number) {
  const text = target.getAttribute("title");
  if (!text) return;

  // Steal the native title to prevent browser tooltip; keep the text
  // reachable for assistive tech.
  target.dataset.title = text;
  target.removeAttribute("title");
  if (!target.hasAttribute("aria-label") && !target.textContent?.trim()) {
    target.setAttribute("aria-label", text);
  }

  hide();
  currentTarget = target;
  watchConnected();
  showTimer = setTimeout(() => show(target, text), delay);
}

export function initTooltips() {
  document.addEventListener("mouseover", (e) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[title]");
    if (target) beginShow(target, DELAY);
  });

  document.addEventListener("mouseout", (e) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-title]");
    if (target) hide();
  });

  // Keyboard parity: focused controls show their tooltip immediately, but
  // only for keyboard-driven focus (mouse clicks also focus, and already
  // have hover).
  document.addEventListener("focusin", (e) => {
    const target = e.target as HTMLElement;
    if (!target.matches?.(":focus-visible")) return;
    const withTitle = target.closest<HTMLElement>("[title], [data-title]");
    if (!withTitle) return;
    if (withTitle.dataset.title) {
      currentTarget = withTitle;
      watchConnected();
      show(withTitle, withTitle.dataset.title);
    } else {
      beginShow(withTitle, 0);
    }
  });

  document.addEventListener("focusout", (e) => {
    const target = (e.target as HTMLElement).closest?.<HTMLElement>("[data-title]");
    if (target && target === currentTarget) hide();
  });

  document.addEventListener("mousedown", hide);
  document.addEventListener("wheel", hide, { passive: true });
}
