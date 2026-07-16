use arboard::Clipboard;
use base64::Engine;
use std::io::Write;
use std::process::{Command, Stdio};

/// OSC 52 payloads beyond this size are dropped by most terminals/multiplexers
/// (tmux caps around 74KB); avoid writing something that will just get ignored.
const OSC52_MAX_LEN: usize = 70_000;

/// Remove control characters (except newline, tab, carriage return)
/// that can cause clipboard operations to fail silently on some backends.
fn sanitize_for_clipboard(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
        .collect()
}

/// Write text to the system clipboard via the OSC 52 terminal escape sequence.
///
/// Unlike `wl-copy`/`xclip`/`arboard`, this is interpreted by the *real*
/// terminal emulator the user is looking at, even when this app is running
/// on a remote host over SSH — the local clipboard tools above can only
/// reach the remote host's clipboard, which nothing on the user's machine
/// reads. Most modern terminals (iTerm2, kitty, WezTerm, Alacritty, Ghostty,
/// and tmux passthrough) honor it identically on Linux and macOS.
fn copy_via_osc52(text: &str) -> anyhow::Result<()> {
    if text.len() > OSC52_MAX_LEN {
        anyhow::bail!("Text too large for OSC 52 clipboard write");
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let mut stderr = std::io::stderr();
    write!(stderr, "\x1b]52;c;{}\x07", encoded)?;
    stderr.flush()?;
    Ok(())
}

/// Copy text using system clipboard tools (wl-copy, xclip, xsel).
fn copy_via_system_tool(text: &str) -> anyhow::Result<()> {
    let tools: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];

    for &(cmd, args) in tools {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            // .take() moves ChildStdin out so it drops (closes) after write, sending EOF
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            let status = child.wait()?;
            if status.success() {
                return Ok(());
            }
        }
    }

    anyhow::bail!("No clipboard tool available (tried wl-copy, xclip, xsel)")
}

/// Paste text using system clipboard tools (wl-paste, xclip, xsel).
fn paste_via_system_tool() -> anyhow::Result<String> {
    let tools: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("xsel", &["--clipboard", "--output"]),
    ];

    for &(cmd, args) in tools {
        if let Ok(output) = Command::new(cmd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            && output.status.success()
        {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }

    anyhow::bail!("No clipboard tool available (tried wl-paste, xclip, xsel)")
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    let clean = sanitize_for_clipboard(text);
    if clean.trim().is_empty() {
        anyhow::bail!("Nothing to copy (empty after sanitization)");
    }

    // Always mirror via OSC 52 first — it's the only path that reaches the
    // client-side OS clipboard when we're running remotely (see comment
    // on `copy_via_osc52`). Local tools below are a same-host convenience
    // on top of that, not a substitute for it.
    let osc52_ok = copy_via_osc52(&clean).is_ok();

    // Prefer system tools on Linux — arboard can silently fail to set
    // clipboard content with non-ASCII characters on Wayland.
    if cfg!(target_os = "linux")
        && let Ok(()) = copy_via_system_tool(&clean)
    {
        return Ok(());
    }

    // Fallback to arboard (primary on macOS/Windows)
    match Clipboard::new().and_then(|mut c| c.set_text(clean)) {
        Ok(()) => Ok(()),
        Err(e) => {
            if osc52_ok {
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}

pub fn paste_from_clipboard() -> anyhow::Result<String> {
    if cfg!(target_os = "linux")
        && let Ok(text) = paste_via_system_tool()
    {
        return Ok(text);
    }

    Ok(Clipboard::new()?.get_text()?)
}
