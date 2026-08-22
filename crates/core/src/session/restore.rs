//! Restore buffers: the byte stream that rebuilds a session's terminal on a
//! freshly attached client.
//!
//! The daemon keeps a `vt100::Parser` per session purely so it can answer
//! terminal queries while nobody is attached and hand out this buffer on
//! attach. The heavy lifting lives in the vendored `vt100`
//! (`Screen::restore_formatted`): scrollback rows replayed as plain lines,
//! then the primary screen, then the alternate screen if the application is
//! in it, then cursor/attributes/input modes. Feeding the result into a new
//! parser (or a reset xterm.js) of the same size yields the same visible
//! screen *and* the same scrollback, which is what keeps `prefix [` / mouse
//! wheel history working across a restart.

/// Escape codes that rebuild `parser`'s terminal in a fresh parser of the
/// same size. See the module docs for what is carried.
pub fn restore_buffer(parser: &vt100::Parser) -> Vec<u8> {
    parser.screen().restore_formatted()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: u16 = 5;
    const COLS: u16 = 20;
    const SCROLLBACK: usize = 100;

    fn parser() -> vt100::Parser {
        vt100::Parser::new(ROWS, COLS, SCROLLBACK)
    }

    /// Replays `a`'s restore buffer into a fresh parser and returns it.
    fn replay(a: &vt100::Parser) -> vt100::Parser {
        let mut b = parser();
        b.process(&restore_buffer(a));
        b
    }

    /// Number of scrollback rows a parser holds (its max scroll offset).
    fn history_len(p: &mut vt100::Parser) -> usize {
        p.screen_mut().set_scrollback(usize::MAX);
        let n = p.screen().scrollback();
        p.screen_mut().set_scrollback(0);
        n
    }

    /// Asserts the visible screen and every scrollback offset agree.
    fn assert_same_terminal(a: &mut vt100::Parser, b: &mut vt100::Parser) {
        assert_eq!(a.screen().size(), b.screen().size());
        assert_eq!(a.screen().contents(), b.screen().contents(), "live screen");
        assert_eq!(
            a.screen().cursor_position(),
            b.screen().cursor_position(),
            "cursor"
        );
        assert_eq!(history_len(a), history_len(b), "scrollback length");
        for k in 1..=history_len(a) {
            a.screen_mut().set_scrollback(k);
            b.screen_mut().set_scrollback(k);
            assert_eq!(
                a.screen().contents(),
                b.screen().contents(),
                "scrollback offset {k}"
            );
        }
        a.screen_mut().set_scrollback(0);
        b.screen_mut().set_scrollback(0);
    }

    #[test]
    fn history_and_screen_round_trip() {
        let mut a = parser();
        for i in 1..=12 {
            a.process(format!("line {i}\r\n").as_bytes());
        }
        a.process(b"prompt> ");
        assert_eq!(
            history_len(&mut a),
            12 + 1 - usize::from(ROWS),
            "setup: text scrolled"
        );

        let mut b = replay(&a);
        assert_same_terminal(&mut a, &mut b);
    }

    #[test]
    fn attributes_survive_in_history_and_screen() {
        let mut a = parser();
        a.process(b"\x1b[31mred\x1b[m plain \x1b[1;44mbold-on-blue\x1b[m\r\n");
        for i in 0..8 {
            a.process(format!("\x1b[3{}mcolour {i}\x1b[m\r\n", i % 8).as_bytes());
        }
        a.process(b"\x1b[4munderlined tail");

        let mut b = replay(&a);
        assert_same_terminal(&mut a, &mut b);

        // The first (now scrolled-off) line kept its colours.
        let max = history_len(&mut a);
        a.screen_mut().set_scrollback(max);
        b.screen_mut().set_scrollback(max);
        for col in 0..COLS {
            let ca = a.screen().cell(0, col).unwrap();
            let cb = b.screen().cell(0, col).unwrap();
            assert_eq!(ca.fgcolor(), cb.fgcolor(), "fg at col {col}");
            assert_eq!(ca.bgcolor(), cb.bgcolor(), "bg at col {col}");
            assert_eq!(ca.bold(), cb.bold(), "bold at col {col}");
        }
        // The live row kept its underline and the pending attrs carry on:
        // writing more text to both must render identically.
        a.screen_mut().set_scrollback(0);
        b.screen_mut().set_scrollback(0);
        a.process(b" more");
        b.process(b" more");
        let (r, c) = a.screen().cursor_position();
        assert!(a.screen().cell(r, c - 1).unwrap().underline());
        assert!(b.screen().cell(r, c - 1).unwrap().underline());
    }

    #[test]
    fn wrapped_rows_rewrap_identically() {
        let mut a = parser();
        // 45 chars at 20 cols → 3 rows, the first two wrapped.
        a.process(b"0123456789abcdefghijKLMNOPQRSTUVWXYZ012345678\r\n");
        for i in 0..6 {
            a.process(format!("after {i}\r\n").as_bytes());
        }
        let mut b = replay(&a);
        // `contents()` only breaks lines at non-wrapped rows, so a wrap flag
        // that differs between the two shows up as a different string.
        assert_same_terminal(&mut a, &mut b);
    }

    #[test]
    fn wide_characters_survive() {
        let mut a = parser();
        a.process("日本語 テキスト\r\n".as_bytes());
        for i in 0..6 {
            a.process(format!("línea {i} ñ\r\n").as_bytes());
        }
        let mut b = replay(&a);
        assert_same_terminal(&mut a, &mut b);
    }

    #[test]
    fn alternate_screen_round_trip_and_return_to_primary() {
        let mut a = parser();
        for i in 0..7 {
            a.process(format!("primary {i}\r\n").as_bytes());
        }
        a.process(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[1mALT TOP\x1b[m\x1b[3;4Hcursor here");
        assert!(a.screen().alternate_screen());

        let mut b = replay(&a);
        assert!(b.screen().alternate_screen(), "alternate screen restored");
        assert_eq!(a.screen().contents(), b.screen().contents());
        assert_eq!(a.screen().cursor_position(), b.screen().cursor_position());

        // Leaving the alternate screen must reveal the same primary screen
        // (and the same history) on both.
        a.process(b"\x1b[?1049l");
        b.process(b"\x1b[?1049l");
        assert!(!a.screen().alternate_screen());
        assert!(!b.screen().alternate_screen());
        assert_same_terminal(&mut a, &mut b);
    }

    #[test]
    fn input_modes_and_cursor_visibility_survive() {
        let mut a = parser();
        a.process(b"\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[?1h\x1b[?25l");
        let b = replay(&a);
        assert!(b.screen().bracketed_paste());
        assert_eq!(
            b.screen().mouse_protocol_mode(),
            a.screen().mouse_protocol_mode()
        );
        assert_eq!(
            b.screen().mouse_protocol_encoding(),
            a.screen().mouse_protocol_encoding()
        );
        assert!(b.screen().application_cursor());
        assert!(b.screen().hide_cursor());
    }

    #[test]
    fn empty_terminal_restores_to_empty() {
        let a = parser();
        let mut b = replay(&a);
        assert_eq!(b.screen().contents(), "");
        assert_eq!(b.screen().cursor_position(), (0, 0));
        assert_eq!(history_len(&mut b), 0);
    }

    #[test]
    fn history_longer_than_receiver_scrollback_keeps_the_newest_rows() {
        let mut a = parser();
        for i in 0..60 {
            a.process(format!("row {i}\r\n").as_bytes());
        }
        let mut b = vt100::Parser::new(ROWS, COLS, 10);
        b.process(&restore_buffer(&a));
        assert_eq!(a.screen().contents(), b.screen().contents());
        assert_eq!(history_len(&mut b), 10);
        b.screen_mut().set_scrollback(10);
        assert!(
            b.screen().contents().starts_with("row 46"),
            "{:?}",
            b.screen().contents()
        );
    }
}
