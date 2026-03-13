use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;

use crate::action::execute_action;
use crate::app::{self, App};
use crate::helpers::{shutdown, spawn_initial_shell};
use crate::input;
use crate::{theme, ui};
use piki_core::workspace::FileWatcher;
use piki_core::workspace::config as ws_config;

const TICK_RATE: Duration = Duration::from_millis(50);
const DEBOUNCE: Duration = Duration::from_millis(500);
const PERIODIC_REFRESH: Duration = Duration::from_secs(3);

fn process_refresh_result(app: &mut App, result: app::RefreshResult) {
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
    app.refresh_pending = false;
    app.needs_redraw = true;
}

pub(crate) async fn run(
    mut terminal: DefaultTerminal,
    preflight_warnings: Vec<String>,
) -> anyhow::Result<()> {
    let manager = piki_core::workspace::WorkspaceManager::new();
    let mut app = App::new();
    app.sysinfo = piki_core::sysinfo::spawn_sysinfo_poller();
    app.theme = theme::load();

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

    // Restore persisted workspaces from all project configs
    let entries = ws_config::load_all();
    for entry in entries {
        let mut ws = app::Workspace::from_info(entry.into_info());

        // Spawn initial Shell tab
        spawn_initial_shell(&mut ws, app.pty_rows, app.pty_cols).await;

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
        let _ = ws.refresh_changed_files().await;

        app.workspaces.push(ws);
    }
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

        // Active workspace — check PTY bytes + is_alive
        {
            let idx = app.active_workspace;
            if let Some(ws) = app.workspaces.get_mut(idx) {
                let mut pty_done = false;
                if let Some(tab) = ws.current_tab_mut()
                    && let Some(ref mut pty) = tab.pty_session
                {
                    if !pty.is_alive() {
                        pty_done = true;
                    }
                    let current_bytes = pty.bytes_processed();
                    if current_bytes != tab.last_bytes_processed {
                        tab.last_bytes_processed = current_bytes;
                        app.needs_redraw = true;
                    }
                }
                if pty_done {
                    ws.status = app::WorkspaceStatus::Done;
                    app.needs_redraw = true;
                }
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
                    app.refresh_pending = true;
                    tokio::spawn(async move {
                        let files = app::get_changed_files(&path).await.unwrap_or_default();
                        let ab = app::get_ahead_behind(&path).await;
                        let _ = tx.send(app::RefreshResult {
                            workspace_idx: idx,
                            changed_files: files,
                            ahead_behind: ab,
                        });
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
                    let mut pty_done = false;
                    if let Some(tab) = ws.current_tab_mut()
                        && let Some(ref mut pty) = tab.pty_session
                        && !pty.is_alive()
                    {
                        pty_done = true;
                    }
                    if pty_done {
                        ws.status = app::WorkspaceStatus::Done;
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
