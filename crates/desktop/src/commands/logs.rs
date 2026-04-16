use serde::Serialize;
use tauri::State;

use crate::log_buffer::LogBuffer;

#[derive(Serialize, Clone)]
pub struct LogEntryResponse {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[tauri::command]
pub async fn get_logs(
    level_filter: Option<u8>,
    log_buffer: State<'_, LogBuffer>,
) -> Result<Vec<LogEntryResponse>, String> {
    let buf = log_buffer.lock();
    let filter = level_filter.unwrap_or(0);

    let entries: Vec<LogEntryResponse> = buf
        .iter()
        .filter(|entry| {
            if filter == 0 {
                return true;
            }
            let level_num = match entry.level {
                tracing::Level::ERROR => 1,
                tracing::Level::WARN => 2,
                tracing::Level::INFO => 3,
                tracing::Level::DEBUG => 4,
                tracing::Level::TRACE => 5,
            };
            level_num <= filter
        })
        .map(|e| LogEntryResponse {
            timestamp: e.timestamp.clone(),
            level: e.level.to_string().to_uppercase(),
            target: e.target.clone(),
            message: e.message.clone(),
        })
        .collect();

    Ok(entries)
}

#[tauri::command]
pub async fn clear_logs(log_buffer: State<'_, LogBuffer>) -> Result<(), String> {
    log_buffer.lock().clear();
    Ok(())
}
