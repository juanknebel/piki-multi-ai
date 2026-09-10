// The app-wide settings store, bound to the Tauri backend. Kept in its own
// module (not in settings-store.ts) so the store logic stays free of IPC
// imports and can be unit-tested with a fake backend.
import * as ipc from "./ipc";
import { createSettingsStore } from "./settings-store";

export const settingsStore = createSettingsStore({
  read: () => ipc.getSettings(),
  write: (json) => ipc.setSettings(json),
});
