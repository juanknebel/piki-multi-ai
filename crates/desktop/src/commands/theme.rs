use std::collections::HashMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
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

/// Theme file shape read from `<config_dir>/desktop-themes/*.json`.
///
/// Mirrors the frontend `ThemePreset` type (`crates/desktop/frontend/src/theme.ts`).
/// `colors` is intentionally a free-form map: validation and partial-key
/// fallback happen in the frontend loader, so unknown keys here are simply
/// ignored rather than rejecting the whole file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub is_dark: bool,
    pub colors: HashMap<String, String>,
}

#[tauri::command]
pub async fn list_custom_themes(
    state: State<'_, Mutex<DesktopApp>>,
) -> Result<Vec<CustomTheme>, String> {
    let dir = {
        let app = state.lock();
        app.paths.config_dir().join("desktop-themes")
    };

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("failed to read {}: {err}", dir.display())),
    };

    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<CustomTheme>(&s).map_err(|e| e.to_string()))
        {
            Ok(theme) => themes.push(theme),
            Err(err) => tracing::warn!(
                "skipping invalid desktop theme {}: {err}",
                path.display()
            ),
        }
    }

    themes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(themes)
}

/// Persist an imported theme to `<config>/desktop-themes/<id>.json` so it
/// survives restarts and appears in the preset dropdown alongside built-ins.
/// The id is sanitized to prevent path-traversal: only `[a-zA-Z0-9_-]`,
/// max 64 chars. Empty/invalid ids are rejected.
#[tauri::command]
pub async fn save_custom_theme(
    state: State<'_, Mutex<DesktopApp>>,
    theme: CustomTheme,
) -> Result<(), String> {
    let id = theme.id.trim();
    if id.is_empty() || id.len() > 64 {
        return Err("theme id must be 1..=64 characters".into());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("theme id may only contain letters, digits, '-' and '_'".into());
    }

    let dir = {
        let app = state.lock();
        app.paths.config_dir().join("desktop-themes")
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create {}: {e}", dir.display()))?;

    let path = dir.join(format!("{id}.json"));
    let json = serde_json::to_string_pretty(&theme).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}
