use parking_lot::Mutex;
use tauri::State;

use crate::state::DesktopApp;

#[tauri::command]
pub async fn get_settings(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Option<String>, String> {
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
