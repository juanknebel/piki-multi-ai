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

/// Same as [`command`] but returns a synchronous `std::process::Command`.
pub fn sync_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    let env = user_login_env();
    if !env.is_empty() {
        cmd.envs(env);
    }
    cmd
}
