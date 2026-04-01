use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use piki_core::ChangedFile;

#[allow(dead_code)]
#[derive(Serialize, Clone)]
pub struct GitRefreshPayload {
    pub workspace_idx: usize,
    pub files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
}

#[derive(Serialize, Clone)]
pub struct SysinfoPayload {
    pub formatted: String,
}

#[allow(dead_code)]
#[derive(Serialize, Clone)]
pub struct ToastPayload {
    pub message: String,
    pub level: String,
}

/// Spawn a background task that emits sysinfo updates every 3 seconds.
pub fn spawn_sysinfo_emitter(app_handle: AppHandle, sysinfo: Arc<Mutex<String>>) {
    tauri::async_runtime::spawn(async move {
        let mut last = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let current = sysinfo.lock().clone();
            if current != last {
                let _ = app_handle.emit(
                    "sysinfo-update",
                    SysinfoPayload {
                        formatted: current.clone(),
                    },
                );
                last = current;
            }
        }
    });
}

/// Emit a toast notification.
#[allow(dead_code)]
pub fn emit_toast(app_handle: &AppHandle, message: &str, level: &str) {
    let _ = app_handle.emit(
        "toast",
        ToastPayload {
            message: message.to_string(),
            level: level.to_string(),
        },
    );
}
