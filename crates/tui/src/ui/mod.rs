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

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::app::App;
    use crate::theme::Theme;

    fn test_terminal(w: u16, h: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(w, h)).unwrap()
    }

    #[test]
    fn test_render_confirm_quit_dialog() {
        let mut terminal = test_terminal(80, 24);
        let app = App::new();
        terminal
            .draw(|frame| {
                super::dialogs::render_confirm_quit_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);
        assert!(content.contains("Quit"), "should contain 'Quit' title");
        assert!(
            content.contains("Y") && content.contains("N"),
            "should contain Y/N options"
        );
    }

    #[test]
    fn test_render_confirm_close_tab_dialog() {
        let mut terminal = test_terminal(80, 24);
        let app = App::new();
        terminal
            .draw(|frame| {
                super::dialogs::render_confirm_close_tab_dialog(frame, frame.area(), &app);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);
        assert!(content.contains("Close"), "should contain 'Close' in title");
    }

    #[test]
    fn test_render_new_tab_dialog() {
        let mut terminal = test_terminal(80, 24);
        terminal
            .draw(|frame| {
                super::dialogs::render_new_tab_dialog(frame, frame.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);
        assert!(
            content.contains("New Tab"),
            "should contain 'New Tab' title"
        );
        assert!(content.contains("Shell"), "should list Shell provider");
    }

    #[test]
    fn test_render_status_bar_normal_no_workspace() {
        let mut terminal = test_terminal(80, 1);
        let app = App::new();
        terminal
            .draw(|frame| {
                super::statusbar::render_status_bar(frame, frame.area(), &app);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);
        assert!(
            content.contains("NAVIGATE"),
            "should show NAVIGATE mode label"
        );
        assert!(
            content.contains("No active workspace"),
            "should indicate no workspace"
        );
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
        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);
        assert!(content.contains("[q]"), "should show [q] key");
        assert!(content.contains("quit"), "should show quit description");
        assert!(content.contains("[?]"), "should show [?] key");
        assert!(content.contains("help"), "should show help description");
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
        // No panic = success; the widget handles wrapping gracefully
        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);
        assert!(content.contains("navigate"), "should contain navigate");
    }

    /// Helper: flatten a TestBackend buffer to a single string for assertions.
    fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
        let area = buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                let cell = &buf[(x, y)];
                out.push_str(cell.symbol());
            }
            out.push('\n');
        }
        out
    }
}
