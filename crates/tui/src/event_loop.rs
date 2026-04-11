use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use crate::action::execute_action;
use crate::app::{self, App};
use crate::helpers::shutdown;
use crate::input;
use crate::{theme, ui};
use piki_core::workspace::FileWatcher;

const TICK_RATE: Duration = Duration::from_millis(50);
const DEBOUNCE: Duration = Duration::from_millis(500);
const PERIODIC_REFRESH: Duration = Duration::from_secs(3);

fn process_refresh_result(app: &mut App, result: app::RefreshResult) {
    if let Some(ref sub_dirs) = result.sub_directories {
        // Project workspace: update sub_directories
        if let Some(ws) = app.workspaces.get_mut(result.workspace_idx) {
            ws.sub_directories = sub_dirs.clone();
            ws.dirty = false;
            ws.last_refresh = Some(Instant::now());
        }
    } else {
        for file in &result.changed_files {
            let prefix = format!("{}@", file.path);
            let keys_to_remove: Vec<String> = app
                .diff_cache
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(key, _)| key.clone())
                .collect();
            for key in keys_to_remove {
                app.diff_cache.pop(&key);
            }
        }
        if let Some(ws) = app.workspaces.get_mut(result.workspace_idx) {
            ws.changed_files = result.changed_files;
            ws.ahead_behind = result.ahead_behind;
            ws.dirty = false;
            ws.last_refresh = Some(Instant::now());
        }
        // Remove selected paths that no longer appear in the file list
        if result.workspace_idx == app.active_workspace
            && let Some(ws) = app.workspaces.get(result.workspace_idx)
        {
            let live_paths: std::collections::HashSet<&str> =
                ws.changed_files.iter().map(|f| f.path.as_str()).collect();
            app.selected_files.retain(|p| live_paths.contains(p.as_str()));
        }
    }
    app.refresh_pending = false;
    app.needs_redraw = true;
}

pub(crate) async fn run(
    mut terminal: DefaultTerminal,
    preflight_warnings: Vec<String>,
    log_buffer: crate::log_buffer::LogBuffer,
    paths: piki_core::paths::DataPaths,
) -> anyhow::Result<()> {
    let manager = piki_core::workspace::WorkspaceManager::with_paths(paths.clone());
    let storage = std::sync::Arc::new(piki_core::storage::create_storage(&paths)?);
    let mut app = App::new(std::sync::Arc::clone(&storage), &paths);
    app.log_buffer = log_buffer;
    app.sysinfo = piki_core::sysinfo::spawn_sysinfo_poller();
    app.theme = theme::load_from(&paths);

    // Load UI preferences from storage (if SQLite backend)
    if let Some(ref ui_prefs) = storage.ui_prefs {
        if let Ok(groups) = ui_prefs.get_collapsed_groups() {
            app.collapsed_groups = groups;
        }
        if let Ok(Some(val)) = ui_prefs.get_preference("sidebar_pct")
            && let Ok(pct) = val.parse::<u16>()
        {
            app.sidebar_pct = pct.clamp(10, 90);
        }
        if let Ok(Some(val)) = ui_prefs.get_preference("left_split_pct")
            && let Ok(pct) = val.parse::<u16>()
        {
            app.left_split_pct = pct.clamp(10, 90);
        }
    }

    // Load chat config from storage (shared with desktop)
    if let Some(ref ui_prefs) = storage.ui_prefs
        && let Ok(Some(json)) = ui_prefs.get_preference("chat_config")
        && let Ok(cfg) = serde_json::from_str::<piki_core::chat::ChatConfig>(&json)
    {
        tracing::info!(
            model = %cfg.model,
            base_url = %cfg.base_url,
            "Loaded chat config from storage"
        );
        app.chat_panel.config = cfg;
    }

    // Show preflight warnings in status bar
    if !preflight_warnings.is_empty() {
        app.status_message = Some(preflight_warnings.join(" | "));
    }

    // Compute real terminal dimensions for PTY spawning
    let term_size = terminal.size()?;
    let pty_area = ui::layout::compute_terminal_area_with(
        Rect::new(0, 0, term_size.width, term_size.height),
        app.sidebar_pct,
    );
    app.pty_rows = pty_area.height;
    app.pty_cols = pty_area.width;

    // Restore persisted workspaces from storage backend
    let entries = storage.workspaces.load_all_workspaces();
    for entry in entries {
        let mut ws = app::Workspace::from_info(entry.into_info());

        // Start file watcher
        match FileWatcher::new(ws.path.clone(), ws.name.clone()) {
            Ok(watcher) => {
                ws.watcher = Some(watcher);
            }
            Err(e) => {
                app.status_message = Some(format!("Watcher error: {}", e));
            }
        }

        // Initial file status refresh so pre-existing changes show up
        if ws.info.workspace_type == piki_core::WorkspaceType::Project {
            ws.refresh_sub_directories().await;
        } else {
            let _ = ws.refresh_changed_files().await;
        }

        app.workspaces.push(ws);
    }
    // Sort by persistent order field for deterministic ordering across restarts
    app.workspaces.sort_by_key(|ws| ws.info.order);
    tracing::info!(count = app.workspaces.len(), "workspaces restored");
    if !app.workspaces.is_empty() {
        app.switch_workspace(0);
    }

    tracing::info!("event loop starting");
    let mut reader = EventStream::new();
    let mut tick_interval = tokio::time::interval(TICK_RATE);
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        // Phase 1: Render only when state has changed
        if app.needs_redraw {
            terminal.draw(|frame| {
                ui::layout::render(frame, &mut app);
            })?;
            app.needs_redraw = false;
        }

        // Phase 2: Wait for terminal event, refresh result, or tick
        let mut is_tick = false;

        tokio::select! {
            biased;

            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        if let Some(action) = input::handle_key_event(&mut app, key) {
                            execute_action(&mut app, &manager, action, &mut terminal).await?;
                        }
                        app.needs_redraw = true;
                    }
                    Some(Ok(Event::Paste(text))) => {
                        input::handle_paste(&mut app, &text);
                        app.needs_redraw = true;
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        if let Some(action) = input::mouse::handle_mouse_event(&mut app, mouse, &mut terminal) {
                            execute_action(&mut app, &manager, action, &mut terminal).await?;
                        }
                        app.needs_redraw = true;
                    }
                    Some(Ok(Event::Resize(cols, rows))) => {
                        let new_area = ui::layout::compute_terminal_area_with(Rect::new(0, 0, cols, rows), app.sidebar_pct);
                        app.pty_rows = new_area.height;
                        app.pty_cols = new_area.width;
                        for ws in &mut app.workspaces {
                            for tab in &mut ws.tabs {
                                if let Some(ref mut pty) = tab.pty_session {
                                    let _ = pty.resize(new_area.height, new_area.width);
                                }
                            }
                        }
                        // Clear open diff so it re-renders at new width; LRU handles stale entries
                        app.diff_content = None;
                        app.footer_cache = None;
                        app.needs_redraw = true;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {}
                    None => break,
                }
            }

            result = app.refresh_rx.recv() => {
                if let Some(result) = result {
                    process_refresh_result(&mut app, result);
                    while let Ok(result) = app.refresh_rx.try_recv() {
                        process_refresh_result(&mut app, result);
                    }
                }
            }

            entry = app.undo_rx.recv() => {
                if let Some(entry) = entry {
                    app.undo_stack.push_back(entry);
                    while let Ok(entry) = app.undo_rx.try_recv() {
                        app.undo_stack.push_back(entry);
                    }
                    // Cap at 20 entries
                    while app.undo_stack.len() > 20 {
                        app.undo_stack.pop_front();
                    }
                }
            }

            msg = app.status_rx.recv() => {
                if let Some(msg) = msg {
                    let mut last_msg = msg;
                    // Drain any additional messages, keep the last one
                    while let Ok(msg) = app.status_rx.try_recv() {
                        last_msg = msg;
                    }
                    // Infer toast level from message content
                    let level = if last_msg.starts_with("✓") || last_msg.starts_with("Staged:") || last_msg.starts_with("Unstaged:") {
                        app::ToastLevel::Success
                    } else if last_msg.contains("failed") || last_msg.contains("error") || last_msg.contains("Error") {
                        app::ToastLevel::Error
                    } else {
                        app::ToastLevel::Info
                    };
                    app.set_toast(last_msg, level);
                    app.needs_redraw = true;
                }
            }

            chat_event = app.chat_token_rx.recv() => {
                if let Some(event) = chat_event {
                    match event {
                        piki_api_client::ChatStreamEvent::Token(token) => {
                            app.chat_panel.current_response.push_str(&token);
                            app.needs_redraw = true;
                        }
                        piki_api_client::ChatStreamEvent::Done(content) => {
                            // Special case: model list response (packed as "__MODELS__" prefix)
                            if let Some(model_data) = content.strip_prefix("__MODELS__") {
                                app.chat_panel.models = model_data
                                    .lines()
                                    .filter(|l| !l.is_empty())
                                    .map(|l| l.to_string())
                                    .collect();
                                // Auto-select first model if none is set
                                if app.chat_panel.config.model.is_empty()
                                    && let Some(first) = app.chat_panel.models.first()
                                {
                                    app.chat_panel.config.model = first.clone();
                                }
                            } else {
                                // Normal chat response completion
                                let response_text = if app.chat_panel.current_response.is_empty() {
                                    content
                                } else {
                                    std::mem::take(&mut app.chat_panel.current_response)
                                };
                                app.chat_panel.messages.push(piki_core::chat::ChatMessage {
                                    role: piki_core::chat::ChatRole::Assistant,
                                    content: response_text,
                                });
                                app.chat_panel.streaming = false;
                            }
                            app.needs_redraw = true;
                        }
                        piki_api_client::ChatStreamEvent::Error(e) => {
                            app.set_toast(format!("Chat error: {e}"), app::ToastLevel::Error);
                            app.chat_panel.streaming = false;
                            app.chat_panel.current_response.clear();
                            app.needs_redraw = true;
                        }
                    }
                }
            }

            _ = tick_interval.tick() => {
                is_tick = true;
            }
        }

        // Phase 3: Sync work after every wakeup
        let now = Instant::now();

        // Poll file watcher events — mark workspaces as dirty when files change
        for ws in &mut app.workspaces {
            if let Some(ref mut watcher) = ws.watcher
                && watcher.try_recv().is_some()
            {
                watcher.drain();
                ws.dirty = true;
            }
        }

        // Active workspace — check PTY bytes + is_alive for all tabs
        {
            let idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(idx) {
                // Check bytes on active tab for redraw
                if let Some(tab) = ws.current_tab_mut()
                    && let Some(ref mut pty) = tab.pty_session
                {
                    let current_bytes = pty.bytes_processed();
                    if current_bytes != tab.last_bytes_processed {
                        tab.last_bytes_processed = current_bytes;
                        app.needs_redraw = true;
                    }
                }
                // Check is_alive for all tabs, recompute workspace status
                let mut any_alive = false;
                let mut any_tab = false;
                for tab in &mut ws.tabs {
                    if let Some(ref mut pty) = tab.pty_session {
                        any_tab = true;
                        if pty.is_alive() {
                            any_alive = true;
                        }
                    }
                }
                let new_status = if any_alive {
                    app::WorkspaceStatus::Busy
                } else if any_tab {
                    app::WorkspaceStatus::Done
                } else {
                    app::WorkspaceStatus::Idle
                };
                if ws.status != new_status {
                    ws.status = new_status;
                    app.needs_redraw = true;
                }
            }
        }

        // Poll API Explorer pending responses
        if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
            && let Some(tab) = ws.current_tab_mut()
            && let Some(ref mut api) = tab.api_state
            && api.loading
        {
            let mut slot = api.pending_responses.lock();
            if let Some(responses) = slot.take() {
                api.responses = responses;
                api.loading = false;
                api.response_scroll = 0;
                app.needs_redraw = true;
            }
        }

        // Spawn background git refresh ONLY for active workspace
        {
            let idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get(idx) {
                let since_last = ws.last_refresh.map(|t| now.duration_since(t));
                let should_refresh = if ws.dirty {
                    since_last.map(|d| d >= DEBOUNCE).unwrap_or(true)
                } else {
                    since_last.map(|d| d >= PERIODIC_REFRESH).unwrap_or(true)
                };
                if should_refresh && !app.refresh_pending {
                    let path = ws.info.path.clone();
                    let tx = app.refresh_tx.clone();
                    let is_project = ws.info.workspace_type == piki_core::WorkspaceType::Project;
                    app.refresh_pending = true;
                    tokio::spawn(async move {
                        if is_project {
                            // Scan sub-directories instead of git status
                            let mut dirs = Vec::new();
                            if let Ok(mut entries) = tokio::fs::read_dir(&path).await {
                                while let Ok(Some(entry)) = entries.next_entry().await {
                                    if let Ok(ft) = entry.file_type().await
                                        && ft.is_dir()
                                        && let Some(name) = entry.file_name().to_str()
                                        && !name.starts_with('.')
                                    {
                                        dirs.push(name.to_string());
                                    }
                                }
                            }
                            dirs.sort();
                            let _ = tx.send(app::RefreshResult {
                                workspace_idx: idx,
                                changed_files: Vec::new(),
                                ahead_behind: None,
                                sub_directories: Some(dirs),
                            });
                        } else {
                            let files = app::get_changed_files(&path).await.unwrap_or_default();
                            let ab = app::get_ahead_behind(&path).await;
                            let _ = tx.send(app::RefreshResult {
                                workspace_idx: idx,
                                changed_files: files,
                                ahead_behind: ab,
                                sub_directories: None,
                            });
                        }
                    });
                }
            }
        }

        // Tick nucleo fuzzy matcher — processes new items and pattern changes
        if let Some(ref mut state) = app.fuzzy {
            let status = state.nucleo.tick(10);
            if status.changed {
                let count = state.nucleo.snapshot().matched_item_count() as usize;
                if count == 0 {
                    state.selected = 0;
                } else if state.selected >= count {
                    state.selected = count - 1;
                }
                app.needs_redraw = true;
            }
        }

        // Tick nucleo command palette matcher
        if let Some(ref mut state) = app.command_palette {
            let status = state.nucleo.tick(10);
            if status.changed {
                let count = state.nucleo.snapshot().matched_item_count() as usize;
                if count == 0 {
                    state.selected = 0;
                } else if state.selected >= count {
                    state.selected = count - 1;
                }
                app.needs_redraw = true;
            }
        }

        // Expire toasts
        if app.expire_toast() {
            app.needs_redraw = true;
        }

        // Phase 4: Tick-gated periodic work
        if is_tick {
            // Inactive workspaces — only check is_alive every ~1s
            if now.duration_since(app.last_inactive_pty_check) >= Duration::from_secs(1) {
                app.last_inactive_pty_check = now;
                for (i, ws) in app.workspaces.iter_mut().enumerate() {
                    if i == app.active_workspace {
                        continue;
                    }
                    let mut any_alive = false;
                    let mut any_tab = false;
                    for tab in &mut ws.tabs {
                        if let Some(ref mut pty) = tab.pty_session {
                            any_tab = true;
                            if pty.is_alive() {
                                any_alive = true;
                            }
                        }
                    }
                    let new_status = if any_alive {
                        app::WorkspaceStatus::Busy
                    } else if any_tab {
                        app::WorkspaceStatus::Done
                    } else {
                        app::WorkspaceStatus::Idle
                    };
                    if ws.status != new_status {
                        ws.status = new_status;
                        app.needs_redraw = true;
                    }
                }
            }
        }

        if app.should_quit {
            shutdown(&mut app);
            break;
        }
    }

    Ok(())
}
