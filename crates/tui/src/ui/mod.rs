pub(crate) mod api;
pub(crate) mod chat;
pub(crate) mod code_review;
pub mod command_palette;
pub(crate) mod dialogs;
pub mod editor;
pub mod fuzzy;
pub mod layout;
pub mod markdown;
mod panels;
mod sidebar;
pub(crate) mod scrollbar;
pub(crate) mod statusbar;
pub mod subtabs;
pub mod terminal;
pub(crate) mod which_key;
pub mod workspace_switcher;

/// Braille frames for the running-activity spinner (~100ms per frame at the
/// 50ms event-loop tick; see `App::spinner_frame`).
pub(crate) const SPINNER_FRAMES: [&str; 10] =
    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Glyph, short label, and color for a Claude agent status — the single
/// source of the status vocabulary (mirrors the desktop `cliAgentStatusView`).
/// Used by the surfaces that *list* agents: the Agents pane, the dashboard,
/// and the workspace switcher. Ambient chrome (tab bar, workspace list,
/// status bar) must go through `actionable_status_view` instead, so activity
/// only ever animates in one place.
pub(crate) fn cli_agent_status_view(
    app: &crate::app::App,
    status: piki_core::cli_agent::CliAgentStatus,
    attention: bool,
) -> (&'static str, &'static str, ratatui::style::Color) {
    use piki_core::cli_agent::CliAgentStatus as S;
    let t = &app.theme.status;
    match status {
        S::Running => (
            SPINNER_FRAMES[(app.spinner_frame / 2) % SPINNER_FRAMES.len()],
            "running",
            t.running,
        ),
        S::WaitingPermission => ("⚠", "permission", t.needs_you),
        // Idle only shouts when it has news you haven't seen; a freshly
        // started or already-viewed agent sits quiet at the prompt.
        S::Idle if attention => ("●", "needs you", t.needs_you),
        S::Idle => ("●", "idle", t.exited),
        S::Done => ("✓", "done", t.done),
    }
}

/// Status glyph for ambient chrome (tab bar, workspace-list rollup). Only
/// actionable states surface here — running/done stay in the Agents pane;
/// the glyph keeps its semantic color even on an accent background.
pub(crate) fn actionable_status_view(
    theme: &crate::theme::Theme,
    status: piki_core::cli_agent::CliAgentStatus,
    attention: bool,
) -> Option<(&'static str, ratatui::style::Color)> {
    use piki_core::cli_agent::CliAgentStatus as S;
    match status {
        S::WaitingPermission => Some(("⚠", theme.status.needs_you)),
        // "Has news you haven't seen" propagates; quiet idle/done doesn't.
        S::Idle | S::Done if attention => Some(("●", theme.status.needs_you)),
        _ => None,
    }
}

/// Fallback liveness indicator for a tab without OSC 777 agent state.
/// Shared by the dashboard and the Agents pane.
pub(crate) fn agent_tab_indicator(
    app: &crate::app::App,
    tab: &crate::app::Tab,
) -> (&'static str, &'static str, ratatui::style::Color) {
    let t = &app.theme.status;
    let alive = tab.pty_session.as_ref().is_some_and(|p| p.peek_alive());
    if alive {
        ("●", "alive", t.running)
    } else if tab.pty_session.is_some() {
        ("○", "exited", t.exited)
    } else {
        ("—", "not started", t.exited)
    }
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::dialog_state::{DialogState, NewTabMenu};
    use crate::test_support::{buffer_to_snapshot, test_storage, test_terminal};
    use crate::theme::Theme;

    // ── Existing tests converted to insta snapshots ──

    #[test]
    fn test_render_confirm_quit_dialog() {
        let mut terminal = test_terminal(80, 24);
        let app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        terminal
            .draw(|frame| {
                super::dialogs::render_confirm_quit_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("confirm_quit_dialog", content);
    }

    #[test]
    fn test_render_confirm_close_tab_dialog() {
        let mut terminal = test_terminal(80, 24);
        let app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        terminal
            .draw(|frame| {
                super::dialogs::render_confirm_close_tab_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("confirm_close_tab_dialog", content);
    }

    #[test]
    fn test_render_new_tab_dialog() {
        let mut terminal = test_terminal(80, 24);
        let app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        terminal
            .draw(|frame| {
                super::dialogs::render_new_tab_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("new_tab_dialog", content);
    }

    #[test]
    fn test_render_status_bar_normal_no_workspace() {
        let mut terminal = test_terminal(80, 1);
        let app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        terminal
            .draw(|frame| {
                super::statusbar::render_status_bar(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("status_bar_normal_no_workspace", content);
    }

    #[test]
    fn test_render_status_bar_prefix_pending() {
        let mut terminal = test_terminal(80, 1);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.input_state = crate::app::InputState::PrefixPending;
        terminal
            .draw(|frame| {
                super::statusbar::render_status_bar(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("status_bar_prefix_pending", content);
    }

    #[test]
    fn test_render_footer_from_keys_single_line() {
        let mut terminal = test_terminal(80, 1);
        let theme = Theme::default();
        let keys = vec![("q".to_string(), "quit"), ("?".to_string(), "help")];
        terminal
            .draw(|frame| {
                super::statusbar::render_footer_from_keys(frame, frame.area(), &keys, &theme);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("footer_from_keys_single_line", content);
    }

    #[test]
    fn test_render_footer_wraps_on_narrow_terminal() {
        let mut terminal = test_terminal(30, 2);
        let theme = Theme::default();
        let keys = vec![
            ("hjkl".to_string(), "navigate"),
            ("enter".to_string(), "interact"),
            ("n".to_string(), "new ws"),
            ("q".to_string(), "quit"),
            ("?".to_string(), "help"),
        ];
        terminal
            .draw(|frame| {
                super::statusbar::render_footer_from_keys(frame, frame.area(), &keys, &theme);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("footer_wraps_on_narrow_terminal", content);
    }

    #[test]
    fn test_render_which_key_overlay() {
        let mut terminal = test_terminal(80, 24);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Pin the platform: on macOS the prefix would render as "Cmd-g".
        app.config.platform = crate::config::Platform::Linux;
        terminal
            .draw(|frame| {
                super::which_key::render(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("which_key_overlay", content);
    }

    // ── New snapshot tests for dialogs ──

    fn test_ws_info(name: &str, group: Option<&str>, order: u32) -> piki_core::WorkspaceInfo {
        piki_core::WorkspaceInfo {
            name: name.to_string(),
            path: std::path::PathBuf::from(format!("/tmp/{name}")),
            branch: name.to_string(),
            workspace_type: piki_core::WorkspaceType::Worktree,
            description: String::new(),
            prompt: String::new(),
            kanban_path: None,
            group: group.map(String::from),
            order,
            source_repo: std::path::PathBuf::from("/tmp/src"),
            source_repo_display: String::new(),
            dispatch_card_id: None,
            dispatch_source_kanban: None,
            dispatch_agent_name: None,
            origin: piki_core::WorkspaceOrigin::default(),
        }
    }

    #[test]
    fn test_snapshot_workspace_list_single_line_rows() {
        let mut terminal = test_terminal(40, 10);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());

        let mut a = crate::app::Workspace::from_info(test_ws_info("nightly", Some("piki"), 0));
        a.changed_files.push(piki_core::ChangedFile {
            path: "src/main.rs".to_string(),
            status: piki_core::FileStatus::Modified,
        });
        a.changed_files.push(piki_core::ChangedFile {
            path: "src/lib.rs".to_string(),
            status: piki_core::FileStatus::Modified,
        });
        a.ahead_behind = Some((1, 2));
        app.workspaces.push(a);
        app.workspaces
            .push(crate::app::Workspace::from_info(test_ws_info(
                "void-setup",
                Some("ricing"),
                1,
            )));
        app.workspaces
            .push(crate::app::Workspace::from_info(test_ws_info(
                "x220t",
                Some("ricing"),
                2,
            )));
        app.active_workspace = 0;
        app.selected_sidebar_row = 1;

        terminal
            .draw(|frame| {
                super::sidebar::render_workspace_list(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("workspace_list_single_line_rows", content);
    }

    #[test]
    fn test_snapshot_agents_pane_with_rows() {
        let mut terminal = test_terminal(40, 8);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let info = piki_core::WorkspaceInfo {
            name: "demo-ws".to_string(),
            path: std::path::PathBuf::from("/tmp/demo"),
            branch: "main".to_string(),
            workspace_type: piki_core::WorkspaceType::Simple,
            description: String::new(),
            prompt: String::new(),
            kanban_path: None,
            group: None,
            order: 0,
            source_repo: std::path::PathBuf::from("/tmp/demo"),
            source_repo_display: String::new(),
            dispatch_card_id: None,
            dispatch_source_kanban: None,
            dispatch_agent_name: None,
            origin: piki_core::WorkspaceOrigin::default(),
        };
        let mut ws = crate::app::Workspace::from_info(info);
        ws.add_tab(piki_core::AIProvider::Custom("Claude".to_string()), true, None);
        app.workspaces.push(ws);
        terminal
            .draw(|frame| {
                super::sidebar::render_agents_pane(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("agents_pane_with_rows", content);
    }

    #[test]
    fn test_snapshot_tab_bar_solid_blocks() {
        let mut terminal = test_terminal(60, 2);
        let app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        let mut ws = crate::app::Workspace::from_info(test_ws_info("demo", None, 0));
        ws.add_tab(piki_core::AIProvider::Custom("Claude".to_string()), true, None);
        ws.add_tab(piki_core::AIProvider::Shell, true, None);
        ws.active_tab = 0;
        terminal
            .draw(|frame| {
                super::subtabs::render(frame, frame.area(), &ws, &app.theme);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("tab_bar_solid_blocks", content);
    }

    #[test]
    fn test_snapshot_workspace_switcher_tree() {
        let mut terminal = test_terminal(70, 16);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());

        let mut a = crate::app::Workspace::from_info(test_ws_info("piki-nightly", Some("piki"), 0));
        a.add_tab(piki_core::AIProvider::Shell, true, None);
        a.add_tab(piki_core::AIProvider::Custom("Claude".to_string()), true, None);
        app.workspaces.push(a);

        let mut b = crate::app::Workspace::from_info(test_ws_info("bob-the-builder", None, 1));
        b.add_tab(piki_core::AIProvider::Custom("Claude".to_string()), true, None);
        app.workspaces.push(b);

        app.active_workspace = 0;
        app.workspace_switcher =
            Some(crate::workspace_switcher::create_state(&app.workspaces));

        terminal
            .draw(|frame| {
                super::workspace_switcher::render(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("workspace_switcher_tree", content);
    }

    #[test]
    fn test_snapshot_confirm_delete_dialog() {
        let mut terminal = test_terminal(80, 24);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::ConfirmDelete { target: 0 });
        terminal
            .draw(|frame| {
                super::dialogs::render_confirm_delete_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("confirm_delete_dialog", content);
    }

            #[test]
    fn test_snapshot_help_overlay() {
        let mut terminal = test_terminal(80, 40);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Force Linux so the snapshot is stable across CI runners. On macOS
        // the footer renders `cmd-*` instead of `ctrl-*`.
        app.config.platform = crate::config::Platform::Linux;
        app.active_dialog = Some(DialogState::Help { scroll: 0, filter: String::new() });
        terminal
            .draw(|frame| {
                super::dialogs::render_help_overlay(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("help_overlay", content);
    }

    /// The rendered overlay only shows the first screenful, so snapshot the full
    /// derived body too — otherwise a catalog entry could vanish from the help
    /// and no test would notice.
    #[test]
    fn test_snapshot_help_body() {
        // Force Linux so the snapshot is stable across CI runners — on macOS the
        // key grammar renders `cmd-` instead of `ctrl-`.
        let cfg = crate::config::Config {
            platform: crate::config::Platform::Linux,
            ..Default::default()
        };
        let body = super::dialogs::help_lines(&cfg).join("\n");
        insta::assert_snapshot!("help_body", body);
    }

    #[test]
    fn test_snapshot_about_overlay() {
        let mut terminal = test_terminal(80, 30);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::About);
        terminal
            .draw(|frame| {
                super::dialogs::render_about_overlay(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("about_overlay", content);
    }

    #[test]
    fn test_snapshot_dashboard_overlay() {
        let mut terminal = test_terminal(80, 24);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::Dashboard {
            selected: 0,
            scroll_offset: 0,
        });
        terminal
            .draw(|frame| {
                super::dialogs::render_dashboard_overlay(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("dashboard_overlay", content);
    }

    #[test]
    fn test_snapshot_new_tab_dialog_agents_menu() {
        let mut terminal = test_terminal(80, 24);
        // Use an isolated `DataPaths` so the snapshot doesn't depend on the
        // user's real `providers.toml` (which may have been customized).
        // The default seed for a fresh providers.toml is Claude Code + Gemini.
        let tmp = tempfile::tempdir().expect("create temp dir");
        let paths = piki_core::paths::DataPaths::new(tmp.path().to_path_buf());
        let mut app = App::new(test_storage(), &paths);
        app.active_dialog = Some(DialogState::NewTab {
            menu: NewTabMenu::Agents { selected: 0 },
        });
        terminal
            .draw(|frame| {
                super::dialogs::render_new_tab_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("new_tab_dialog_agents_menu", content);
    }

    // ── Full layout snapshot test ──

    #[test]
    fn test_snapshot_empty_app_layout() {
        let mut terminal = test_terminal(80, 24);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        // Force Linux so the snapshot is stable across CI runners.
        app.config.platform = crate::config::Platform::Linux;
        terminal
            .draw(|frame| {
                super::layout::render(frame, &mut app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("empty_app_layout", content);
    }
}
