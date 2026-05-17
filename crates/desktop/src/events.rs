use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use piki_core::ChangedFile;
use piki_core::notifications;

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct GitRefreshPayload {
    pub workspace_idx: usize,
    pub files: Vec<ChangedFile>,
    pub ahead_behind: Option<(usize, usize)>,
}

#[derive(Serialize, Clone)]
pub struct FileChangedPayload {
    pub workspace_idx: usize,
    /// Paths relative to the workspace root that the watcher reported as changed.
    pub paths: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct SysinfoPayload {
    pub formatted: String,
}

/// Tauri event payload emitted when a tab needs the user's attention.
/// Sources today:
/// - `provider-idle`: a provider tab's PTY went silent past its threshold.
/// - `shell-command-end`: a shell tab finished a command (forwarded from the
///   per-PTY OSC parser; carries `exit_code`).
#[derive(Serialize, Clone)]
pub struct PtyAttentionPayload {
    pub workspace_idx: usize,
    pub tab_id: String,
    pub source: &'static str,
}

#[allow(dead_code)]
#[derive(Serialize, Clone)]
pub struct ToastPayload {
    pub message: String,
    pub level: String,
}

/// Background loop polling every 250 ms over all provider tabs, ticking each
/// tab's `IdleWatcher` against its current PTY byte count. When a watcher
/// fires, emits a `pty-attention` Tauri event for the frontend (sidebar
/// badge) and spawns a `notify-rust` notification regardless of whether the
/// workspace is the active one.
pub fn spawn_idle_watcher_loop(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            interval.tick().await;
            let Some(state) = app_handle.try_state::<Mutex<DesktopApp>>() else {
                continue;
            };
            let mut events: Vec<PtyAttentionPayload> = Vec::new();
            // (origin, workspace_name, provider_label, silent_for, icon) — origin
            // is the tab UUID (globally unique within the desktop process).
            let mut pending_idle: Vec<(
                String,
                String,
                String,
                std::time::Duration,
                Option<String>,
            )> = Vec::new();
            {
                let mut app = state.lock();
                // Snapshot the icon map up-front — we can't borrow
                // `app.provider_manager` immutably while `app.workspaces` is
                // borrowed mutably for iteration.
                let icons: std::collections::HashMap<String, String> = app
                    .provider_manager
                    .all()
                    .iter()
                    .filter_map(|c| c.icon.clone().map(|i| (c.name.clone(), i)))
                    .collect();
                for (ws_idx, ws) in app.workspaces.iter_mut().enumerate() {
                    let ws_name = ws.info.name.clone();
                    for tab in &mut ws.tabs {
                        let Some(ref pty) = tab.pty else { continue };
                        let Some(ref mut watcher) = tab.idle_watcher else {
                            continue;
                        };
                        if !pty.peek_alive() {
                            continue;
                        }
                        // Structured cli-agent channel live for this tab → it
                        // owns attention (precise Stop/Notification); skip the
                        // byte-silence heuristic so it can't double-fire. No
                        // events (missing / version-skewed hooks) → `cli_agent`
                        // stays `None` and the watcher is the graceful
                        // fallback.
                        if pty
                            .shell()
                            .is_some_and(|s| s.lock().state.cli_agent.is_some())
                        {
                            continue;
                        }
                        if let Some(sig) = watcher.poll(pty.bytes_processed()) {
                            events.push(PtyAttentionPayload {
                                workspace_idx: ws_idx,
                                tab_id: tab.id.clone(),
                                source: "provider-idle",
                            });
                            let provider_label = tab.provider.label().to_string();
                            let icon = icons.get(&provider_label).cloned();
                            pending_idle.push((
                                tab.id.clone(),
                                ws_name.clone(),
                                provider_label,
                                sig.silent_for,
                                icon,
                            ));
                        }
                    }
                }
            }
            for ev in events {
                let _ = app_handle.emit("pty-attention", ev);
            }
            for (origin, ws_name, provider_label, silent_for, icon) in pending_idle {
                notifications::notify_agent_idle(
                    &origin,
                    &ws_name,
                    &provider_label,
                    silent_for,
                    icon.as_deref(),
                );
            }
        }
    });
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

            // Drain file watcher events — instant trigger for file edits.
            // Also collect the per-path changes so we can emit `file-changed` events
            // for editor reload-on-external-change.
            let (watcher_triggered, file_change_payloads): (Vec<(usize, PathBuf)>, Vec<FileChangedPayload>) = {
                let mut app = state.lock();
                let mut triggered = Vec::new();
                let mut payloads = Vec::new();
                for (idx, ws) in app.workspaces.iter_mut().enumerate() {
                    let Some(watcher) = ws.watcher.as_mut() else { continue };
                    let events = watcher.drain();
                    if events.is_empty() {
                        continue;
                    }
                    triggered.push((idx, ws.info.path.clone()));

                    // Collect changed paths relative to the workspace, deduplicated.
                    use std::collections::BTreeSet;
                    let mut rel_paths = BTreeSet::new();
                    for ev in events {
                        for abs in ev.paths {
                            let rel = abs
                                .strip_prefix(&ws.info.path)
                                .unwrap_or(&abs)
                                .to_string_lossy()
                                .to_string();
                            if !rel.is_empty() {
                                rel_paths.insert(rel);
                            }
                        }
                    }
                    if !rel_paths.is_empty() {
                        payloads.push(FileChangedPayload {
                            workspace_idx: idx,
                            paths: rel_paths.into_iter().collect(),
                        });
                    }
                }
                (triggered, payloads)
            };

            for payload in file_change_payloads {
                let _ = app_handle.emit("file-changed", payload);
            }

            // Every 120th tick (~60s): git fetch for all workspaces
            if tick.is_multiple_of(120) {
                let paths: Vec<PathBuf> = {
                    let app = state.lock();
                    app.workspaces.iter().map(|ws| ws.info.path.clone()).collect()
                };
                for path in paths {
                    let _ = piki_core::shell_env::command("git")
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
