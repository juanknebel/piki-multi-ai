import { showConfirm } from "../confirm";
import { showWorkspaceDialog } from "./workspace-dialog";

/** Empty state for features that only make sense inside a workspace (agent
 *  profiles, dispatch) when none exists yet: says why instead of failing
 *  with an index error, and offers to create one. `why` is a plain-text
 *  sentence written by the caller (rendered as-is, so no user data). */
export function showNeedsWorkspace(why: string) {
  showConfirm({
    bodyHtml: `
      <p>No workspace yet.</p>
      <p class="ws-delete-hint">${why}</p>
    `,
    actions: [
      { label: "Cancel", kind: "secondary" },
      {
        label: "Create Workspace",
        kind: "primary",
        isDefault: true,
        autofocus: true,
        onSelect: () => showWorkspaceDialog({ mode: "create" }),
      },
    ],
  });
}
