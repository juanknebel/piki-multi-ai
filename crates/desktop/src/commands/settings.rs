//! Settings persistence. Two documents live in `UiPrefsStorage`:
//!
//! - `settings` — the desktop-only JSON blob owned by the frontend's
//!   `settings-store.ts` (shortcuts, zoom, density, terminal look, …).
//!   Opaque here; `commands/pty.rs` only reads `shell` out of it.
//! - `app_settings` — `piki_core::app_settings::AppSettings`, the choices
//!   BOTH frontends honour (persistent sessions, notification delivery,
//!   sound). DB override > `config.toml` > default; see that module.

use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use piki_core::app_settings::{AppSettings, EffectiveSettings};

use crate::state::DesktopApp;

#[tauri::command]
pub async fn get_settings(state: State<'_, Mutex<DesktopApp>>) -> Result<Option<String>, String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_ref()
        .ok_or("Storage not available")?;
    prefs
        .get_preference("settings")
        .map_err(|e| format!("Failed to get settings: {e}"))
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, Mutex<DesktopApp>>,
    value: String,
) -> Result<(), String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_ref()
        .ok_or("Storage not available")?;
    prefs
        .set_preference("settings", &value)
        .map_err(|e| format!("Failed to save settings: {e}"))
}

/// What Settings ▸ General renders: the three layers (so the dialog can say
/// "from config.toml" vs "chosen here") plus the sessions state this process
/// actually runs with. Mirrored by `AppSettingsView` in `frontend/src/types.ts`.
#[derive(Serialize)]
pub struct AppSettingsView {
    #[serde(flatten)]
    pub effective: EffectiveSettings,
    /// `[sessions] enabled` the running process started with — a change in
    /// `effective.sessions_enabled` applies on the next launch.
    pub runtime_sessions_enabled: bool,
}

fn view(app: &DesktopApp) -> AppSettingsView {
    AppSettingsView {
        effective: piki_core::app_settings::resolve(
            &app.paths.config_path(),
            app.storage.ui_prefs.as_deref(),
        ),
        runtime_sessions_enabled: app.sessions_enabled,
    }
}

#[tauri::command]
pub async fn get_app_settings(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<AppSettingsView, String> {
    let app = state.lock();
    Ok(view(&app))
}

/// Store the overrides (the whole document — a `None` field means "back to
/// config.toml" for that key) and apply what can apply live: the
/// notification delivery + sound layer switches immediately for both this
/// window and its background agents. The sessions choice is only read at
/// startup (connecting the daemon and re-attaching is startup work), which
/// the dialog says. Returns the merged view so the UI re-renders from truth.
#[tauri::command]
pub async fn set_app_settings(
    state: State<'_, Mutex<DesktopApp>>,
    overrides: AppSettings,
) -> Result<AppSettingsView, String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_deref()
        .ok_or("Storage not available")?;
    overrides
        .save(prefs)
        .map_err(|e| format!("Failed to save settings: {e}"))?;
    let v = view(&app);
    v.effective.notifications.apply();
    Ok(v)
}
