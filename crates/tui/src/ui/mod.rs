pub(crate) mod api;
pub(crate) mod code_review;
pub mod command_palette;
pub(crate) mod dialogs;
pub mod diff;
pub mod editor;
pub mod fuzzy;
pub mod layout;
pub mod markdown;
mod panels;
mod sidebar;
pub(crate) mod statusbar;
pub mod subtabs;
pub mod terminal;
pub mod workspace_switcher;

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::app::App;
    use crate::dialog_state::{DialogState, NewTabMenu};
    use crate::theme::Theme;

    fn test_storage() -> std::sync::Arc<piki_core::storage::AppStorage> {
        std::sync::Arc::new(piki_core::storage::AppStorage {
            workspaces: Box::new(piki_core::storage::json::JsonStorage),
            api_history: None,
            ui_prefs: None,
        })
    }

    fn test_terminal(w: u16, h: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(w, h)).unwrap()
    }

    /// Convert buffer to snapshot-friendly string (trimmed trailing whitespace per line).
    fn buffer_to_snapshot(buf: &ratatui::buffer::Buffer) -> String {
        let area = buf.area();
        let mut lines = Vec::new();
        for y in 0..area.height {
            let mut line = String::new();
            for x in 0..area.width {
                let cell = &buf[(x, y)];
                line.push_str(cell.symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        // Remove trailing empty lines
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

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

    // ── New snapshot tests for dialogs ──

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
    fn test_snapshot_confirm_merge_dialog() {
        let mut terminal = test_terminal(80, 24);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::ConfirmMerge);
        terminal
            .draw(|frame| {
                super::dialogs::render_confirm_merge_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("confirm_merge_dialog", content);
    }

    #[test]
    fn test_snapshot_commit_message_dialog() {
        let mut terminal = test_terminal(80, 24);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::CommitMessage {
            buffer: "feat: add snapshot tests".to_string(),
        });
        terminal
            .draw(|frame| {
                super::dialogs::render_commit_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("commit_message_dialog", content);
    }

    #[test]
    fn test_snapshot_help_overlay() {
        let mut terminal = test_terminal(80, 40);
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::Help { scroll: 0 });
        terminal
            .draw(|frame| {
                super::dialogs::render_help_overlay(frame, frame.area(), &app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("help_overlay", content);
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
        let mut app = App::new(test_storage(), &piki_core::paths::DataPaths::default_paths());
        app.active_dialog = Some(DialogState::NewTab {
            menu: NewTabMenu::Agents,
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
        terminal
            .draw(|frame| {
                super::layout::render(frame, &mut app);
            })
            .unwrap();
        let content = buffer_to_snapshot(terminal.backend().buffer());
        insta::assert_snapshot!("empty_app_layout", content);
    }
}
