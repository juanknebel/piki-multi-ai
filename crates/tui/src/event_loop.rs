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
use piki_core::notifications;
use piki_core::shell_integration::ShellEvent;
use piki_core::workspace::FileWatcher;

/// Fallback cadence for periodic work (liveness checks, idle detection, OSC
/// drains, git-refresh scheduling). PTY output no longer rides this: reader
/// threads wake the loop directly through `App::pty_output`, so the tick can
/// be slow — it only bounds how stale the periodic bookkeeping may get.
const TICK_RATE: Duration = Duration::from_millis(250);
const DEBOUNCE: Duration = Duration::from_millis(500);
const PERIODIC_REFRESH: Duration = Duration::from_secs(3);
/// Minimum interval between frames (~30 fps cap). Output-driven wakeups can
/// arrive per PTY read; without this cap a fast-streaming agent would have
/// the loop rebuilding the full UI once per chunk.
const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(33);
/// Cadence of the Agents-pane activity spinner. Each advance forces a full
/// UI redraw, so this — not `TICK_RATE` — bounds the steady-state frame rate
/// while any agent is running.
const SPINNER_INTERVAL: Duration = Duration::from_millis(150);
/// Cadence of passive (screen-scrape) agent-state detection. Status changes
/// on a hookless agent are human-scale events; scraping faster than this just
/// burns locks against the PTY reader thread.
const PASSIVE_DETECT_INTERVAL: Duration = Duration::from_millis(300);

fn process_refresh_result(app: &mut App, result: app::RefreshResult) {
    if let Some(ws) = app.workspaces.get_mut(result.workspace_idx) {
        ws.changed_files = result.changed_files;
        ws.ahead_behind = result.ahead_behind;
        ws.branch = result.branch;
        ws.dirty = false;
        ws.last_refresh = Some(Instant::now());
    }
    app.refresh_pending = false;
    app.needs_redraw = true;
}

fn process_watcher_result(app: &mut App, result: app::WatcherResult) {
    if let Some(ws) = app.workspaces.get_mut(result.workspace_idx) {
        match result.watcher {
            Ok(watcher) => ws.watcher = Some(watcher),
            Err(e) => {
                app.status_message = Some(format!("Watcher error: {}", e));
            }
        }
    }
}

pub(crate) async fn run(
    mut terminal: DefaultTerminal,
    preflight_warnings: Vec<String>,
    log_buffer: crate::log_buffer::LogBuffer,
    paths: piki_core::paths::DataPaths,
) -> anyhow::Result<()> {
    let run_t0 = Instant::now();
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
        if let Ok(Some(val)) = ui_prefs.get_preference("code_review_split_pct")
            && let Ok(pct) = val.parse::<u16>()
        {
            app.code_review_split_pct = pct.clamp(10, 90);
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
    let restore_t0 = Instant::now();
    let entries = storage.workspaces.load_all_workspaces();
    for entry in entries {
        let ws = app::Workspace::from_info(entry.into_info());
        app.workspaces.push(ws);
    }
    // Sort by persistent order field for deterministic ordering across restarts
    app.workspaces.sort_by_key(|ws| ws.info.order);
    tracing::info!(
        count = app.workspaces.len(),
        elapsed_ms = restore_t0.elapsed().as_millis(),
        "startup: workspaces restored (watchers still pending)"
    );

    // FileWatcher setup runs in the BACKGROUND: `FileWatcher::new` walks the
    // whole worktree tree synchronously to register the recursive inotify
    // watch, so doing it inline for every workspace before the first frame
    // could take seconds on a large tree (e.g. an unfiltered target/ or
    // node_modules/). Results arrive through watcher_rx and attach lazily —
    // mirrors the git-status backgrounding below, which solved the same
    // shape of problem.
    for (idx, ws) in app.workspaces.iter().enumerate() {
        let path = ws.path.clone();
        let name = ws.name.clone();
        let tx = app.watcher_tx.clone();
        tokio::spawn(async move {
            let t0 = Instant::now();
            let watcher = tokio::task::spawn_blocking(move || FileWatcher::new(path, name))
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("watcher task panicked: {e}")));
            tracing::info!(
                workspace_idx = idx,
                elapsed_ms = t0.elapsed().as_millis(),
                ok = watcher.is_ok(),
                "startup: background file watcher setup done"
            );
            let _ = tx.send(app::WatcherResult {
                workspace_idx: idx,
                watcher,
            });
        });
    }

    // Initial file status refresh runs in the BACKGROUND: blocking startup on
    // sequential `git status` for every workspace made launch take seconds.
    // Results arrive through the regular refresh channel and fill in the
    // sidebar counts as they land. (Harmless no-op for non-git directories.)
    for (idx, ws) in app.workspaces.iter().enumerate() {
        let path = ws.info.path.clone();
        // Branch is inferred, never persisted (see `Workspace::branch`) — and
        // only inferred once some tool (shell/agent/git) has actually run in
        // this workspace, i.e. it has a tab. Startup itself doesn't count,
        // so a workspace with no tabs yet shows no branch until one opens.
        let has_tab = !ws.tabs.is_empty();
        let tx = app.refresh_tx.clone();
        tokio::spawn(async move {
            let files = app::get_changed_files(&path).await.unwrap_or_default();
            let ab = app::get_ahead_behind(&path).await;
            let branch = if has_tab {
                app::get_current_branch(&path).await
            } else {
                None
            };
            let _ = tx.send(app::RefreshResult {
                workspace_idx: idx,
                changed_files: files,
                ahead_behind: ab,
                branch,
            });
        });
    }
    if !app.workspaces.is_empty() {
        // Restore the last focused workspace from UiPrefs if available.
        // Match by `path` since it's unique and stable across restarts.
        let restored_idx = app
            .storage
            .ui_prefs
            .as_ref()
            .and_then(|prefs| {
                prefs
                    .get_preference("last_focused_workspace")
                    .ok()
                    .flatten()
            })
            .and_then(|saved_path| {
                app.workspaces
                    .iter()
                    .position(|ws| ws.info.path.to_string_lossy() == saved_path)
            });
        // Bare switch_workspace: keep the default ActivePane::WorkspaceList on
        // cold start (switch_workspace_and_focus would force MainPanel here).
        app.switch_workspace(restored_idx.unwrap_or(0));
    }

    tracing::info!(
        elapsed_ms = run_t0.elapsed().as_millis(),
        "startup: event loop starting (first frame imminent)"
    );
    let mut reader = EventStream::new();
    let mut tick_interval = tokio::time::interval(TICK_RATE);
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Render throttle state: when a redraw is due but the last frame is less
    // than MIN_RENDER_INTERVAL old, `render_deadline` arms a select branch
    // that wakes the loop exactly when the frame becomes eligible.
    let mut last_draw_at: Option<Instant> = None;
    let mut render_deadline: Option<Instant> = None;
    let pty_output = app.pty_output.clone();

    loop {
        // Phase 0: Keep the Agents highlight on the tab the user is standing on
        app.sync_agent_selection();

        // Phase 1: Render only when state has changed, capped at ~30 fps
        if app.needs_redraw {
            let now = Instant::now();
            let eligible = last_draw_at
                .is_none_or(|t| now.duration_since(t) >= MIN_RENDER_INTERVAL);
            if eligible {
                terminal.draw(|frame| {
                    ui::layout::render(frame, &mut app);
                })?;
                app.needs_redraw = false;
                last_draw_at = Some(Instant::now());
                render_deadline = None;
            } else if render_deadline.is_none() {
                // Safe: `eligible` is false only when `last_draw_at` is Some.
                render_deadline = Some(last_draw_at.unwrap() + MIN_RENDER_INTERVAL);
            }
        } else {
            render_deadline = None;
        }
        // Evaluated unconditionally (select! builds every future even for
        // disabled branches); the far-future fallback is never polled.
        let render_wakeup =
            render_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));

        // Phase 2: Wait for terminal event, PTY output, refresh result, or tick
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
                    Some(Ok(Event::FocusGained)) => {
                        piki_core::notifications::set_window_focused(true);
                    }
                    Some(Ok(Event::FocusLost)) => {
                        piki_core::notifications::set_window_focused(false);
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
                        app.footer_cache = None;
                        app.needs_redraw = true;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {}
                    None => break,
                }
            }

            // Coalesced "some PTY produced output" wakeup from the reader
            // threads. `take()` re-arms the signal; only the active tab can
            // need a redraw from output, and its byte probe is one atomic
            // load — background tabs cost nothing here.
            _ = pty_output.notified() => {
                pty_output.take();
                check_active_tab_output(&mut app);
            }

            result = app.refresh_rx.recv() => {
                if let Some(result) = result {
                    process_refresh_result(&mut app, result);
                    while let Ok(result) = app.refresh_rx.try_recv() {
                        process_refresh_result(&mut app, result);
                    }
                }
            }

            result = app.watcher_rx.recv() => {
                if let Some(result) = result {
                    process_watcher_result(&mut app, result);
                    while let Ok(result) = app.watcher_rx.try_recv() {
                        process_watcher_result(&mut app, result);
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
                                    tool_calls: None,
                                    tool_call_id: None,
                                });
                                app.chat_panel.streaming = false;
                            }
                            app.needs_redraw = true;
                        }
                        piki_api_client::ChatStreamEvent::ToolCalls(_calls) => {
                            // Tool calls are handled by the agent loop (F4).
                            // In plain chat mode, treat as end of response.
                            app.chat_panel.streaming = false;
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

            agent_event = app.agent_event_rx.recv() => {
                if let Some(event) = agent_event {
                    match event {
                        piki_agent::AgentEvent::Token(token) => {
                            app.chat_panel.current_response.push_str(&token);
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::Done(content) => {
                            let response_text = if app.chat_panel.current_response.is_empty() {
                                content
                            } else {
                                std::mem::take(&mut app.chat_panel.current_response)
                            };
                            app.chat_panel.messages.push(piki_core::chat::ChatMessage {
                                role: piki_core::chat::ChatRole::Assistant,
                                content: response_text,
                                tool_calls: None,
                                tool_call_id: None,
                            });
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::ToolCallsStarted(calls) => {
                            let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
                            app.chat_panel.agent_tool_status = Some(names.join(", "));
                            app.chat_panel.current_response.clear();
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::ToolExecuting { name } => {
                            app.chat_panel.agent_tool_status = Some(name);
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::ToolResult { name, result, is_error, .. } => {
                            // Show tool result as a Tool message in the chat
                            let prefix = if is_error { "[Error] " } else { "" };
                            let display = format!("[{name}] {prefix}{result}");
                            // Truncate long results for display
                            let truncated = if display.len() > 500 {
                                format!("{}...", &display[..500])
                            } else {
                                display
                            };
                            app.chat_panel.messages.push(piki_core::chat::ChatMessage {
                                role: piki_core::chat::ChatRole::Tool,
                                content: truncated,
                                tool_calls: None,
                                tool_call_id: None,
                            });
                            app.chat_panel.agent_tool_status = None;
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::Finished => {
                            app.chat_panel.streaming = false;
                            app.chat_panel.agent_tool_status = None;
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::Error(e) => {
                            app.set_toast(format!("Agent error: {e}"), app::ToastLevel::Error);
                            app.chat_panel.streaming = false;
                            app.chat_panel.agent_tool_status = None;
                            app.chat_panel.current_response.clear();
                            app.needs_redraw = true;
                        }
                        piki_agent::AgentEvent::ApprovalRequired(req) => {
                            app.chat_panel.pending_approval = Some(req);
                            app.needs_redraw = true;
                        }
                    }
                }
            }

            // A frame became eligible after being deferred by the ~30 fps
            // cap — wake with no other work so Phase 1 can draw it.
            _ = tokio::time::sleep_until(render_wakeup.into()), if render_deadline.is_some() => {}

            _ = tick_interval.tick() => {
                is_tick = true;
            }
        }

        // Phase 3: Sync work after every wakeup
        let now = Instant::now();

        // O(workspaces × tabs) polling with per-tab mutex locks — tick-gated
        // so keystrokes and stream events never pay the full per-tab sweep.
        if is_tick {
            poll_workspaces(&mut app, now);
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
            // Advance the Agents-pane activity spinner while any agent runs.
            // Throttled to its own cadence (not every tick): the redraw this
            // forces rebuilds the whole UI, so at the raw tick rate a single
            // running agent anywhere kept the app rendering at 20 fps
            // indefinitely. The check itself also allocates `agent_rows()`
            // and takes a mutex per tab, so it rides the same throttle.
            if now.duration_since(app.last_spinner_at) >= SPINNER_INTERVAL {
                app.last_spinner_at = now;
                let any_running = app.agent_rows().iter().any(|&(wi, ti)| {
                    matches!(
                        app.workspaces[wi].tabs[ti].cli_agent_snapshot(),
                        Some((piki_core::cli_agent::CliAgentStatus::Running, _, _))
                    )
                });
                if any_running {
                    app.spinner_frame = app.spinner_frame.wrapping_add(1);
                    app.needs_redraw = true;
                }
            }

            // Looking at a tab acknowledges its "unseen news" marker, so the
            // amber ● stops propagating once the user has actually seen it.
            if app.active_pane == crate::app::ActivePane::MainPanel
                && let Some(ws) = app.workspaces.get(app.active_workspace)
                && let Some(tab) = ws.tabs.get(ws.active_tab)
                && let Some(shell) = tab.pty_session.as_ref().and_then(|p| p.shell())
            {
                let mut guard = shell.lock();
                if let Some(agent) = guard.state.cli_agent.as_mut()
                    && agent.last_attention_at.is_some()
                {
                    agent.acknowledge();
                    app.needs_redraw = true;
                }
            }

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

/// Redraw check for the active tab's PTY output: one atomic load comparing
/// the byte counter against the last render. Called from the output-signal
/// wakeup (immediate) and from [`poll_workspaces`] (tick fallback).
fn check_active_tab_output(app: &mut App) {
    let idx = app.active_workspace;
    if let Some(ws) = app.workspaces.get_mut(idx)
        && let Some(tab) = ws.current_tab_mut()
        && let Some(ref pty) = tab.pty_session
    {
        let current_bytes = pty.bytes_processed();
        if current_bytes != tab.last_bytes_processed {
            tab.last_bytes_processed = current_bytes;
            app.needs_redraw = true;
        }
    }
}

/// Per-tab polling across every workspace: file-watcher drain, PTY byte /
/// liveness checks, idle detection, passive agent-state detection, shell +
/// cli-agent OSC drain, API Explorer polling and git-refresh scheduling.
///
/// Everything here is O(workspaces × tabs) and takes per-tab mutex locks, so
/// the event loop calls it only on the tick — input and stream wakeups must
/// not pay this sweep on every keystroke.
fn poll_workspaces(app: &mut App, now: Instant) {
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
    check_active_tab_output(app);
    {
        let idx = app.active_workspace;
        if let Some(ws) = app.workspaces.get_mut(idx) {
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

    // PTY idle detection across ALL workspaces (every tick).
    //
    // Each provider tab carries its own `IdleWatcher` (`tab.idle_watcher`,
    // `Some` only for `AIProvider::Custom(_)`). The watcher tracks the
    // PTY byte counter and emits a one-shot signal when bytes have been
    // still for the configured threshold (default 3s). The OS
    // notification fires regardless of whether the workspace is active.
    {
        let active_ws = app.active_workspace;
        let mut idle_events: Vec<IdleEvent> = Vec::new();
        for (ws_idx, ws) in app.workspaces.iter_mut().enumerate() {
            let active_tab = ws.active_tab;
            for (tab_idx, tab) in ws.tabs.iter_mut().enumerate() {
                let Some(ref pty) = tab.pty_session else {
                    continue;
                };
                let Some(ref mut watcher) = tab.idle_watcher else {
                    continue;
                };
                if !pty.peek_alive() {
                    continue;
                }
                // The structured cli-agent channel proved itself live for
                // this tab (≥1 OSC 777 event parsed) → it owns attention;
                // skip the byte-silence heuristic so it can't double-fire.
                // If hooks are missing / version-skewed, no events arrive,
                // `cli_agent` stays `None`, and the watcher remains the
                // graceful fallback.
                if pty
                    .shell()
                    .is_some_and(|s| s.lock().state.cli_agent.is_some())
                {
                    continue;
                }
                if let Some(sig) = watcher.poll(pty.bytes_processed()) {
                    idle_events.push(IdleEvent {
                        workspace_idx: ws_idx,
                        workspace_name: ws.info.name.clone(),
                        provider_label: tab.provider.label().to_string(),
                        origin: format!("ws-{ws_idx}#tab-{}", tab.id),
                        silent_for: sig.silent_for,
                        from_active_view: ws_idx == active_ws && tab_idx == active_tab,
                    });
                }
            }
        }
        for event in idle_events {
            if let Some(ws) = app.workspaces.get_mut(event.workspace_idx) {
                ws.has_idle_notification = true;
            }
            app.needs_redraw = true;
            let icon = app
                .provider_manager
                .get(&event.provider_label)
                .and_then(|c| c.icon.clone());
            notifications::notify_agent_idle(
                &event.origin,
                &event.workspace_name,
                &event.provider_label,
                event.silent_for,
                icon.as_deref(),
                event.from_active_view,
            );
        }
    }

    // Passive agent-state detection.
    //
    // Providers with no hook bridge (currently just Codex) get no
    // `cli_agent_sock`, but `spawn_tab` still turns shell integration on
    // for them when `agent_state_detect::manifest_for_command` matches,
    // so `OscParser` captures their OSC window-title. Combined with a
    // screen-text sample, that's enough to classify Working/Blocked/Idle
    // without a hook — write the result into the same `cli_agent` field
    // the hook bridges use so the Agents pane needs no extra rendering.
    //
    // The screen sample locks the same vt100 parser the PTY reader thread
    // holds while processing output batches, and materializes the whole
    // screen as strings — so it runs at most every `PASSIVE_DETECT_INTERVAL`,
    // and per tab only when new output has actually arrived (the byte
    // counter is a lock-free atomic load).
    if now.duration_since(app.last_passive_detect) >= PASSIVE_DETECT_INTERVAL {
        app.last_passive_detect = now;
        for ws in app.workspaces.iter_mut() {
            for tab in ws.tabs.iter_mut() {
                let Some(ref pty) = tab.pty_session else {
                    continue;
                };
                let bytes = pty.bytes_processed();
                if bytes == tab.last_detect_bytes {
                    continue;
                }
                tab.last_detect_bytes = bytes;
                let piki_core::AIProvider::Custom(name) = &tab.provider else {
                    continue;
                };
                let Some(cmd) = app.provider_manager.get(name).map(|c| c.command.clone()) else {
                    continue;
                };
                let Some(manifest) = piki_core::agent_state_detect::manifest_for_command(&cmd)
                else {
                    continue;
                };
                let Some(shell) = pty.shell() else {
                    continue;
                };
                let title = shell.lock().state.window_title.clone();

                let screen_tail = if let Some(ref parser) = tab.pty_parser {
                    let guard = parser.lock();
                    let (rows, cols) = guard.screen().size();
                    let tail: Vec<String> = guard
                        .screen()
                        .rows(0, cols)
                        .take(rows as usize)
                        .filter(|r| !r.trim().is_empty())
                        .collect();
                    drop(guard);
                    tail.into_iter()
                        .rev()
                        .take(6)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::new()
                };

                let Some(new_status) =
                    piki_core::agent_state_detect::detect(manifest, title.as_deref(), &screen_tail)
                else {
                    continue;
                };

                let mut guard = shell.lock();
                let agent = guard
                    .state
                    .cli_agent
                    .get_or_insert_with(piki_core::cli_agent::CliAgentState::new);
                if agent.status != new_status {
                    let was_running = agent.status == piki_core::cli_agent::CliAgentStatus::Running;
                    agent.status = new_status;
                    if was_running
                        && matches!(
                            new_status,
                            piki_core::cli_agent::CliAgentStatus::WaitingPermission
                                | piki_core::cli_agent::CliAgentStatus::Idle
                                | piki_core::cli_agent::CliAgentStatus::Done
                        )
                    {
                        agent.last_attention_at = Some(std::time::Instant::now());
                    }
                    app.needs_redraw = true;
                }
            }
        }
    }

    // Shell + cli-agent OSC drain (every tick).
    //
    // Tabs spawned with OSC integration accumulate `ShellEvent`s on
    // `PtySession.shell().pending_events`. Shell `command-end` markers
    // become a notification with the workspace name + exit code.
    // Structured Claude `CliAgent` events that warrant attention
    // (permission / idle / done) set the workspace's idle badge and
    // fire a precise notification — this replaces the byte-silence
    // `IdleWatcher` for Claude-with-hooks tabs (whose watcher is now
    // `None`). Other lifecycle events are informational and dropped.
    {
        let active_ws = app.active_workspace;
        let mut command_end_events: Vec<CommandEndNotice> = Vec::new();
        let mut cli_agent_events: Vec<CliAgentNotice> = Vec::new();
        for (ws_idx, ws) in app.workspaces.iter_mut().enumerate() {
            let ws_name = ws.info.name.clone();
            let active_tab = ws.active_tab;
            for (tab_idx, tab) in ws.tabs.iter_mut().enumerate() {
                let Some(ref pty) = tab.pty_session else {
                    continue;
                };
                let Some(shell) = pty.shell() else { continue };
                let from_active_view = ws_idx == active_ws && tab_idx == active_tab;
                let drained = shell.lock().drain_events();
                for ev in drained {
                    match ev {
                        ShellEvent::CommandEnd { exit_code, command } => {
                            command_end_events.push(CommandEndNotice {
                                origin: format!("ws-{ws_idx}#tab-{}", tab.id),
                                workspace_name: ws_name.clone(),
                                exit_code,
                                command,
                                from_active_view,
                            });
                        }
                        ShellEvent::CliAgent(a) => {
                            if let Some((kind, summary)) = a.attention() {
                                cli_agent_events.push(CliAgentNotice {
                                    workspace_idx: ws_idx,
                                    workspace_name: ws_name.clone(),
                                    provider_label: tab.provider.label().to_string(),
                                    origin: format!("ws-{ws_idx}#tab-{}", tab.id),
                                    kind,
                                    summary: summary.map(|s| s.to_string()),
                                    from_active_view,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        for n in command_end_events {
            notifications::notify_command_end(
                &n.origin,
                &n.workspace_name,
                n.exit_code,
                n.command.as_deref(),
                n.from_active_view,
            );
        }
        for n in cli_agent_events {
            if let Some(ws) = app.workspaces.get_mut(n.workspace_idx) {
                ws.has_idle_notification = true;
            }
            app.needs_redraw = true;
            let icon = app
                .provider_manager
                .get(&n.provider_label)
                .and_then(|c| c.icon.clone());
            notifications::notify_cli_agent(
                &n.origin,
                &n.workspace_name,
                n.kind,
                n.summary.as_deref(),
                icon.as_deref(),
                n.from_active_view,
            );
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

    // Poll PR picker list load
    {
        let result = { app.pending_pr_list.lock().take() };
        if let Some(result) = result {
            if let Some(crate::dialog_state::DialogState::PrPicker {
                loading, items, error, selected, ..
            }) = &mut app.active_dialog
            {
                *loading = false;
                match result {
                    Ok(list) => {
                        *items = list;
                        *selected = 0;
                        *error = None;
                    }
                    Err(e) => *error = Some(e),
                }
            }
            app.needs_redraw = true;
        }
    }

    // Poll PR checkout resolution — opens the ephemeral review workspace
    {
        let result = { app.pending_pr_checkout.lock().take() };
        if let Some(result) = result {
            match result {
                Ok(session) => {
                    let info = piki_core::workspace::WorkspaceManager::create_review_workspace(
                        &session.checkout,
                    );
                    let review_dir = session.checkout.path.clone();
                    let base_spec = session.checkout.base_spec.clone();
                    let repo_nwo = session.checkout.repo_nwo.clone();
                    let idx = app.open_review_workspace(info);
                    let tab_idx = app.workspaces[idx].add_tab(piki_core::AIProvider::CodeReview, true, None);
                    app.workspaces[idx].active_tab = tab_idx;
                    let mut cr = crate::code_review::CodeReviewState::new(
                        session.checkout.pr,
                        review_dir,
                        base_spec,
                        repo_nwo,
                        session.files,
                    );
                    cr.existing_comments = session.existing_comments;
                    app.workspaces[idx].code_review = Some(cr);
                    app.active_dialog = None;
                    app.mode = app::AppMode::Normal;
                    app.set_toast("PR loaded", app::ToastLevel::Success);
                }
                Err(e) => {
                    if let Some(crate::dialog_state::DialogState::PrPicker {
                        checking_out, error, ..
                    }) = &mut app.active_dialog
                    {
                        *checking_out = None;
                        *error = Some(e);
                    }
                }
            }
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
                // See the startup loop above: branch is only inferred
                // once a tool has actually run in this workspace.
                let has_tab = !ws.tabs.is_empty();
                let tx = app.refresh_tx.clone();
                app.refresh_pending = true;
                tokio::spawn(async move {
                    // Non-git dirs (Project workspaces) yield an empty list
                    let files = app::get_changed_files(&path).await.unwrap_or_default();
                    let ab = app::get_ahead_behind(&path).await;
                    let branch = if has_tab {
                        app::get_current_branch(&path).await
                    } else {
                        None
                    };
                    let _ = tx.send(app::RefreshResult {
                        workspace_idx: idx,
                        changed_files: files,
                        ahead_behind: ab,
                        branch,
                    });
                });
            }
        }
    }
}

/// A queued shell `command-end` notification, applied after the
/// per-workspace borrow scope ends.
struct CommandEndNotice {
    origin: String,
    workspace_name: String,
    exit_code: Option<i32>,
    command: Option<String>,
    /// True when this event's tab is the active tab of the active
    /// workspace — combined with window focus to gate the OS toast.
    from_active_view: bool,
}

/// A queued structured cli-agent attention event (Stop / Notification /
/// PermissionRequest), applied after the per-workspace borrow scope ends.
struct CliAgentNotice {
    workspace_idx: usize,
    workspace_name: String,
    provider_label: String,
    /// Mailbox dedup key — stable per (workspace, tab) tuple.
    origin: String,
    /// cli-agent notification kind: `permission_request` / `notification` /
    /// `stop`.
    kind: &'static str,
    /// Hook-built one-liner (permission preview / final response preview).
    summary: Option<String>,
    /// True when this event's tab is the active tab of the active
    /// workspace — combined with window focus to gate the OS toast.
    from_active_view: bool,
}

/// A queued idle-notification event, applied after the per-workspace borrow
/// scope ends so the OS notification can run without holding `&mut App`.
struct IdleEvent {
    workspace_idx: usize,
    workspace_name: String,
    provider_label: String,
    /// Mailbox dedup key — stable per (workspace, tab) tuple.
    origin: String,
    /// How long the PTY was silent before the watcher fired. Surfaced in
    /// the notification body as `(idle Ns)`.
    silent_for: Duration,
    /// True when this event's tab is the active tab of the active
    /// workspace — combined with window focus to gate the OS toast.
    from_active_view: bool,
}
