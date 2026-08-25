import * as ipc from "../../ipc";
import { appState } from "../../state";
import { reportError } from "../toast";
import { showConfirm, escapeHtml as escapeConfirmHtml } from "../confirm";
import { makeInteractive } from "../a11y";
import { icon, type IconName } from "../icons";
import { createDropdown } from "../dropdown";
import type { SessionRow, SessionsSnapshot } from "../../types";

/// Sessions dialog: every session the persistent-session daemon holds —
/// including ones no tab in this window shows (opened in the TUI, or orphaned
/// after a workspace closed). Mirrors the TUI's `prefix ctrl-s` overlay. The
/// daemon is shared, so this is the desktop's window onto the same state the
/// `sessions` CLI reports.
export async function showSessionsDialog() {
  document.querySelector(".sessions-dialog-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop sessions-dialog-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog ui-surface";
  dialog.style.maxWidth = "720px";
  dialog.style.maxHeight = "80vh";
  dialog.style.width = "88vw";

  dialog.innerHTML = `
    <div class="ui-header">
      <span class="ui-header-title" id="sessions-title">Sessions</span>
      <span style="display:flex;gap:6px;align-items:center">
        <button data-variant="secondary" data-size="sm" class="ui-btn" id="sessions-refresh" title="Refresh">Refresh</button>
        <button data-variant="ghost" data-icon class="dialog-close ui-btn" title="Close" aria-label="Close">×</button>
      </span>
    </div>
    <div id="sessions-body" style="flex:1;overflow-y:auto;padding:4px 0;font-size:12px"></div>
    <div class="dialog-hint" style="padding:6px 12px;color:var(--text-muted);font-size:11px">
      Click an attached session to jump to its tab · Adopt opens a detached one as a tab · these survive quitting the app · manage from the CLI with <code>piki-multi-ai sessions</code>
    </div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const titleEl = dialog.querySelector<HTMLElement>("#sessions-title")!;
  const body = dialog.querySelector<HTMLElement>("#sessions-body")!;

  function stateBadge(row: SessionRow): { icon: IconName; text: string; color: string } {
    switch (row.state) {
      case "attached":
        return {
          icon: "play",
          text: `attached${row.attached > 1 ? ` ×${row.attached}` : ""}`,
          color: "var(--git-added)",
        };
      case "detached":
        return {
          icon: "warning",
          text: row.attached > 0 ? `attached ×${row.attached} (elsewhere)` : "detached",
          color: "var(--warning-color)",
        };
      case "exited":
        return {
          icon: "circle",
          text: row.exit_code != null ? `exited ${row.exit_code}` : "exited",
          color: "var(--text-muted)",
        };
    }
  }

  function render(snap: SessionsSnapshot) {
    // Title: daemon state + pid.
    if (!snap.connected) {
      titleEl.innerHTML = `Sessions — daemon ${icon("circle")} not connected`;
    } else if (snap.daemon_pid != null) {
      titleEl.innerHTML = `Sessions — daemon ${icon("dot")} pid ${Number(snap.daemon_pid)}`;
    } else {
      titleEl.innerHTML = `Sessions — daemon ${icon("dot")}`;
    }

    body.innerHTML = "";

    if (!snap.connected) {
      body.innerHTML = `<div style="padding:16px;color:var(--text-muted)">Persistent sessions are disabled or the daemon is unavailable. Tabs run in-process.</div>`;
      return;
    }
    if (snap.error) {
      body.innerHTML = `<div style="padding:16px;color:var(--error-color)">${snap.error}</div>`;
      return;
    }
    if (snap.sessions.length === 0) {
      body.innerHTML = `<div style="padding:16px;color:var(--text-muted)">No sessions.</div>`;
      return;
    }

    for (const row of snap.sessions) {
      const badge = stateBadge(row);
      const el = document.createElement("div");
      el.className = "sessions-row";
      el.style.cssText =
        "display:flex;align-items:center;gap:10px;padding:2px 12px 2px 0;border-bottom:1px solid var(--border-subtle)";

      const jumpable = row.local_workspace_idx != null && row.local_tab_idx != null;

      // The clickable part is its own element so the Kill/Remove buttons
      // are siblings, not interactive content nested inside an interactive
      // row (which confuses screen readers and Enter handling).
      const main = document.createElement("div");
      main.className = "sessions-main";
      main.style.cssText =
        "flex:1;display:flex;align-items:center;gap:10px;min-width:0;padding:4px 12px;border-radius:var(--radius-sm)";
      main.innerHTML = `
        <span style="color:${badge.color};width:1em;text-align:center">${icon(badge.icon)}</span>
        <span style="flex:1;font-weight:600;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(row.name)}</span>
        <span style="width:130px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(row.workspace)}</span>
        <span style="width:150px;color:${badge.color}">${badge.text}</span>
      `;
      if (jumpable) {
        main.style.cursor = "pointer";
        main.title = "Jump to this tab";
        makeInteractive(main);
        main.addEventListener("click", () => {
          close();
          jumpTo(row.local_workspace_idx!, row.local_tab_idx!);
        });
      }
      el.appendChild(main);

      const actions = document.createElement("span");
      actions.className = "sessions-actions";
      actions.style.cssText = "display:flex;gap:4px;flex-shrink:0";
      const killBtn = actionBtn("Kill", "Kill the process (kept as exited)");
      const removeBtn = actionBtn("Remove", "Remove from the daemon");
      if (row.state === "exited") killBtn.disabled = true;
      if (row.state === "detached") {
        const adoptBtn = actionBtn("Adopt", "Open this session as a tab here");
        adoptBtn.dataset.variant = "primary";
        adoptBtn.addEventListener("click", () => void adopt(row));
        actions.append(adoptBtn);
      }
      actions.append(killBtn, removeBtn);
      el.appendChild(actions);

      killBtn.addEventListener("click", () => {
        const doKill = () => act(() => ipc.killSession(row.id));
        if (row.state !== "attached") return doKill();
        // Attached = a tab in this window (or another client) is using it.
        confirmDestructive({
          title: `Kill "${row.name}"?`,
          hint: "The process ends now; the session stays listed as exited.",
          label: "Kill",
          onConfirm: doKill,
        });
      });
      removeBtn.addEventListener("click", () => {
        confirmDestructive({
          title: `Remove "${row.name}" from the daemon?`,
          hint:
            row.state === "exited"
              ? "Its record and scrollback are dropped."
              : "The process is killed and the session is forgotten.",
          label: "Remove",
          onConfirm: () => act(() => ipc.removeSession(row.id)),
        });
      });

      body.appendChild(el);
    }

    // Initial focus: the first row (jumpable or not), else the Refresh
    // button, so keyboard users land inside the dialog.
    if (!dialog.contains(document.activeElement)) {
      const first =
        body.querySelector<HTMLElement>('.sessions-main[tabindex="0"]') ??
        body.querySelector<HTMLElement>("button:not(:disabled)") ??
        dialog.querySelector<HTMLElement>("#sessions-refresh");
      first?.focus();
    }
  }

  function confirmDestructive(opts: { title: string; hint: string; label: string; onConfirm: () => void }) {
    showConfirm({
      bodyHtml: `<p>${escapeConfirmHtml(opts.title)}</p><p class="ws-delete-hint">${escapeConfirmHtml(opts.hint)}</p>`,
      actions: [
        { label: opts.label, kind: "danger", isDefault: true, onSelect: opts.onConfirm },
        { label: "Cancel", kind: "secondary", autofocus: true },
      ],
    });
  }

  /** Adopt a detached session as a tab: into the loaded workspace matching
   *  its recorded path, else ask which one (TUI parity: `attach_orphan`). */
  async function adopt(row: SessionRow) {
    const workspaces = appState.workspaces;
    if (workspaces.length === 0) {
      reportError("Adopt session failed", "No workspace to attach the session to");
      return;
    }
    let target = row.workspace_idx;
    if (target == null) {
      target = workspaces.length === 1 ? 0 : await pickWorkspace(row);
      if (target == null) return;
    }
    try {
      const tabIdx = await ipc.adoptSession(row.id, target);
      close();
      await jumpTo(target, tabIdx);
    } catch (err) {
      reportError("Adopt session failed", err);
      refresh();
    }
  }

  /** Workspace chooser for an orphan whose path matches no loaded workspace. */
  function pickWorkspace(row: SessionRow): Promise<number | null> {
    return new Promise((resolve) => {
      const options = appState.workspaces.map((w, i) => ({ value: String(i), label: w.info.name }));
      const dropdown = createDropdown(options, String(appState.activeWorkspace), "width:100%");
      const { overlay } = showConfirm({
        bodyHtml: `<p>Open "${escapeConfirmHtml(row.name)}" in which workspace?</p><p class="ws-delete-hint">Its own folder (${escapeConfirmHtml(row.workspace)}) is not loaded here.</p>`,
        actions: [
          { label: "Adopt", kind: "primary", isDefault: true, onSelect: () => resolve(Number(dropdown.value)) },
          { label: "Cancel", kind: "secondary", autofocus: true, onSelect: () => resolve(null) },
        ],
        onDismiss: () => resolve(null),
      });
      overlay.querySelector(".ws-delete-buttons")?.before(dropdown.container);
    });
  }

  async function jumpTo(wsIdx: number, tabIdx: number) {
    try {
      if (wsIdx !== appState.activeWorkspace) {
        const detail = await ipc.switchWorkspace(wsIdx);
        appState.setActiveWorkspace(wsIdx, detail);
      }
      appState.setActiveTab(tabIdx);
    } catch (err) {
      reportError("Jump to session failed", err);
    }
  }

  async function refresh() {
    try {
      render(await ipc.listSessions());
    } catch (err) {
      reportError("List sessions failed", err);
    }
  }

  async function act(fn: () => Promise<SessionsSnapshot>) {
    try {
      render(await fn());
    } catch (err) {
      reportError("Session action failed", err);
      refresh();
    }
  }

  const prevFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  function close() {
    backdrop.remove();
    prevFocus?.focus();
  }
  function onKey(e: KeyboardEvent) {
    // A confirm overlay on top owns Escape while it is open.
    if (e.key === "Escape" && !document.querySelector(".ws-delete-confirm")) close();
  }

  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  dialog.querySelector("#sessions-refresh")!.addEventListener("click", refresh);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  backdrop.tabIndex = -1;
  backdrop.addEventListener("keydown", onKey);
  backdrop.focus();

  await refresh();
}

function actionBtn(label: string, title: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "ui-btn";
  b.dataset.variant = "secondary";
  b.dataset.size = "sm";
  b.textContent = label;
  b.title = title;
  return b;
}

function escapeHtml(s: string): string {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}
