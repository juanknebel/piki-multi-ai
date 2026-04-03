use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use piki_core::ChangedFile;

use crate::state::DesktopApp;

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

/// Hybrid git status refresh: file watcher for instant feedback on file
/// edits, plus a periodic poll (every 2s) to catch commits, branch switches,
/// and other git-internal operations that the watcher misses (`.git/` is
/// excluded from watching).  A background `git fetch` every 60s keeps
/// ahead/behind counts up to date with the remote.
pub fn spawn_git_watcher(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick: u32 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            tick = tick.wrapping_add(1);

            let state = app_handle.state::<Mutex<DesktopApp>>();

            // Drain file watcher events — instant trigger for file edits
            let watcher_triggered: Vec<(usize, PathBuf)> = {
                let mut app = state.lock();
                app.workspaces
                    .iter_mut()
                    .enumerate()
                    .filter_map(|(idx, ws)| {
                        let watcher = ws.watcher.as_mut()?;
                        if watcher.drain().is_empty() {
                            None
                        } else {
                            Some((idx, ws.info.path.clone()))
                        }
                    })
                    .collect()
            };

            // Every 120th tick (~60s): git fetch for all workspaces
            if tick.is_multiple_of(120) {
                let paths: Vec<PathBuf> = {
                    let app = state.lock();
                    app.workspaces.iter().map(|ws| ws.info.path.clone()).collect()
                };
                for path in paths {
                    let _ = tokio::process::Command::new("git")
                        .args(["fetch", "--quiet"])
                        .current_dir(&path)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .await;
                }
            }

            // Every 4th tick (~2s): poll ALL workspaces for git-internal changes
            let poll_all = tick.is_multiple_of(4);

            let to_refresh: Vec<(usize, PathBuf)> = if poll_all {
                let app = state.lock();
                app.workspaces
                    .iter()
                    .enumerate()
                    .map(|(idx, ws)| (idx, ws.info.path.clone()))
                    .collect()
            } else {
                watcher_triggered
            };

            for (idx, path) in to_refresh {
                let Ok(files) = piki_core::git::get_changed_files(&path).await
                else {
                    continue;
                };
                let ahead_behind = piki_core::git::get_ahead_behind(&path).await;

                let changed = {
                    let app = state.lock();
                    if idx >= app.workspaces.len() {
                        continue;
                    }
                    let ws = &app.workspaces[idx];
                    ws.changed_files != files || ws.ahead_behind != ahead_behind
                };

                if changed {
                    {
                        let mut app = state.lock();
                        if idx < app.workspaces.len() {
                            app.workspaces[idx].changed_files = files.clone();
                            app.workspaces[idx].ahead_behind = ahead_behind;
                        }
                    }
                    let _ = app_handle.emit(
                        "git-refresh",
                        GitRefreshPayload {
                            workspace_idx: idx,
                            files,
                            ahead_behind,
                        },
                    );
                }
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
