use arboard::Clipboard;
use std::io::Write;
use std::process::{Command, Stdio};

/// Remove control characters (except newline, tab, carriage return)
/// that can cause clipboard operations to fail silently on some backends.
fn sanitize_for_clipboard(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
        .collect()
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
        {
            if output.status.success() {
                return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
    }

    anyhow::bail!("No clipboard tool available (tried wl-paste, xclip, xsel)")
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    let clean = sanitize_for_clipboard(text);
    if clean.trim().is_empty() {
        anyhow::bail!("Nothing to copy (empty after sanitization)");
    }

    // Prefer system tools on Linux — arboard can silently fail to set
    // clipboard content with non-ASCII characters on Wayland.
    if cfg!(target_os = "linux") {
        if let Ok(()) = copy_via_system_tool(&clean) {
            return Ok(());
        }
    }

    // Fallback to arboard (primary on macOS/Windows)
    Clipboard::new()?.set_text(clean)?;
    Ok(())
}

pub fn paste_from_clipboard() -> anyhow::Result<String> {
    if cfg!(target_os = "linux") {
        if let Ok(text) = paste_via_system_tool() {
            return Ok(text);
        }
    }

    Ok(Clipboard::new()?.get_text()?)
}
