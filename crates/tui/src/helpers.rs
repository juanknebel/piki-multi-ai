use std::sync::Arc;

use ratatui::layout::Rect;

use crate::app::{self, App};
use crate::clipboard;
use crate::ui;
use piki_core::AIProvider;
use piki_core::cli_agent::install as cli_agent_install;
use piki_core::cli_agent::install_antigravity as agy_install;
use piki_core::cli_agent::{AgentBridge, bridge_for_command};
use piki_core::pty::PtySession;
use piki_core::shell_integration::install as shell_install;

/// Kill all PTY sessions and drop watchers for a clean exit.
pub(crate) fn shutdown(app: &mut App) {
    for ws in &mut app.workspaces {
        for tab in &mut ws.tabs {
            if let Some(ref mut pty) = tab.pty_session {
                let _ = pty.kill();
            }
        }
        ws.tabs.clear();
        ws.watcher = None;
    }
}

/// Tools a provider's hook bridge needs but that aren't on PATH, with the
/// agent's display name. `None` when the provider has no bridge (nothing to
/// degrade) or when everything it needs is installed.
///
/// The tab spawns either way — [`spawn_tab`] just falls back to the byte-silence
/// idle heuristic — so callers use this to *warn*, never to block.
pub(crate) fn missing_bridge_prereqs(
    provider: &AIProvider,
    provider_manager: &piki_core::providers::ProviderManager,
) -> Option<(String, Vec<String>)> {
    let AIProvider::Custom(name) = provider else {
        return None;
    };
    let cmd = provider_manager.get(name).map(|c| c.command.clone())?;
    let bridge = bridge_for_command(&cmd)?;
    let missing = piki_core::cli_agent::missing_prerequisites(bridge);
    if missing.is_empty() {
        return None;
    }
    Some((
        bridge.label().to_string(),
        missing.into_iter().map(String::from).collect(),
    ))
}

/// Fire-and-forget save of the current workspace list under `source`'s key.
/// `save_workspaces` only touches rows whose `source_repo` matches, so callers
/// must pass the repo of the workspace that changed.
///
/// The save is detached, so there is no `&mut App` left to toast with — but a
/// failure here means the user's workspaces silently vanish on the next start,
/// which is far too quiet a way to lose data. `error!` at least surfaces it in
/// the Logs pane and the log file.
pub(crate) fn persist_workspaces(app: &app::App, source: std::path::PathBuf) {
    let infos = app.persistable_workspaces();
    let storage = Arc::clone(&app.storage);
    tokio::spawn(async move {
        if let Err(e) = storage.workspaces.save_workspaces(&source, &infos) {
            tracing::error!(
                source = %source.display(),
                error = %e,
                "failed to persist workspaces — changes will be lost on restart"
            );
        }
    });
}

/// Spawn a new tab with the given provider in a workspace.
/// For Custom providers, `provider_manager` is used to resolve the command and prompt args.
///
/// Returns the new tab's index plus, when the PTY failed to start, a message
/// describing why. The tab is kept either way so the user has something to
/// close; callers MUST surface the message — a blank dead tab with no
/// explanation is the single most confusing failure this app can produce
/// (a missing or non-executable provider binary looks identical to a hang).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_tab(
    ws: &mut app::Workspace,
    provider: &AIProvider,
    rows: u16,
    cols: u16,
    prompt: Option<&str>,
    provider_manager: Option<&piki_core::providers::ProviderManager>,
    paths: &piki_core::paths::DataPaths,
    output_signal: piki_core::pty::PtyOutputSignal,
) -> (usize, Option<String>) {
    // Resolve the provider's `providers.toml` entry up front so its
    // per-provider idle knobs drive the tab's IdleWatcher (re-used below for
    // command/arg resolution).
    let provider_cfg = if let AIProvider::Custom(name) = provider {
        provider_manager.and_then(|m| m.get(name))
    } else {
        None
    };
    let idx = ws.add_tab(provider.clone(), true, provider_cfg);
    // Kanban / CodeReview / Api render from app state, not a PTY.
    if *provider == AIProvider::Kanban || *provider == AIProvider::CodeReview {
        return (idx, None);
    }
    if *provider == AIProvider::Api {
        ws.tabs[idx].api_state = Some(app::ApiTabState::new());
        return (idx, None);
    }

    // Resolve command and args: use ProviderManager for Custom providers, built-in methods otherwise
    let (cmd, args) = if let AIProvider::Custom(name) = provider {
        if let Some(mgr) = provider_manager
            && let Some(config) = mgr.get(name)
        {
            let prompt_args = prompt
                .map(|p| piki_core::providers::ProviderManager::prompt_args(config, p))
                .unwrap_or_default();
            let mut all_args = config.default_args.clone();
            all_args.extend(prompt_args);
            (config.command.clone(), all_args)
        } else {
            return (
                idx,
                Some(format!("No provider configured named '{name}'")),
            );
        }
    } else {
        let cmd = provider.resolved_command();
        let prompt_args = prompt
            .map(|p| provider.prompt_args(p))
            .unwrap_or_default();
        (cmd, prompt_args)
    };

    // Shell tabs get OSC 133/7 shell integration. Provider tabs whose binary
    // has a hook bridge (Claude Code, Antigravity) get the structured
    // cli-agent channel. Both ride the same OSC parser, so both enable
    // `integration_on`. Everything else runs bare.
    let bridge = match provider {
        AIProvider::Custom(_) => bridge_for_command(&cmd),
        _ => None,
    };
    let (extra_env, extra_args, integration_on, cli_agent_sock) =
        if *provider == AIProvider::Shell {
            match shell_install::setup_for(&cmd, &paths.shell_integration_dir()) {
                Ok(Some(setup)) => {
                    let mut env: Vec<(String, String)> = setup.env.into_iter().collect();
                    // Also wire the cli-agent channel so a manually-typed
                    // `claude` inside this shell reports to the Agents pane:
                    // the FIFO + hook env ride the shell's environment, and
                    // the bridge script wraps `claude` with `--settings`.
                    // Only the env is merged — the `--settings` extra_args
                    // are claude args, not shell args.
                    let sock = match cli_agent_install::setup_for_claude(&paths.claude_hooks_dir())
                    {
                        Ok(agent) => {
                            env.extend(agent.env);
                            agent.sock_path
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "cli-agent channel skipped for shell tab");
                            None
                        }
                    };
                    (env, setup.extra_args, true, sock)
                }
                Ok(None) => (Vec::new(), Vec::new(), false, None),
                Err(e) => {
                    tracing::warn!(error = %e, shell = %cmd, "shell integration setup failed");
                    (Vec::new(), Vec::new(), false, None)
                }
            }
        } else if bridge == Some(AgentBridge::Claude) {
            match cli_agent_install::setup_for_claude(&paths.claude_hooks_dir()) {
                Ok(setup) => {
                    let sock = setup.sock_path.clone();
                    let env: Vec<(String, String)> = setup.env.into_iter().collect();
                    (env, setup.extra_args, true, sock)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "claude cli-agent hook setup failed");
                    (Vec::new(), Vec::new(), false, None)
                }
            }
        } else if bridge == Some(AgentBridge::Antigravity) {
            // No extra_args: agy discovers the bridge from its own plugins
            // root, so the hooks ride the environment alone.
            match agy_install::setup_for_antigravity(
                &paths.antigravity_hooks_dir(),
                &agy_install::plugins_root(),
            ) {
                Ok(setup) => {
                    let sock = setup.sock_path.clone();
                    let env: Vec<(String, String)> = setup.env.into_iter().collect();
                    (env, Vec::new(), true, sock)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "antigravity cli-agent hook setup failed");
                    (Vec::new(), Vec::new(), false, None)
                }
            }
        } else if piki_core::agent_state_detect::manifest_for_command(&cmd).is_some() {
            // No hook bridge for this provider (e.g. Codex) — turn on shell
            // integration so `OscParser` captures its window-title spinner,
            // but withhold `cli_agent_sock`: that FIFO is exclusive to the
            // real hook bridges above.
            (Vec::new(), Vec::new(), true, None)
        } else {
            (Vec::new(), Vec::new(), false, None)
        };

    let spawn_error = match PtySession::spawn(
        &ws.path,
        rows,
        cols,
        &cmd,
        &args,
        &extra_env,
        &extra_args,
        integration_on,
        cli_agent_sock,
        Some(output_signal),
    )
    .await
    {
        Ok(session) => {
            ws.tabs[idx].pty_parser = Some(Arc::clone(session.parser()));
            ws.tabs[idx].pty_session = Some(session);
            ws.status = app::WorkspaceStatus::Busy;
            None
        }
        Err(e) => {
            // Usually a missing or non-executable provider binary. Without
            // this the tab just sits there blank forever.
            tracing::warn!(%cmd, error = %e, "failed to spawn PTY for tab");
            Some(format!("Could not start '{cmd}': {e}"))
        }
    };
    // A tool (shell/agent/git) just activated for this workspace — that's
    // the trigger for inferring its branch (never persisted, see
    // `ws.branch`), so kick the background refresh loop immediately instead
    // of waiting for its next periodic tick.
    ws.dirty = true;
    ws.last_refresh = None;
    (idx, spawn_error)
}

/// Probe the actual scrollback buffer size by setting a large offset and reading back.
/// `scrollback()` returns the current offset (which is always 0 after render reset),
/// so we temporarily set it to MAX, read the clamped value, then restore to 0.
pub(crate) fn scrollback_max(parser: &Arc<parking_lot::Mutex<vt100::Parser>>) -> usize {
    let mut guard = parser.lock();
    guard.screen_mut().set_scrollback(usize::MAX);
    let max = guard.screen().scrollback();
    guard.screen_mut().set_scrollback(0);
    max
}

pub(crate) fn copy_visible_terminal(app: &mut App) {
    if let Some(ws) = app.workspaces.get(app.active_workspace)
        && let Some(tab) = ws.current_tab()
        && let Some(ref parser) = tab.pty_parser
    {
        let mut guard = parser.lock();
        guard.screen_mut().set_scrollback(tab.term_scroll);
        let text = guard.screen().contents();
        guard.screen_mut().set_scrollback(0);
        drop(guard);
        match clipboard::copy_to_clipboard(&text) {
            Ok(()) => {
                app.status_message = Some("Terminal content copied".into());
            }
            Err(e) => {
                app.status_message = Some(format!("Copy failed: {}", e));
            }
        }
    }
}

/// Recompute PTY dimensions after sidebar resize and resize all PTY sessions
pub(crate) fn resize_all_ptys(app: &mut App) {
    // We need the current terminal size; use cached pty dimensions as proxy
    // The actual resize will happen on next render via compute_terminal_area_with
    // For now, just mark that a resize is needed — the Event::Resize path handles actual PTY resize
    // But we can estimate from the stored terminal_inner_area
    if let Some(inner) = app.terminal_inner_area {
        // Rough estimate: total width = inner.width / old_main_pct * 100
        let total_width = inner.x + inner.width + 2; // approximate
        let total_height = inner.y + inner.height + 4; // approximate (tabs+subtabs+status+footer+borders)
        let new_area = ui::layout::compute_terminal_area_with(
            Rect::new(0, 0, total_width, total_height),
            app.sidebar_pct,
        );
        app.pty_rows = new_area.height;
        app.pty_cols = new_area.width;
        for ws in &mut app.workspaces {
            for tab in &mut ws.tabs {
                if let Some(ref mut pty) = tab.pty_session {
                    let _ = pty.resize(new_area.height, new_area.width);
                }
            }
        }
    }
}

/// Check if a point (col, row) is inside a Rect
pub(crate) fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// What a click on the tab bar landed on.
pub(crate) enum SubtabHit {
    /// Tab index + whether the close button (`×`) was hit.
    Tab(usize, bool),
    /// The trailing `+` (new tab) button.
    NewTab,
}

/// Calculate what was clicked in the tab bar
pub(crate) fn subtab_index_at(app: &App, col: u16, area: Rect) -> Option<SubtabHit> {
    let ws = app.current_workspace()?;
    let mut x = area.x;
    for (i, tab) in ws.tabs.iter().enumerate() {
        let label = tab
            .markdown_label
            .as_deref()
            .unwrap_or(tab.provider.label());
        // Matches subtabs.rs: " N" (2, if i < 9) + " icon " (3) + label
        // + " g" (2, if agent glyph) + " ×" (2, if closable) + " " (1);
        // blocks separated by a 1-col gap
        let mut tab_display_width = label.len() as u16 + 4;
        if i < 9 {
            tab_display_width += 2;
        }
        // The glyph is only rendered when the status is *actionable*, not for
        // every tab that has a cli-agent snapshot — mirror subtabs.rs exactly
        // or the close-button hit region drifts right of the visible `×`.
        if let Some((status, attention, _)) = tab.cli_agent_snapshot()
            && crate::ui::actionable_status_view(&app.theme, status, attention).is_some()
        {
            tab_display_width += 2;
        }
        if tab.closable {
            tab_display_width += 2;
        }
        if col >= x && col < x + tab_display_width {
            // The block ends with " ×" (2 cols) then a trailing space (1 col).
            // The close target is just those two `" ×"` columns; excluding the
            // trailing space keeps a click in the padding from closing the tab.
            let on_close = tab.closable
                && col >= x + tab_display_width - 3
                && col < x + tab_display_width - 1;
            return Some(SubtabHit::Tab(i, on_close));
        }
        x += tab_display_width + 1; // +1 for the gap between blocks
    }
    // Trailing " + " button right after the last tab's gap
    if col >= x && col < x + 3 {
        return Some(SubtabHit::NewTab);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{add_test_workspace, test_app};

    fn bogus_provider(name: &str, command: &str) -> piki_core::providers::ProviderConfig {
        piki_core::providers::ProviderConfig {
            name: name.to_string(),
            description: String::new(),
            command: command.to_string(),
            default_args: Vec::new(),
            prompt_format: piki_core::providers::PromptFormat::Positional,
            dispatchable: false,
            agent_dir: None,
            idle_threshold_secs: None,
            idle_notify: false,
            icon: None,
        }
    }

    /// A tab whose provider isn't configured must say so, not sit there blank.
    #[tokio::test]
    async fn spawn_tab_reports_an_unknown_provider() {
        let mut app = test_app();
        let ws_idx = add_test_workspace(&mut app);
        let paths = piki_core::paths::DataPaths::default_paths();
        let signal = piki_core::pty::PtyOutputSignal::new();

        let (idx, err) = spawn_tab(
            &mut app.workspaces[ws_idx],
            &AIProvider::Custom("does-not-exist".into()),
            24,
            80,
            None,
            None,
            &paths,
            signal,
        )
        .await;

        assert_eq!(idx, 0, "the tab is still created so the user can close it");
        let err = err.expect("an unconfigured provider must report why");
        assert!(err.contains("does-not-exist"), "{err}");
    }

    /// The headline case: a provider whose binary doesn't exist used to leave
    /// a blank, dead tab with no message at all.
    #[tokio::test]
    async fn spawn_tab_reports_a_missing_binary() {
        let mut app = test_app();
        let ws_idx = add_test_workspace(&mut app);
        app.provider_manager.upsert(bogus_provider(
            "ghost",
            "/nonexistent/definitely-not-a-real-binary",
        ));
        let paths = piki_core::paths::DataPaths::default_paths();
        let signal = piki_core::pty::PtyOutputSignal::new();

        let (_idx, err) = spawn_tab(
            &mut app.workspaces[ws_idx],
            &AIProvider::Custom("ghost".into()),
            24,
            80,
            None,
            Some(&app.provider_manager),
            &paths,
            signal,
        )
        .await;

        let err = err.expect("a missing binary must report why");
        assert!(
            err.contains("definitely-not-a-real-binary"),
            "the message must name the command: {err}"
        );
    }

    /// Kanban/CodeReview/Api tabs render from app state and never spawn a
    /// PTY — they must not be reported as failures.
    #[tokio::test]
    async fn spawn_tab_succeeds_for_ptyless_providers() {
        let mut app = test_app();
        let ws_idx = add_test_workspace(&mut app);
        let paths = piki_core::paths::DataPaths::default_paths();

        for provider in [AIProvider::Kanban, AIProvider::CodeReview, AIProvider::Api] {
            let (_idx, err) = spawn_tab(
                &mut app.workspaces[ws_idx],
                &provider,
                24,
                80,
                None,
                None,
                &paths,
                piki_core::pty::PtyOutputSignal::new(),
            )
            .await;
            assert!(err.is_none(), "{provider:?} should not report an error");
        }
    }
}
