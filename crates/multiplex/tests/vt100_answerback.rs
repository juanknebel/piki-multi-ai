//! Pins the vendored vt100 patch (vendor/vt100): terminal query sequences
//! (DSR, DA1) queue a response in the screen's answerback buffer, which the
//! PTY reader writes back to the application. Agent CLIs (muse) and full-screen
//! apps probe the terminal at startup and exit or hang if nothing answers.

#[test]
fn dsr_cursor_position_report() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"\x1b[5;10H\x1b[6n");
    assert_eq!(parser.screen_mut().take_answerback(), b"\x1b[5;10R");
}

#[test]
fn dsr_status_report() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"\x1b[5n");
    assert_eq!(parser.screen_mut().take_answerback(), b"\x1b[0n");
}

#[test]
fn da1_identifies_terminal() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"\x1b[c");
    assert_eq!(parser.screen_mut().take_answerback(), b"\x1b[?6c");
    // `CSI 0 c` is the explicit form of the same query.
    parser.process(b"\x1b[0c");
    assert_eq!(parser.screen_mut().take_answerback(), b"\x1b[?6c");
}

#[test]
fn take_answerback_drains() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"\x1b[6n");
    assert!(!parser.screen_mut().take_answerback().is_empty());
    assert!(parser.screen_mut().take_answerback().is_empty());
}

#[test]
fn unanswered_queries_stay_silent() {
    // Sequences we deliberately do not answer (kitty keyboard query,
    // secondary DA, OSC color queries) must not queue garbage.
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"\x1b[?u\x1b[>c\x1b]10;?\x07\x1b]11;?\x07");
    assert!(parser.screen_mut().take_answerback().is_empty());
}
