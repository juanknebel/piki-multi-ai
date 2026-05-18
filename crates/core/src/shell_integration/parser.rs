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

/// Soft cap on bytes captured between [`ShellEvent::CommandInputStart`] and
/// [`ShellEvent::CommandOutputStart`]. Pathological cases (vim-style line
/// editors, huge pastes) shouldn't grow this unbounded; once exceeded we
/// stop appending until the next `B` resets the buffer.
const MAX_CMD_BUF: usize = 4 * 1024;

/// Streaming OSC parser. Maintains state across [`feed`](Self::feed) calls so
/// sequences can be split across PTY chunks.
pub struct OscParser {
    state: State,
    buf: Vec<u8>,
    /// Raw bytes captured between `B` and `C` (the user-typed command, with
    /// the shell's echo / syntax-highlight ANSI codes mixed in). Cleared on
    /// `B`, finalised on `C` into [`pending_command`](Self::pending_command).
    command_buf: Vec<u8>,
    capturing_command: bool,
    /// Finalised (ANSI-stripped, trimmed) command waiting to be attached to
    /// the next [`ShellEvent::CommandEnd`]. Cleared once emitted.
    pending_command: Option<String>,
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
            command_buf: Vec::new(),
            capturing_command: false,
            pending_command: None,
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
        // Capture command-input bytes: any byte arriving while we're between
        // `B` and `C` AND not inside an OSC payload (those bytes belong to
        // the OSC sequence, not the user's command). We include `\x1b` too —
        // the ANSI stripper at finalise time handles CSI/OSC residue.
        if self.capturing_command
            && !matches!(self.state, State::OscPayload | State::OscMaybeSt)
            && self.command_buf.len() < MAX_CMD_BUF
        {
            self.command_buf.push(b);
        }
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
        let Some(event) = parse_payload(&payload) else {
            return;
        };
        match event {
            ShellEvent::CommandInputStart => {
                self.command_buf.clear();
                self.capturing_command = true;
                out.push(ShellEvent::CommandInputStart);
            }
            ShellEvent::CommandOutputStart => {
                self.capturing_command = false;
                let raw = std::mem::take(&mut self.command_buf);
                self.pending_command = finalise_command(&raw);
                out.push(ShellEvent::CommandOutputStart);
            }
            ShellEvent::CommandEnd { exit_code, .. } => {
                let command = self.pending_command.take();
                out.push(ShellEvent::CommandEnd { exit_code, command });
            }
            other => out.push(other),
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
        "777" => parse_osc_777(rest),
        _ => None,
    }
}

/// OSC 777 is shared turf (Warp's `warp://cli-agent`, urxvt `notify`, VTE…).
/// We only claim sequences whose target is exactly piki's
/// [`CLI_AGENT_TARGET`](crate::cli_agent::install::CLI_AGENT_TARGET); anything
/// else returns `None` and is left for the downstream emulator, exactly like
/// any other unknown OSC.
///
/// `rest` is `notify;<target>;<json>`. We `splitn(3)` so semicolons inside
/// the JSON body survive the framing split.
fn parse_osc_777(rest: &str) -> Option<ShellEvent> {
    let mut parts = rest.splitn(3, ';');
    if parts.next()? != "notify" {
        return None;
    }
    if parts.next()? != crate::cli_agent::install::CLI_AGENT_TARGET {
        return None;
    }
    let json = parts.next()?;
    let event = crate::cli_agent::parse_cli_agent_payload(json)?;
    Some(ShellEvent::CliAgent(event))
}

fn parse_osc_133(rest: &str) -> Option<ShellEvent> {
    // rest is "A", "B", "C", or "D[;<exit_code>][;<aid>]"
    let (subkind, args) = rest.split_once(';').unwrap_or((rest, ""));
    match subkind {
        "A" => Some(ShellEvent::PromptStart),
        "B" => Some(ShellEvent::CommandInputStart),
        "C" => Some(ShellEvent::CommandOutputStart),
        "D" => {
            // Optional exit code is the first numeric arg. `command` is
            // injected by `OscParser::flush` from `pending_command`.
            let exit_code = args
                .split([';', ' '])
                .next()
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse::<i32>().ok());
            Some(ShellEvent::CommandEnd {
                exit_code,
                command: None,
            })
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

/// Convert the raw byte buffer captured between `B` and `C` into a clean,
/// human-readable command string suitable for a notification body.
///
/// Best-effort: strips CSI/OSC escape sequences (color codes, cursor moves),
/// drops control chars, splits on CR/LF and keeps the *last* non-empty line
/// (line-editor redraws overwrite earlier states), trims whitespace, and
/// truncates to 80 visible chars. Returns `None` when the result is empty.
fn finalise_command(raw: &[u8]) -> Option<String> {
    let stripped = strip_ansi(raw);
    // CR (0x0D) is used by shells to redraw the line in place. Split on it
    // (and on \n) and take the last non-empty segment.
    let last_segment = stripped
        .split(|b: &u8| *b == b'\r' || *b == b'\n')
        .rfind(|s| !s.is_empty() && s.iter().any(|b| !b.is_ascii_whitespace()))?;
    let text = String::from_utf8_lossy(last_segment).trim().to_string();
    if text.is_empty() {
        return None;
    }
    // Truncate to keep notification bodies readable.
    const MAX_LEN: usize = 80;
    let truncated = if text.chars().count() > MAX_LEN {
        let cutoff: String = text.chars().take(MAX_LEN - 1).collect();
        format!("{cutoff}…")
    } else {
        text
    };
    Some(truncated)
}

/// Strip CSI (`\x1b[...`), OSC (`\x1b]...`), and other ESC-prefixed
/// sequences from a byte buffer. Keeps regular printable text and UTF-8
/// continuation bytes. Skips backspace (0x08) and BEL (0x07) but preserves
/// CR/LF so `finalise_command` can split on them.
fn strip_ansi(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'[' => {
                    // CSI: skip until final byte in 0x40..=0x7E.
                    i += 2;
                    while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                }
                b']' => {
                    // OSC: skip until BEL (0x07) or ST (`\x1b\\`).
                    i += 2;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Other two-byte ESC sequence.
                    i += 2;
                }
            }
        } else if b == 0x08 || b == 0x07 {
            // Backspace / BEL: drop.
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    out
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
        assert_eq!(
            events,
            vec![ShellEvent::CommandEnd {
                exit_code: Some(0),
                command: None,
            }]
        );

        let events = parse_one(b"\x1b]133;D;127\x07");
        assert_eq!(
            events,
            vec![ShellEvent::CommandEnd {
                exit_code: Some(127),
                command: None,
            }]
        );
    }

    #[test]
    fn osc_133_d_without_exit_code() {
        let events = parse_one(b"\x1b]133;D\x07");
        assert_eq!(
            events,
            vec![ShellEvent::CommandEnd {
                exit_code: None,
                command: None,
            }]
        );
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
    fn osc_777_piki_target_emits_cli_agent_event() {
        use crate::cli_agent::CliAgentEvent;
        let json = r#"{"v":1,"event":"prompt_submit","session_id":"s1"}"#;
        let seq = format!("\x1b]777;notify;piki://cli-agent;{json}\x07");
        let events = parse_one(seq.as_bytes());
        assert_eq!(
            events,
            vec![ShellEvent::CliAgent(CliAgentEvent::UserPromptSubmit {
                session_id: "s1".into()
            })]
        );
    }

    #[test]
    fn osc_777_foreign_target_is_ignored() {
        // Warp's own sequence must not be claimed by us.
        let warp = b"\x1b]777;notify;warp://cli-agent;{\"event\":\"stop\"}\x07";
        assert!(parse_one(warp).is_empty());
        // urxvt-style `OSC 777;notify;title;body` (no piki target) too.
        let urxvt = b"\x1b]777;notify;Build done;all green\x07";
        assert!(parse_one(urxvt).is_empty());
    }

    #[test]
    fn osc_777_json_with_semicolons_survives_framing_split() {
        use crate::cli_agent::CliAgentEvent;
        let json = r#"{"v":1,"event":"stop","session_id":"s","response":"a; b; c"}"#;
        let seq = format!("\x1b]777;notify;piki://cli-agent;{json}\x07");
        let events = parse_one(seq.as_bytes());
        assert_eq!(
            events,
            vec![ShellEvent::CliAgent(CliAgentEvent::Stop {
                session_id: "s".into(),
                query: None,
                response: Some("a; b; c".into()),
                transcript_path: None,
            })]
        );
    }

    #[test]
    fn osc_777_split_across_chunks() {
        use crate::cli_agent::CliAgentEvent;
        let json = r#"{"v":1,"event":"tool_complete","session_id":"s","tool_name":"Bash"}"#;
        let seq = format!("\x1b]777;notify;piki://cli-agent;{json}\x07");
        let mut p = OscParser::new();
        let mut all = Vec::new();
        for chunk in seq.as_bytes().chunks(3) {
            all.extend(p.feed(chunk));
        }
        assert_eq!(
            all,
            vec![ShellEvent::CliAgent(CliAgentEvent::PostToolUse {
                session_id: "s".into(),
                tool_name: Some("Bash".into()),
            })]
        );
    }

    #[test]
    fn osc_777_malformed_json_yields_no_event() {
        let seq = b"\x1b]777;notify;piki://cli-agent;not json at all\x07";
        assert!(parse_one(seq).is_empty());
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
        assert_eq!(
            all,
            vec![ShellEvent::CommandEnd {
                exit_code: Some(42),
                command: None,
            }]
        );
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
        assert_eq!(
            events,
            vec![ShellEvent::CommandEnd {
                exit_code: Some(0),
                command: None,
            }]
        );
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

    #[test]
    fn captures_command_text_between_b_and_c() {
        // Simulate: prompt-input-start, user types "git status", output-start,
        // command runs, output-end with exit 0.
        let mut p = OscParser::new();
        let events = p.feed(b"\x1b]133;B\x07git status\x1b]133;C\x07hello\n\x1b]133;D;0\x07");
        // CommandEnd should carry the captured command.
        let end = events
            .iter()
            .find_map(|e| match e {
                ShellEvent::CommandEnd { command, exit_code } => Some((command.clone(), *exit_code)),
                _ => None,
            })
            .expect("CommandEnd emitted");
        assert_eq!(end, (Some("git status".to_string()), Some(0)));
    }

    #[test]
    fn captures_command_strips_csi_color_codes() {
        // Shell syntax-highlights "git" in blue: `\x1b[34mgit\x1b[0m status`.
        let mut p = OscParser::new();
        let events = p.feed(
            b"\x1b]133;B\x07\x1b[34mgit\x1b[0m status\x1b]133;C\x07\x1b]133;D;0\x07",
        );
        let end = events
            .iter()
            .find_map(|e| match e {
                ShellEvent::CommandEnd { command, .. } => Some(command.clone()),
                _ => None,
            })
            .expect("CommandEnd emitted");
        assert_eq!(end, Some("git status".to_string()));
    }

    #[test]
    fn captures_command_handles_cr_redraw() {
        // Shell line editor emits CR + final rewritten line.
        let mut p = OscParser::new();
        let events = p.feed(
            b"\x1b]133;B\x07ls\rls -la\x1b]133;C\x07\x1b]133;D;0\x07",
        );
        let end = events
            .iter()
            .find_map(|e| match e {
                ShellEvent::CommandEnd { command, .. } => Some(command.clone()),
                _ => None,
            })
            .expect("CommandEnd emitted");
        // After CR-redraw, the final line is "ls -la".
        assert_eq!(end, Some("ls -la".to_string()));
    }

    #[test]
    fn command_capture_resets_between_runs() {
        let mut p = OscParser::new();
        // First command.
        p.feed(b"\x1b]133;B\x07first\x1b]133;C\x07\x1b]133;D;0\x07");
        // Second command on the same parser.
        let events = p.feed(b"\x1b]133;B\x07second\x1b]133;C\x07\x1b]133;D;0\x07");
        let cmd = events
            .iter()
            .find_map(|e| match e {
                ShellEvent::CommandEnd { command, .. } => Some(command.clone()),
                _ => None,
            })
            .expect("CommandEnd emitted");
        assert_eq!(cmd, Some("second".to_string()));
    }

    #[test]
    fn finalise_command_truncates_long_input() {
        let long = "x".repeat(200);
        let stripped = finalise_command(long.as_bytes()).expect("non-empty");
        // 80 chars max, with an ellipsis sentinel on overflow.
        assert!(stripped.chars().count() <= 80);
        assert!(stripped.ends_with('…'));
    }

    #[test]
    fn finalise_command_empty_returns_none() {
        assert!(finalise_command(b"").is_none());
        assert!(finalise_command(b"   \t  ").is_none());
        assert!(finalise_command(b"\x1b[31m\x1b[0m").is_none());
    }
}
