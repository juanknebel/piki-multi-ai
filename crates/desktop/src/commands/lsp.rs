use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use crate::lsp::registry::LspRegistry;
use crate::lsp::server::{LspConnectionInfo, LspManager, LspServerStatusInfo};
use crate::state::DesktopApp;

type LspState = Arc<tokio::sync::Mutex<LspManager>>;

#[derive(Serialize, Clone)]
pub struct LspConfigResponse {
    pub servers: Vec<crate::lsp::registry::LspServerConfig>,
    pub idle_ttl_secs: u64,
    pub max_concurrent: usize,
}

/// Start (or reactivate) an LSP server for a file.
/// Returns connection info for the frontend to establish a WebSocket.
#[tauri::command]
pub async fn lsp_ensure_server(
    state: State<'_, Mutex<DesktopApp>>,
    lsp_state: State<'_, LspState>,
    workspace_idx: usize,
    file_path: String,
) -> Result<Option<LspConnectionInfo>, String> {
    let root_path = {
        let app = state.lock();
        let ws = app
            .workspaces
            .get(workspace_idx)
            .ok_or("Invalid workspace index")?;
        PathBuf::from(&ws.info.path)
    };

    let mut manager = lsp_state.lock().await;
    manager
        .ensure_server(&file_path, &root_path)
        .await
        .map_err(|e| e.to_string())
}

/// Notify that a workspace gained or lost focus.
#[tauri::command]
pub async fn lsp_notify_workspace_focus(
    state: State<'_, Mutex<DesktopApp>>,
    lsp_state: State<'_, LspState>,
    workspace_idx: usize,
    focused: bool,
) -> Result<(), String> {
    let root_path = {
        let app = state.lock();
        let ws = app
            .workspaces
            .get(workspace_idx)
            .ok_or("Invalid workspace index")?;
        PathBuf::from(&ws.info.path)
    };

    let mut manager = lsp_state.lock().await;
    if focused {
        manager.mark_active_by_root(&root_path);
    } else {
        manager.mark_idle_by_root(&root_path);
    }
    Ok(())
}

/// Get the status of all running LSP servers.
#[tauri::command]
pub async fn lsp_server_status(
    lsp_state: State<'_, LspState>,
) -> Result<Vec<LspServerStatusInfo>, String> {
    let manager = lsp_state.lock().await;
    Ok(manager.status_all())
}

/// Force-stop a specific server.
#[tauri::command]
pub async fn lsp_stop_server(
    lsp_state: State<'_, LspState>,
    server_id: String,
    root_path: String,
) -> Result<(), String> {
    let key = crate::lsp::server::LspServerKey {
        server_id,
        root_path: PathBuf::from(root_path),
    };
    let mut manager = lsp_state.lock().await;
    manager.shutdown_server(&key).await;
    Ok(())
}

/// Get LSP registry configuration.
#[tauri::command]
pub async fn lsp_get_config(
    lsp_state: State<'_, LspState>,
) -> Result<LspConfigResponse, String> {
    let manager = lsp_state.lock().await;
    Ok(LspConfigResponse {
        servers: manager.registry.servers.clone(),
        idle_ttl_secs: manager.registry.idle_ttl_secs,
        max_concurrent: manager.registry.max_concurrent,
    })
}

/// Update LSP registry configuration.
#[tauri::command]
pub async fn lsp_set_config(
    state: State<'_, Mutex<DesktopApp>>,
    lsp_state: State<'_, LspState>,
    config: LspRegistry,
) -> Result<(), String> {
    let lsp_config_path = {
        let app = state.lock();
        app.paths.config_dir().join("lsp.toml")
    };

    config.save(&lsp_config_path).map_err(|e| e.to_string())?;

    let mut manager = lsp_state.lock().await;
    manager.registry = config;
    Ok(())
}
