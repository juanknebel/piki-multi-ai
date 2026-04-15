use std::io::Write;
use std::process::{Command, Stdio};

fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
        .collect()
}

fn copy_via_system_tool(text: &str) -> Result<(), String> {
    let tools: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
    ];

    for &(cmd, args) in tools {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if let Ok(status) = child.wait()
                && status.success()
            {
                return Ok(());
            }
        }
    }

    Err("No clipboard tool available".into())
}

fn paste_via_system_tool() -> Result<String, String> {
    let tools: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("xsel", &["--clipboard", "--output"]),
        ("pbpaste", &[]),
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

    Err("No clipboard tool available".into())
}

#[tauri::command]
pub async fn clipboard_copy(text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        tracing::info!("clipboard_copy called, text len={} bytes", text.len());
        let clean = sanitize(&text);
        if clean.trim().is_empty() {
            tracing::warn!("clipboard_copy: empty after sanitization");
            return Err("Nothing to copy".into());
        }
        let result = copy_via_system_tool(&clean);
        match &result {
            Ok(()) => tracing::info!("clipboard_copy: success"),
            Err(e) => tracing::error!("clipboard_copy: failed: {e}"),
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn clipboard_paste() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(paste_via_system_tool)
        .await
        .map_err(|e| e.to_string())?
}
