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
