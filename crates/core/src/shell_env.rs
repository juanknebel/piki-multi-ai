use std::collections::HashMap;
use std::sync::OnceLock;

/// Load environment variables from the user's login shell.
///
/// When launched from a .app bundle (macOS) or .desktop entry (Linux), the app
/// inherits only a minimal environment — missing PATH extensions, LANG, and
/// other variables configured in shell profiles.  Running the user's login
/// shell captures the full environment.
///
/// The result is computed once and cached for the process lifetime.
pub fn user_login_env() -> &'static HashMap<String, String> {
    static ENV: OnceLock<HashMap<String, String>> = OnceLock::new();
    ENV.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let result = std::process::Command::new(&shell)
            .args(["-l", "-c", "env -0"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();

        match result {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .split('\0')
                    .filter_map(|entry| {
                        let (k, v) = entry.split_once('=')?;
                        Some((k.to_string(), v.to_string()))
                    })
                    .collect()
            }
            _ => {
                tracing::warn!("Failed to load login shell environment");
                HashMap::new()
            }
        }
    })
}

/// Create a `tokio::process::Command` with the user's login shell environment
/// applied.  This ensures that commands like `gh`, `git`, and `delta` are
/// found even when the app is launched outside a terminal (e.g. from Finder
/// on macOS or a .desktop entry on Linux).
pub fn command(program: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    let env = user_login_env();
    if !env.is_empty() {
        cmd.envs(env);
    }
    cmd
}

/// Resolve a command name to its absolute path using the user's login shell
/// environment.  Falls back to the original name if not found.
///
/// This is useful when `portable-pty`'s built-in PATH search fails because the
/// PTY spawner uses the parent process's inherited (minimal) PATH instead of
/// the overridden env vars.
pub fn resolve_command(name: &str) -> String {
    use std::os::unix::fs::PermissionsExt;

    let env = user_login_env();
    if let Some(path_var) = env.get("PATH") {
        for dir in std::env::split_paths(&std::ffi::OsString::from(path_var)) {
            let candidate = dir.join(name);
            if let Ok(meta) = std::fs::metadata(&candidate)
                && meta.is_file()
                && meta.permissions().mode() & 0o111 != 0
                && let Some(s) = candidate.to_str()
            {
                tracing::debug!(command = name, resolved = s, "Resolved command via login env PATH");
                return s.to_string();
            }
        }
    }
    tracing::warn!(command = name, "Could not resolve command in login env PATH, using bare name");
    name.to_string()
}

/// Same as [`command`] but returns a synchronous `std::process::Command`.
pub fn sync_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    let env = user_login_env();
    if !env.is_empty() {
        cmd.envs(env);
    }
    cmd
}
