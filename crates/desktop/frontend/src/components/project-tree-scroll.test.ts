// The sidebar tree must not jump on click: every render() rebuilds the list,
// so it has to carry the scroll position from the old list to the new one.
// The scroller is `.projects-list` (overflow-y: auto) — NOT #projects-view,
// which has no overflow and always reports scrollTop 0. Reading/restoring on
// the container resets the panel to the top on every click (project toggle,
// checkout switch). Read as text — no DOM — like the other guard tests.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const treeSrc = readFileSync(
  decodeURIComponent(new URL("./project-tree.ts", import.meta.url).pathname),
  "utf8",
);
const css = readFileSync(
  decodeURIComponent(new URL("../styles/projects.css", import.meta.url).pathname),
  "utf8",
);

describe("project tree scroll preservation", () => {
  it("keeps .projects-list as the one scroller", () => {
    expect(css).toMatch(/\.projects-list\s*{[^}]*overflow-y:\s*auto/);
    // #projects-view must not become a scroller of its own — two nested
    // scrollers would split the position the tree preserves.
    for (const m of css.matchAll(/#projects-view\s*{([^}]*)}/g)) {
      expect(m[1], "#projects-view block").not.toMatch(/overflow/);
    }
  });

  it("captures the previous list's scrollTop before rebuilding", () => {
    expect(treeSrc).toMatch(/querySelector.*\.projects-list/);
    expect(treeSrc).toMatch(/prevScroll.*scrollTop|scrollTop.*prevScroll/);
  });

  it("restores the position onto the rebuilt list, never the container", () => {
    expect(treeSrc).toMatch(/list\.scrollTop\s*=\s*prevScroll/);
    expect(treeSrc, "container.scrollTop must not be used for scroll memory").not.toMatch(
      /container\.scrollTop/,
    );
  });
});
