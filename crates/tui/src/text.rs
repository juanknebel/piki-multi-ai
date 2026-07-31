//! Char-boundary-safe text measurement, wrapping and windowing.
//!
//! Every function here exists because the obvious byte-slicing version
//! panics. `&s[..n]` where `n` came from a terminal column count is a crash
//! waiting for the first accented character — and agent output is full of
//! them. Ratatui lays out by *display width*, so widths here are measured
//! with `unicode-width` rather than counting chars, which also keeps CJK and
//! emoji from overflowing the pane they were wrapped for.

use unicode_width::UnicodeWidthChar;

/// Byte index of the char boundary at or before `idx`.
pub fn floor_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Byte index of the char boundary at or after `idx`.
pub fn ceil_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Truncate to at most `max_bytes`, snapping down to a char boundary.
/// For cutting arbitrary external text (tool output, PTY bytes) down to a
/// display-sized chunk where the exact cut point doesn't matter.
pub fn truncate_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    &s[..floor_boundary(s, max_bytes)]
}

/// Display width of `s` in terminal columns.
pub fn width(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Byte index at which `s` reaches `target` columns of display width.
/// Returns `s.len()` when the whole string is narrower than `target`.
fn byte_at_width(s: &str, target: usize) -> usize {
    let mut cols = 0;
    for (i, c) in s.char_indices() {
        let w = c.width().unwrap_or(0);
        if cols + w > target {
            return i;
        }
        cols += w;
    }
    s.len()
}

/// Word-wrap `text` into chunks at most `width` columns wide.
///
/// Breaks on the last space that fits; falls back to a hard break mid-word
/// when a single word is wider than the line. Never splits a char.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    let mut remaining = text;
    while self::width(remaining) > width {
        let hard = byte_at_width(remaining, width);
        // Prefer a word break, but only one that leaves a non-empty line —
        // a leading space would otherwise break at 0 and loop forever.
        let break_at = match remaining[..hard].rfind(' ') {
            Some(p) if p > 0 => p + 1,
            _ => hard,
        };
        // `hard` is 0 only when the first char alone exceeds `width`; take it
        // whole rather than emitting an empty line and spinning.
        let break_at = if break_at == 0 {
            ceil_boundary(remaining, 1)
        } else {
            break_at
        };
        result.push(remaining[..break_at].to_string());
        remaining = &remaining[break_at..];
    }
    result.push(remaining.to_string());
    result
}

/// A `(before, cursor_char, after)` split of `text` for rendering a single-line
/// input field that scrolls to keep the cursor visible in `max_width` columns.
///
/// `cursor` is a **byte** offset (that is how the input handlers track it) and
/// is snapped to a char boundary, so a stale or mid-char cursor degrades into a
/// slightly-off caret instead of a panic.
pub fn field_window(text: &str, cursor: usize, max_width: usize) -> (&str, &str, &str) {
    let cursor = floor_boundary(text, cursor);
    if max_width == 0 {
        return ("", " ", "");
    }
    // Scroll so the cursor sits at the right edge once the text outgrows the
    // field: walk back `max_width` columns from the cursor.
    let head = &text[..cursor];
    let start = if width(head) > max_width {
        let mut cols = 0;
        let mut start = cursor;
        for (i, c) in head.char_indices().rev() {
            cols += c.width().unwrap_or(0);
            if cols > max_width {
                break;
            }
            start = i;
        }
        start
    } else {
        0
    };
    let before = &text[start..cursor];

    // Columns left for the caret and whatever follows it.
    let tail_budget = max_width.saturating_sub(width(before));
    // `byte_at_width` returns an offset from `char_indices`, so `end` is always
    // a char boundary and is never below `cursor`.
    let end = cursor + byte_at_width(&text[cursor..], tail_budget);

    // Past the end of the text the caret is a blank cell and there is no tail.
    if cursor >= text.len() {
        return (before, " ", "");
    }
    let after_start = ceil_boundary(text, cursor + 1);
    let cursor_char = &text[cursor..after_start];
    let after = &text[after_start.min(end)..end.max(after_start)];
    (before, cursor_char, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact string that used to crash the TUI: at 10 columns the byte cut
    // lands inside the 'ó' of "instalación".
    #[test]
    fn wrap_does_not_panic_mid_codepoint() {
        let s = "instalación completada correctamente en el sistema";
        let out = wrap(s, 10);
        assert!(out.iter().all(|l| width(l) <= 10), "{out:?}");
        assert_eq!(out.concat(), s, "wrapping must preserve the text");
    }

    #[test]
    fn wrap_breaks_on_words_when_it_can() {
        let out = wrap("hola mundo cruel", 11);
        assert_eq!(out, vec!["hola mundo ", "cruel"]);
    }

    #[test]
    fn wrap_hard_breaks_a_word_longer_than_the_line() {
        let out = wrap("supercalifragilistico", 5);
        assert!(out.iter().all(|l| width(l) <= 5), "{out:?}");
        assert_eq!(out.concat(), "supercalifragilistico");
    }

    #[test]
    fn wrap_terminates_on_a_char_wider_than_the_line() {
        // A double-width char in a 1-column line must still make progress.
        let out = wrap("日本語", 1);
        assert_eq!(out.concat(), "日本語");
    }

    #[test]
    fn wrap_handles_leading_spaces_without_looping() {
        let out = wrap("   indented text here", 6);
        assert_eq!(out.concat(), "   indented text here");
    }

    #[test]
    fn wrap_zero_width_returns_input() {
        assert_eq!(wrap("abc", 0), vec!["abc"]);
    }

    #[test]
    fn truncate_bytes_snaps_below_a_multibyte_char() {
        // byte 10 is inside 'ó'
        let s = "instalación";
        let t = truncate_bytes(s, 10);
        assert_eq!(t, "instalaci");
        assert!(s.starts_with(t));
    }

    #[test]
    fn truncate_bytes_keeps_short_strings_whole() {
        assert_eq!(truncate_bytes("hola", 500), "hola");
    }

    #[test]
    fn field_window_never_panics_on_accents() {
        let text = "configuración avanzada del sistema";
        // Sweep every byte offset, including the ones inside multibyte chars.
        for cursor in 0..=text.len() {
            let (before, cur, after) = field_window(text, cursor, 10);
            assert!(width(before) <= 10, "cursor={cursor}");
            assert!(!cur.is_empty());
            let _ = after;
        }
    }

    #[test]
    fn field_window_shows_the_end_while_typing() {
        let text = "abcdefghijklmno";
        let (before, cur, _after) = field_window(text, text.len(), 5);
        assert_eq!(cur, " ", "cursor past the end renders as a blank caret");
        assert_eq!(before, "klmno");
    }

    #[test]
    fn field_window_on_empty_text() {
        let (before, cur, after) = field_window("", 0, 10);
        assert_eq!((before, cur, after), ("", " ", ""));
    }
}
