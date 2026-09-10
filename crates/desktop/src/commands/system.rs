use parking_lot::Mutex;
use tauri::State;

use piki_core::sysinfo::SysInfoSnapshot;

use crate::state::DesktopApp;

#[tauri::command]
pub async fn get_sysinfo(state: State<'_, Mutex<DesktopApp>>) -> Result<String, String> {
    let app = state.lock();
    Ok(app.sysinfo.lock().clone())
}

#[tauri::command]
pub async fn get_sysinfo_detailed() -> Result<SysInfoSnapshot, String> {
    tokio::task::spawn_blocking(piki_core::sysinfo::sample_snapshot)
        .await
        .map_err(|e| e.to_string())
}

/// Open `url` in the user's default browser (terminal links: Ctrl+click and
/// the terminal menu's "Open link"). Only `http(s)` — the shell plugin's own
/// open-scope validates too, but refusing here first keeps a `file:` or
/// `javascript:` string that a program printed from ever reaching it.
///
/// `Shell::open` is deprecated upstream in favour of `tauri-plugin-opener`;
/// the shell plugin is already a dependency, so we stay on it until the app
/// migrates as a whole — this is the only call site.
#[allow(deprecated)]
#[tauri::command]
pub async fn open_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;

    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(format!("Refusing to open a non-http URL: {trimmed}"));
    }
    app.shell().open(trimmed, None).map_err(|e| e.to_string())
}
