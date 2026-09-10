// The git loop's shared actions — push, pull, discard — used by the Source
// Control panel, the Git menu and the command palette alike, so every entry
// point has the same guard (one in-flight op per workspace via
// `runExclusive`), the same status refresh and the same toasts.

import { appState } from "../state";
import * as ipc from "../ipc";
import { reportError, toast } from "./toast";
import { runExclusive } from "../in-flight";
import { escapeHtml, showConfirm } from "./confirm";
import type { ChangedFile } from "../types";

export const pushKey = (wsIdx: number) => `git-push:${wsIdx}`;
export const pullKey = (wsIdx: number) => `git-pull:${wsIdx}`;

/** Re-read files + ahead/behind after an operation changed them. */
export async function refreshGitStatus(wsIdx: number): Promise<void> {
  try {
    const status = await ipc.getWorkspaceGitStatus(wsIdx);
    appState.updateFiles(wsIdx, status.files, status.ahead_behind);
  } catch (err) {
    reportError("Failed to refresh git status", err);
  }
}

/** `git push` of a workspace: a second call while one is running is a no-op
 *  (a double click = one push). */
export async function pushWorkspace(wsIdx: number = appState.activeWorkspace): Promise<void> {
  const ran = await runExclusive(pushKey(wsIdx), async () => {
    try {
      await ipc.gitPush(wsIdx);
      toast("Pushed successfully", "success");
    } catch (err) {
      reportError("Push failed", err);
    }
    await refreshGitStatus(wsIdx);
    return true;
  });
  if (!ran) toast("Push already in progress", "info");
}

/** `git pull`; the toast says what changed ("Pulled 3 commits"). A diverged
 *  branch or a conflicting pull surfaces git's message as the error. */
export async function pullWorkspace(wsIdx: number = appState.activeWorkspace): Promise<void> {
  const ran = await runExclusive(pullKey(wsIdx), async () => {
    try {
      const result = await ipc.gitPull(wsIdx);
      toast(result.summary, "success");
    } catch (err) {
      reportError("Pull failed", err);
    }
    await refreshGitStatus(wsIdx);
    return true;
  });
  if (!ran) toast("Pull already in progress", "info");
}

/** Confirm, then throw away a file's working-tree changes — or delete it
 *  when it is untracked (git holds no copy to restore). */
export function confirmDiscardFile(
  wsIdx: number,
  file: ChangedFile,
  onDone: () => Promise<void>,
) {
  const untracked = file.status === "Untracked";
  const name = `<strong>${escapeHtml(file.path)}</strong>`;
  const bodyHtml = untracked
    ? `<p>Delete ${name}?</p>
       <p class="ws-delete-hint">The file is untracked — git has no copy of it. This cannot be undone.</p>`
    : `<p>Discard changes to ${name}?</p>
       <p class="ws-delete-hint">The working copy goes back to the staged or committed version. This cannot be undone.</p>`;
  showConfirm({
    bodyHtml,
    actions: [
      {
        label: untracked ? "Delete file" : "Discard changes",
        kind: "danger",
        onSelect: () => {
          void (async () => {
            try {
              await ipc.gitDiscardFile(wsIdx, file.path, untracked);
              toast(untracked ? `Deleted ${file.path}` : `Discarded changes to ${file.path}`, "success");
            } catch (err) {
              reportError(untracked ? "Delete failed" : "Discard failed", err);
            }
            await onDone();
          })();
        },
      },
      { label: "Cancel", kind: "secondary", isDefault: true },
    ],
  });
}
