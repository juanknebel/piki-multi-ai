// Settings ▸ General — the choices BOTH frontends honour (persistent
// sessions, notification delivery, sound). They live in the shared piki
// database (`piki_core::app_settings`, DB override > config.toml > default):
// the backend returns the merged view with the two lower layers, so each
// control can say whether it follows config.toml or was set here. Sessions
// apply on the next launch (connecting the daemon + re-attaching is startup
// work) and the tab says so; notifications switch live.

import * as ipc from "../../ipc";
import type { AppSettings, AppSettingsView, NotificationDelivery } from "../../types";
import { createDropdown } from "../dropdown";
import { reportError, toast } from "../toast";
import { settingsCheckbox, settingsGrid, settingsHint, settingsSection, sourceBadge, type SettingsSection } from "./settings-controls";

const DELIVERY_OPTIONS: { value: NotificationDelivery; label: string }[] = [
  { value: "system", label: "System notification" },
  { value: "terminal", label: "Host terminal (OSC 9 — for the TUI in tmux/ssh)" },
  { value: "off", label: "Off" },
];

function deliveryValue(s: string): NotificationDelivery {
  return DELIVERY_OPTIONS.some((o) => o.value === s) ? (s as NotificationDelivery) : "system";
}

export function buildGeneralSettingsSection(): SettingsSection {
  const el = document.createElement("div");
  el.className = "settings-tab-general";

  const loading = document.createElement("div");
  loading.className = "ui-empty";
  loading.dataset.tone = "loading";
  loading.textContent = "Loading…";
  el.appendChild(loading);

  let view: AppSettingsView | null = null;
  let firstControl: HTMLElement | null = null;

  const save = async (patch: AppSettings) => {
    if (!view) return;
    const overrides: AppSettings = { ...view.overrides, ...patch };
    try {
      render(await ipc.setAppSettings(overrides));
    } catch (err) {
      reportError("Save settings", err);
      render(view); // put the controls back to what is actually stored
    }
  };

  const render = (v: AppSettingsView) => {
    view = v;
    el.replaceChildren();

    // ── Persistent sessions ──
    const sessions = settingsSection("Persistent sessions");
    const { row: sRow } = settingsGrid(sessions);
    const sessionsCheck = settingsCheckbox(
      "Keep terminal tabs running in a background daemon (they survive quitting the app)",
      v.sessions_enabled,
      (on) => void save({ sessions_enabled: on }),
    );
    const sessionsCell = sRow("Sessions", sessionsCheck.label);
    sessionsCell.appendChild(sourceBadge(v.overrides.sessions_enabled != null));
    firstControl = sessionsCheck.input;

    const running = v.runtime_sessions_enabled ? "on" : "off";
    const pending = v.sessions_enabled !== v.runtime_sessions_enabled;
    const state = settingsHint(
      pending
        ? `Currently ${running} — will be ${v.sessions_enabled ? "on" : "off"} after you restart piki. Open tabs are not affected until then.`
        : `Currently ${running}. Takes effect on the next launch (the daemon is connected at startup); the status bar shows the live state.`,
    );
    if (pending) state.dataset.tone = "pending";
    sessions.appendChild(state);
    el.appendChild(sessions);

    // ── Notifications ──
    const notif = settingsSection("Notifications");
    const { row: nRow } = settingsGrid(notif);
    const delivery = createDropdown(DELIVERY_OPTIONS, deliveryValue(v.notifications.delivery));
    delivery.container.addEventListener("change", () =>
      void save({ notification_delivery: deliveryValue(delivery.value) }),
    );
    const dCell = nRow("Delivery", delivery.container);
    dCell.appendChild(sourceBadge(v.overrides.notification_delivery != null));

    const sound = settingsCheckbox(
      "Chime when an agent finishes or needs you",
      v.notifications.sound,
      (on) => void save({ sound: on }),
    );
    const soundCell = nRow("Sound", sound.label);
    soundCell.appendChild(sourceBadge(v.overrides.sound != null));
    notif.appendChild(
      settingsHint(
        "Applies immediately, to agent events in tabs you are not looking at. Custom sound files stay in config.toml (sound_path, sound_done_path, sound_attention_path).",
      ),
    );
    el.appendChild(notif);

    // ── Where these live ──
    const about = settingsSection("Shared with the TUI");
    const cfg = v.config_notifications;
    about.appendChild(
      settingsHint(
        `These three override [sessions] and [notifications] in config.toml and are stored in the piki database, so the TUI follows them too. config.toml currently says: sessions ${v.config_sessions_enabled ? "on" : "off"}, delivery "${cfg.delivery}", sound ${cfg.sound ? "on" : "off"}. Reset this tab to follow config.toml again.`,
      ),
    );
    el.appendChild(about);
  };

  const load = async () => {
    try {
      render(await ipc.getAppSettings());
    } catch (err) {
      reportError("Load settings", err);
      el.replaceChildren();
      const failed = document.createElement("div");
      failed.className = "ui-empty";
      failed.dataset.tone = "error";
      failed.innerHTML = `<p class="ui-empty-title">Could not load settings</p><p class="ui-empty-hint"></p>`;
      failed.querySelector(".ui-empty-hint")!.textContent = String(err);
      el.appendChild(failed);
    }
  };
  void load();

  return {
    el,
    async reset() {
      if (!view) return;
      try {
        render(await ipc.setAppSettings({}));
        toast("General settings follow config.toml again", "info");
      } catch (err) {
        reportError("Reset settings", err);
      }
    },
    focus() {
      firstControl?.focus();
    },
  };
}
