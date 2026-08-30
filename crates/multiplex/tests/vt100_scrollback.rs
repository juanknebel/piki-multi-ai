//! Pins the vendored vt100 patch (vendor/vt100): lines scrolled off a
//! top-anchored scroll region must land in scrollback, like real terminals.
//! Codex-style inline TUIs (ratatui `Terminal::insert_before`) publish their
//! transcript exactly this way — set a region `1..N` above the viewport and
//! emit newlines — so without this, wheel scrollback over a Codex tab is
//! permanently empty.

fn scrollback_max(parser: &mut vt100::Parser) -> usize {
    parser.screen_mut().set_scrollback(usize::MAX);
    let max = parser.screen().scrollback();
    parser.screen_mut().set_scrollback(0);
    max
}

#[test]
fn top_anchored_region_scroll_feeds_scrollback() {
    let mut parser = vt100::Parser::new(10, 40, 1000);
    for i in 0..10 {
        parser.process(format!("\x1b[{};1Hrow{i}", i + 1).as_bytes());
    }
    // codex insert_history_lines pattern: region rows 1-5, cursor at the
    // region bottom, newlines push history lines through the top.
    parser.process(b"\x1b[1;5r\x1b[5;1H");
    for i in 0..8 {
        parser.process(format!("\r\nhistory{i}").as_bytes());
    }
    parser.process(b"\x1b[r");

    let max = scrollback_max(&mut parser);
    assert_eq!(max, 8, "8 lines scrolled off the top-anchored region");

    parser.screen_mut().set_scrollback(max);
    let deepest = parser.screen().contents();
    assert!(
        deepest.starts_with("row0"),
        "deepest scrollback should hold the first pushed-out line, got: {deepest:?}"
    );
}

#[test]
fn non_top_anchored_region_scroll_is_discarded() {
    let mut parser = vt100::Parser::new(10, 40, 1000);
    for i in 0..10 {
        parser.process(format!("\x1b[{};1Hrow{i}", i + 1).as_bytes());
    }
    // Region NOT anchored at the top row: scrolled-off lines must not
    // pollute scrollback (matches real-terminal behavior).
    parser.process(b"\x1b[3;7r\x1b[7;1H");
    for i in 0..5 {
        parser.process(format!("\r\nmid{i}").as_bytes());
    }
    parser.process(b"\x1b[r");

    assert_eq!(scrollback_max(&mut parser), 0);
}
