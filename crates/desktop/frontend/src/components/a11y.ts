/** Make a non-button element keyboard-operable: focusable, announced with
 *  `role`, and activatable with Enter/Space (which forward to `click`).
 *  Use for interactive rows/tabs built from divs; real <button>s don't
 *  need it. */
export function makeInteractive(el: HTMLElement, role = "button") {
  el.tabIndex = 0;
  el.setAttribute("role", role);
  el.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      el.click();
    }
  });
}
