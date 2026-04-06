// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Many items are prepared for Phase 2/3 features
#![allow(dead_code)]

mod commands;
mod events;
mod log_buffer;
mod pty_raw;
mod state;

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::Manager;
use tracing_subscriber::prelude::*;

use piki_core::WorkspaceStatus;
use piki_core::paths::DataPaths;
use piki_core::storage::create_storage;
use piki_core::workspace::manager::WorkspaceManager;
use piki_core::workspace::watcher::FileWatcher;

use state::{DesktopApp, DesktopWorkspace};

fn main() {
    // Parse --data-dir flag before Tauri takes over args
    let data_dir = parse_data_dir();

    // Initialize tracing with in-memory log buffer + stderr output
    let log_buf = log_buffer::new_buffer();
    let memory_layer = log_buffer::MemoryLayer::new(Arc::clone(&log_buf));

    let fmt_layer = tracing_subscriber::fmt::layer().with_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    );

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(memory_layer)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(log_buf)
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Initialize paths and storage
            let paths = match data_dir {
                Some(ref dir) => DataPaths::new(dir.into()),
                None => DataPaths::default_paths(),
            };
            let storage = create_storage(&paths).expect("Failed to initialize storage");
            let storage = Arc::new(storage);
            let manager = WorkspaceManager::with_paths(paths.clone());

            // Load existing workspaces from storage
            let entries = storage.workspaces.load_all_workspaces();
            let workspaces: Vec<DesktopWorkspace> = entries
                .into_iter()
                .map(|entry| {
                    let info = entry.into_info();
                    let watcher =
                        FileWatcher::new(info.path.clone(), info.name.clone()).ok();
                    DesktopWorkspace {
                        info,
                        status: WorkspaceStatus::Idle,
                        changed_files: Vec::new(),
                        ahead_behind: None,
                        tabs: Vec::new(),
                        active_tab: 0,
                        watcher,
                    }
                })
                .collect();

            tracing::info!(count = workspaces.len(), "Loaded workspaces");

            // Initialize sysinfo with an empty string — the background task will fill it.
            // We cannot call spawn_sysinfo_poller() here because Tauri's setup() runs
            // outside the tokio runtime context. Instead, we start our own updater.
            let sysinfo = Arc::new(parking_lot::Mutex::new(String::new()));
            let sysinfo_for_updater = Arc::clone(&sysinfo);
            let sysinfo_for_emitter = Arc::clone(&sysinfo);

            // Spawn a background sysinfo updater on Tauri's async runtime
            tauri::async_runtime::spawn(async move {
                loop {
                    let snapshot =
                        tokio::task::spawn_blocking(piki_core::sysinfo::sample_formatted)
                            .await
                            .unwrap_or_default();
                    *sysinfo_for_updater.lock() = snapshot;
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            });

            // Create app state
            let desktop_app = DesktopApp {
                workspaces,
                active_workspace: 0,
                storage,
                paths,
                manager,
                sysinfo,
            };

            // Start sysinfo event emitter
            events::spawn_sysinfo_emitter(app_handle.clone(), sysinfo_for_emitter);

            // Start file watcher poller for git auto-refresh
            events::spawn_git_watcher(app_handle);

            app.manage(Mutex::new(desktop_app));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace::list_workspaces,
            commands::workspace::switch_workspace,
            commands::workspace::create_workspace,
            commands::workspace::delete_workspace,
            commands::workspace::update_workspace,
            commands::pty::spawn_tab,
            commands::pty::write_pty,
            commands::pty::resize_pty,
            commands::pty::close_tab,
            commands::pty::spawn_editor_tab,
            commands::git::get_changed_files,
            commands::git::git_stage,
            commands::git::git_unstage,
            commands::git::git_commit,
            commands::git::git_push,
            commands::git::git_merge,
            commands::git::git_abort_merge,
            commands::git::git_resolve_conflict,
            commands::git::git_continue_merge,
            commands::git::git_stage_all,
            commands::git::git_unstage_all,
            commands::diff::get_file_diff,
            commands::diff::get_commit_diff,
            commands::diff::get_side_by_side_diff,
            commands::diff::get_commit_side_by_side_diff,
            commands::diff::get_conflict_diff,
            commands::gitlog::get_git_log,
            commands::stash::git_stash_list,
            commands::stash::git_stash_save,
            commands::stash::git_stash_pop,
            commands::stash::git_stash_apply,
            commands::stash::git_stash_drop,
            commands::search::fuzzy_file_list,
            commands::search::read_file_content,
            commands::search::write_file_content,
            commands::search::project_search,
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::agents::list_agents,
            commands::agents::save_agent,
            commands::agents::delete_agent,
            commands::agents::scan_repo_agents,
            commands::agents::import_agents,
            commands::agents::dispatch_agent,
            commands::review::get_pr_info,
            commands::review::get_pr_file_diff,
            commands::review::submit_pr_review,
            commands::markdown::read_markdown_file,
            commands::system::get_sysinfo,
            commands::theme::get_theme,
            commands::theme::set_theme,
            commands::logs::get_logs,
            commands::logs::clear_logs,
            commands::kanban::kanban_load_board,
            commands::kanban::kanban_create_card,
            commands::kanban::kanban_update_card,
            commands::kanban::kanban_move_card,
            commands::kanban::kanban_delete_card,
            commands::kanban::kanban_load_board_by_path,
            commands::kanban::kanban_move_card_by_path,
            commands::api::send_api_request,
            commands::api::load_api_history,
            commands::api::search_api_history,
            commands::api::delete_api_history_entry,
            commands::api::jq_filter,
        ])
        .run(tauri::generate_context!())
        .expect("error while running piki-desktop");
}

/// Parse `--data-dir <PATH>` from CLI args (before Tauri consumes them).
fn parse_data_dir() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for (i, arg) in args.iter().enumerate() {
        if arg == "--data-dir" {
            return args.get(i + 1).cloned();
        }
        if let Some(val) = arg.strip_prefix("--data-dir=") {
            return Some(val.to_string());
        }
    }
    None
}
