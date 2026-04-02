use parking_lot::Mutex;
use tauri::State;

use crate::state::DesktopApp;

#[tauri::command]
pub async fn get_theme(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<(Option<String>, Option<String>), String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_ref()
        .ok_or("UI prefs not available")?;
    let preset = prefs
        .get_preference("theme")
        .map_err(|e| e.to_string())?;
    let overrides = prefs
        .get_preference("theme_overrides")
        .map_err(|e| e.to_string())?;
    Ok((preset, overrides))
}

#[tauri::command]
pub async fn set_theme(
    state: State<'_, Mutex<DesktopApp>>,
    preset: String,
    overrides: String,
) -> Result<(), String> {
    let app = state.lock();
    let prefs = app
        .storage
        .ui_prefs
        .as_ref()
        .ok_or("UI prefs not available")?;
    prefs
        .set_preference("theme", &preset)
        .map_err(|e| e.to_string())?;
    prefs
        .set_preference("theme_overrides", &overrides)
        .map_err(|e| e.to_string())?;
    Ok(())
}
