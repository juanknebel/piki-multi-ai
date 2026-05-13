//! Streaming parser that observes OSC 133 (prompt/command markers) and OSC 7
//! (cwd) escape sequences in a PTY byte stream and emits structured
//! [`ShellEvent`]s.
//!
//! The parser is **observe-only** — it does not modify or filter the byte
//! stream. Bytes still flow to whatever terminal emulator (vt100, xterm.js)
//! lives downstream; that emulator handles or ignores the OSC sequences as
//! usual. This keeps the integration safe even if the parser misbehaves.

use std::path::PathBuf;

use crate::shell_integration::ShellEvent;

/// Maximum bytes we'll buffer for a single OSC payload before discarding it.
/// Real OSC 133/7 payloads are tiny (≤ a few KB for very long paths). A bigger
/// buffer means a malformed sequence without a terminator can never wedge us.
const MAX_OSC_PAYLOAD: usize = 8 * 1024;

/// Streaming OSC parser. Maintains state across [`feed`](Self::feed) calls so
/// sequences can be split across PTY chunks.
pub struct OscParser {
    state: State,
    buf: Vec<u8>,
}

#[derive(Debug)]
enum State {
    /// Pass-through.
    Normal,
    /// Just saw `\x1b`.
    Esc,
    /// Inside OSC payload (after `\x1b]`).
    OscPayload,
    /// Inside OSC payload and just saw `\x1b` — might be start of ST (`\x1b\\`).
    OscMaybeSt,
}

impl OscParser {
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            buf: Vec::new(),
        }
    }

    /// Feed a chunk of bytes. Returns any structured events extracted.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ShellEvent> {
        let mut events = Vec::new();
        for &b in bytes {
            self.step(b, &mut events);
        }
        events
    }

    fn step(&mut self, b: u8, out: &mut Vec<ShellEvent>) {
        match self.state {
            State::Normal => {
                if b == 0x1b {
                    self.state = State::Esc;
                }
            }
            State::Esc => {
                if b == b']' {
                    self.buf.clear();
                    self.state = State::OscPayload;
                } else {
                    // ESC followed by something other than `]` — not OSC. We
                    // only re-enter Esc on a fresh ESC byte; everything else
                    // resets us cleanly.
                    self.state = if b == 0x1b { State::Esc } else { State::Normal };
                }
            }
            State::OscPayload => {
                if b == 0x07 {
                    // BEL terminator
                    self.flush(out);
                } else if b == 0x1b {
                    self.state = State::OscMaybeSt;
                } else {
                    self.buf.push(b);
                    if self.buf.len() > MAX_OSC_PAYLOAD {
                        // Malformed — abandon to avoid unbounded growth.
                        self.buf.clear();
                        self.state = State::Normal;
                    }
                }
            }
            State::OscMaybeSt => {
                if b == b'\\' {
                    // ST terminator (\x1b\\)
                    self.flush(out);
                } else {
                    // The \x1b was part of the payload after all (rare). Push
                    // it and the new byte, return to OSC payload.
                    self.buf.push(0x1b);
                    self.buf.push(b);
                    self.state = State::OscPayload;
                }
            }
        }
    }

    fn flush(&mut self, out: &mut Vec<ShellEvent>) {
        let payload = std::mem::take(&mut self.buf);
        self.state = State::Normal;
        if let Some(event) = parse_payload(&payload) {
            out.push(event);
        }
    }
}

impl Default for OscParser {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_payload(payload: &[u8]) -> Option<ShellEvent> {
    let s = std::str::from_utf8(payload).ok()?;
    let (id, rest) = s.split_once(';').unwrap_or((s, ""));
    match id {
        "133" => parse_osc_133(rest),
        "7" => parse_osc_7(rest),
        _ => None,
    }
}

fn parse_osc_133(rest: &str) -> Option<ShellEvent> {
    // rest is "A", "B", "C", or "D[;<exit_code>][;<aid>]"
    let (subkind, args) = rest.split_once(';').unwrap_or((rest, ""));
    match subkind {
        "A" => Some(ShellEvent::PromptStart),
        "B" => Some(ShellEvent::CommandInputStart),
        "C" => Some(ShellEvent::CommandOutputStart),
        "D" => {
            // Optional exit code is the first numeric arg.
            let exit_code = args
                .split([';', ' '])
                .next()
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse::<i32>().ok());
            Some(ShellEvent::CommandEnd { exit_code })
        }
        _ => None,
    }
}

fn parse_osc_7(rest: &str) -> Option<ShellEvent> {
    // Format: file://<host>/<path>  — host may be empty.
    let url = rest.strip_prefix("file://")?;
    let path_start = url.find('/')?;
    let raw_path = &url[path_start..];
    let decoded = percent_decode(raw_path.as_bytes());
    let path = String::from_utf8(decoded).ok()?;
    Some(ShellEvent::CwdChanged(PathBuf::from(path)))
}

fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let b = input[i];
        if b == b'%' && i + 2 < input.len() {
            let hi = hex_digit(input[i + 1]);
            let lo = hex_digit(input[i + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    out
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(input: &[u8]) -> Vec<ShellEvent> {
        let mut p = OscParser::new();
        p.feed(input)
    }

    #[test]
    fn osc_133_a_emits_prompt_start() {
        let events = parse_one(b"\x1b]133;A\x07");
        assert_eq!(events, vec![ShellEvent::PromptStart]);
    }

    #[test]
    fn osc_133_b_emits_command_input_start() {
        let events = parse_one(b"\x1b]133;B\x07");
        assert_eq!(events, vec![ShellEvent::CommandInputStart]);
    }

    #[test]
    fn osc_133_c_emits_command_output_start() {
        let events = parse_one(b"\x1b]133;C\x07");
        assert_eq!(events, vec![ShellEvent::CommandOutputStart]);
    }

    #[test]
    fn osc_133_d_with_exit_code() {
        let events = parse_one(b"\x1b]133;D;0\x07");
        assert_eq!(events, vec![ShellEvent::CommandEnd { exit_code: Some(0) }]);

        let events = parse_one(b"\x1b]133;D;127\x07");
        assert_eq!(
            events,
            vec![ShellEvent::CommandEnd { exit_code: Some(127) }]
        );
    }

    #[test]
    fn osc_133_d_without_exit_code() {
        let events = parse_one(b"\x1b]133;D\x07");
        assert_eq!(events, vec![ShellEvent::CommandEnd { exit_code: None }]);
    }

    #[test]
    fn osc_7_emits_cwd_changed() {
        let events = parse_one(b"\x1b]7;file://hostname/home/user/code\x07");
        assert_eq!(
            events,
            vec![ShellEvent::CwdChanged(PathBuf::from("/home/user/code"))]
        );
    }

    #[test]
    fn osc_7_url_decodes_path() {
        let events = parse_one(b"\x1b]7;file://host/path%20with%20space/sub\x07");
        assert_eq!(
            events,
            vec![ShellEvent::CwdChanged(PathBuf::from("/path with space/sub"))]
        );
    }

    #[test]
    fn osc_7_with_st_terminator() {
        let events = parse_one(b"\x1b]7;file://host/x\x1b\\");
        assert_eq!(events, vec![ShellEvent::CwdChanged(PathBuf::from("/x"))]);
    }

    #[test]
    fn unknown_osc_yields_no_event() {
        let events = parse_one(b"\x1b]99;some-payload\x07");
        assert!(events.is_empty());

        let events = parse_one(b"\x1b]133;Z\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn partial_bytes_across_chunks() {
        let mut p = OscParser::new();
        let full = b"\x1b]133;D;42\x07";
        let mut all = Vec::new();
        for chunk in full.chunks(1) {
            all.extend(p.feed(chunk));
        }
        assert_eq!(all, vec![ShellEvent::CommandEnd { exit_code: Some(42) }]);
    }

    #[test]
    fn partial_bytes_two_byte_chunks() {
        let mut p = OscParser::new();
        let full = b"\x1b]7;file://h/tmp\x07";
        let mut all = Vec::new();
        for chunk in full.chunks(2) {
            all.extend(p.feed(chunk));
        }
        assert_eq!(all, vec![ShellEvent::CwdChanged(PathBuf::from("/tmp"))]);
    }

    #[test]
    fn csi_passes_through_without_event() {
        // Color SGR followed by an OSC marker.
        let events = parse_one(b"\x1b[31mred\x1b[0m\x1b]133;D;0\x07");
        assert_eq!(events, vec![ShellEvent::CommandEnd { exit_code: Some(0) }]);
    }

    #[test]
    fn plain_text_emits_nothing() {
        let events = parse_one(b"hello world\nmore text");
        assert!(events.is_empty());
    }

    #[test]
    fn multiple_events_in_one_feed() {
        let events = parse_one(b"\x1b]7;file://h/a\x07prompt\x1b]133;A\x07");
        assert_eq!(
            events,
            vec![
                ShellEvent::CwdChanged(PathBuf::from("/a")),
                ShellEvent::PromptStart,
            ]
        );
    }

    #[test]
    fn unterminated_osc_does_not_grow_unbounded() {
        let mut p = OscParser::new();
        // 9 KB of garbage inside an unterminated OSC.
        let mut payload = b"\x1b]".to_vec();
        payload.extend(std::iter::repeat_n(b'A', 9 * 1024));
        let events = p.feed(&payload);
        assert!(events.is_empty());
        // After exceeding MAX_OSC_PAYLOAD the parser resets to Normal — a
        // following clean sequence should still parse.
        let events = p.feed(b"\x1b]133;A\x07");
        assert_eq!(events, vec![ShellEvent::PromptStart]);
    }

    #[test]
    fn esc_followed_by_non_bracket_does_not_break_next_osc() {
        // ESC + `M` (RI control) then a clean OSC. The CSI/RI byte resets us
        // out of Esc state without consuming the next OSC.
        let events = parse_one(b"\x1bM\x1b]133;A\x07");
        assert_eq!(events, vec![ShellEvent::PromptStart]);
    }
}
