use parking_lot::Mutex;
use tauri::State;

use crate::state::DesktopApp;

#[tauri::command]
pub async fn get_sysinfo(state: State<'_, Mutex<DesktopApp>>) -> Result<String, String> {
    let app = state.lock();
    Ok(app.sysinfo.lock().clone())
}
