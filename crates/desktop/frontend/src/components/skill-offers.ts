// Offers a freshly added/created workspace's piki skills (today: piki-ctl)
// for injection into its AGENTS.md, so any coding agent reading that file —
// Claude Code, Codex, muse-cli, whatever — picks them up. One dialog per
// workspace add, only when there's something undecided; `ipc.sync...`
// already silently re-applies whatever was already accepted for this repo
// before we even get here, so a second worktree of a decided repo never
// prompts again.

import * as ipc from "../ipc";
import { showConfirm, escapeHtml } from "./confirm";
import { toast, reportError } from "./toast";

export async function offerSkillsIfPending(workspaceIdx: number): Promise<void> {
  let offers: ipc.SkillOffer[];
  try {
    offers = await ipc.syncAndListPendingSkillOffers(workspaceIdx);
  } catch {
    // Best-effort — never block the workspace-add flow on this.
    return;
  }
  if (offers.length === 0) return;

  const list = offers
    .map((o) => `<li><strong>${escapeHtml(o.title)}</strong></li>`)
    .join("");

  showConfirm({
    bodyHtml: `
      <p>Add instructions for your coding agents to this repo's <code>AGENTS.md</code>?</p>
      <ul class="ws-delete-hint">${list}</ul>
      <p class="ws-delete-hint">Wrapped in a marked block piki can update later; declining never touches the file.</p>
    `,
    actions: [
      {
        label: "Add to AGENTS.md",
        kind: "primary",
        isDefault: true,
        autofocus: true,
        keepOpen: true,
        onSelect: async ({ close }) => {
          try {
            await ipc.respondToSkillOffers(workspaceIdx, offers.map((o) => o.id), []);
            toast("Added piki skills to AGENTS.md", "success");
          } catch (err) {
            reportError("Failed to update AGENTS.md", err);
          }
          close();
        },
      },
      {
        label: "Not now",
        kind: "secondary",
        onSelect: () => {
          ipc.respondToSkillOffers(workspaceIdx, [], offers.map((o) => o.id)).catch(() => {});
        },
      },
    ],
  });
}
