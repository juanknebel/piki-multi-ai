import * as ipc from "../../ipc";
import { appState } from "../../state";
import { reportError } from "../toast";
import { makeInteractive } from "../a11y";
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
  dialog.className = "dialog";
  dialog.style.maxWidth = "720px";
  dialog.style.maxHeight = "80vh";
  dialog.style.width = "88vw";

  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title" id="sessions-title">Sessions</span>
      <span style="display:flex;gap:6px;align-items:center">
        <button class="dialog-btn dialog-btn-secondary dialog-btn-sm" id="sessions-refresh" title="Refresh">Refresh</button>
        <button class="dialog-close" title="Close" aria-label="Close">×</button>
      </span>
    </div>
    <div id="sessions-body" style="flex:1;overflow-y:auto;padding:4px 0;font-size:12px"></div>
    <div class="dialog-hint" style="padding:6px 12px;color:var(--text-muted);font-size:11px">
      Click an attached session to jump to its tab · these survive quitting the app · manage from the CLI with <code>piki-multi-ai sessions</code>
    </div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const titleEl = dialog.querySelector<HTMLElement>("#sessions-title")!;
  const body = dialog.querySelector<HTMLElement>("#sessions-body")!;

  function stateBadge(row: SessionRow): { glyph: string; text: string; color: string } {
    switch (row.state) {
      case "attached":
        return {
          glyph: "▷",
          text: `attached${row.attached > 1 ? ` ×${row.attached}` : ""}`,
          color: "var(--git-added)",
        };
      case "detached":
        return {
          glyph: "⚠",
          text: row.attached > 0 ? `attached ×${row.attached} (elsewhere)` : "detached",
          color: "var(--warning-color)",
        };
      case "exited":
        return {
          glyph: "○",
          text: row.exit_code != null ? `exited ${row.exit_code}` : "exited",
          color: "var(--text-muted)",
        };
    }
  }

  function render(snap: SessionsSnapshot) {
    // Title: daemon state + pid.
    if (!snap.connected) {
      titleEl.textContent = "Sessions — daemon ○ not connected";
    } else if (snap.daemon_pid != null) {
      titleEl.textContent = `Sessions — daemon ● pid ${snap.daemon_pid}`;
    } else {
      titleEl.textContent = "Sessions — daemon ●";
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
        "display:flex;align-items:center;gap:10px;padding:6px 12px;border-bottom:1px solid var(--border-subtle)";

      const jumpable = row.local_workspace_idx != null && row.local_tab_idx != null;
      if (jumpable) el.style.cursor = "pointer";

      el.innerHTML = `
        <span style="color:${badge.color};width:1em;text-align:center">${badge.glyph}</span>
        <span style="flex:1;font-weight:600;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(row.name)}</span>
        <span style="width:130px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(row.workspace)}</span>
        <span style="width:150px;color:${badge.color}">${badge.text}</span>
        <span class="sessions-actions" style="display:flex;gap:4px"></span>
      `;

      const actions = el.querySelector<HTMLElement>(".sessions-actions")!;
      const killBtn = actionBtn("Kill", "Kill the process (kept as exited)");
      const removeBtn = actionBtn("Remove", "Remove from the daemon");
      if (row.state === "exited") killBtn.disabled = true;
      actions.append(killBtn, removeBtn);

      killBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        act(() => ipc.killSession(row.id));
      });
      removeBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        act(() => ipc.removeSession(row.id));
      });

      if (jumpable) {
        makeInteractive(el);
        const jump = () => {
          close();
          jumpTo(row.local_workspace_idx!, row.local_tab_idx!);
        };
        el.addEventListener("click", jump);
        el.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter") jump();
        });
      }

      body.appendChild(el);
    }
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

  function close() {
    document.removeEventListener("keydown", onKey);
    backdrop.remove();
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  dialog.querySelector("#sessions-refresh")!.addEventListener("click", refresh);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });
  document.addEventListener("keydown", onKey);

  await refresh();
}

function actionBtn(label: string, title: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "dialog-btn dialog-btn-secondary dialog-btn-sm";
  b.textContent = label;
  b.title = title;
  return b;
}

function escapeHtml(s: string): string {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}
