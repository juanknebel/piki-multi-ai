use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// Print the current process environment as JSON to stdout and exit.
///
/// Called by binaries when they receive the `--printenv` flag, enabling the
/// self-invocation strategy: the login shell re-launches the current binary
/// with `--printenv`, which inherits the fully-configured environment and
/// dumps it as clean JSON — free of shell startup noise.
pub fn print_env_and_exit() -> ! {
    let env: HashMap<String, String> = std::env::vars().collect();
    let json = serde_json::to_string(&env).unwrap_or_else(|_| "{}".to_string());
    print!("{json}");
    std::process::exit(0)
}

/// Load environment variables from the user's login shell.
///
/// When launched from a .app bundle (macOS) or .desktop entry (Linux), the app
/// inherits only a minimal environment.  This function captures the full
/// environment using the same strategy as the Zed editor:
///
/// 1. Detect the user's shell (via `$SHELL`, `getpwuid_r`, or platform default)
/// 2. Spawn an interactive login shell (`-l -i -c`) that re-invokes the current
///    binary with `--printenv`
/// 3. The child binary dumps `std::env::vars()` as JSON and exits
/// 4. The parent parses the JSON — cleanly separated from any shell startup noise
///
/// Falls back to parsing `env` output if self-invocation is not available.
///
/// The result is computed once and cached for the process lifetime.
pub fn user_login_env() -> &'static HashMap<String, String> {
    static ENV: OnceLock<HashMap<String, String>> = OnceLock::new();
    ENV.get_or_init(|| {
        let shell = detect_user_shell();

        // Strategy 1: self-invocation (preferred — clean JSON, no parsing ambiguity)
        let mut env_map = match capture_env_via_self_invocation(&shell) {
            Ok(map) if !map.is_empty() => {
                tracing::debug!(shell, vars = map.len(), "Loaded shell env via self-invocation");
                map
            }
            Ok(_) => {
                tracing::warn!(shell, "Self-invocation returned empty env, trying fallback");
                capture_env_via_shell_command(&shell)
            }
            Err(e) => {
                tracing::warn!(shell, error = %e, "Self-invocation failed, trying fallback");
                capture_env_via_shell_command(&shell)
            }
        };

        // Safety net: ensure common tool directories are in PATH
        augment_path_with_common_dirs(&mut env_map);

        if env_map.is_empty() {
            tracing::warn!(shell, "Could not load login shell environment");
        }

        env_map
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
    #[cfg(unix)]
    {
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

// ---------------------------------------------------------------------------
// Shell detection
// ---------------------------------------------------------------------------

/// Detect the user's login shell.
///
/// Checks `$SHELL`, then the system passwd database (via `getpwuid_r`), and
/// finally falls back to `/bin/zsh` on macOS or `/bin/sh` on Linux.
fn detect_user_shell() -> String {
    // 1. $SHELL env var (set by login/launchd)
    if let Ok(shell) = std::env::var("SHELL")
        && is_valid_shell(&shell)
    {
        return shell;
    }

    // 2. System passwd database
    #[cfg(unix)]
    if let Some(shell) = shell_from_passwd()
        && is_valid_shell(&shell)
    {
        return shell;
    }

    // 3. Platform default
    default_shell().to_string()
}

fn is_valid_shell(path: &str) -> bool {
    !path.is_empty() && path != "/bin/false" && std::path::Path::new(path).exists()
}

fn default_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    }
}

/// Read the user's shell from the system passwd database via `getpwuid_r`.
#[cfg(unix)]
fn shell_from_passwd() -> Option<String> {
    use std::ffi::CStr;
    use std::mem::MaybeUninit;

    let uid = unsafe { libc::getuid() };
    let mut buf = vec![0u8; 4096];
    let mut pwd = MaybeUninit::<libc::passwd>::uninit();
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            pwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };

    if ret != 0 || result.is_null() {
        return None;
    }

    let pwd = unsafe { pwd.assume_init() };
    let shell = unsafe { CStr::from_ptr(pwd.pw_shell) };
    shell.to_str().ok().map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Strategy 1: Self-invocation (Zed-style)
// ---------------------------------------------------------------------------

/// Capture the environment by re-invoking the current binary with `--printenv`
/// inside the user's login shell.  The child inherits the shell's fully
/// configured environment and dumps it as JSON.
fn capture_env_via_self_invocation(shell: &str) -> anyhow::Result<HashMap<String, String>> {
    let exe = std::env::current_exe()?;
    let exe_path = exe
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 executable path"))?;

    let script = format!("{} --printenv", shell_quote(exe_path));

    let mut cmd = new_shell_command(shell, &script);
    cmd.stdout(std::process::Stdio::piped());

    let output = cmd.output()?;

    if !output.status.success() {
        anyhow::bail!("shell exited with {}", output.status);
    }

    parse_env_json_from_output(&output.stdout)
}

/// Parse a `HashMap<String, String>` from potentially noisy shell output by
/// scanning for the first valid JSON object.
fn parse_env_json_from_output(data: &[u8]) -> anyhow::Result<HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);
    for (pos, _) in text.match_indices('{') {
        let candidate = &text[pos..];
        let mut de = serde_json::Deserializer::from_str(candidate);
        if let Ok(map) = HashMap::<String, String>::deserialize(&mut de) {
            return Ok(map);
        }
    }
    anyhow::bail!("no valid JSON object found in shell output")
}

// ---------------------------------------------------------------------------
// Strategy 2: Fallback — parse `env` output with markers
// ---------------------------------------------------------------------------

/// Fallback when self-invocation is unavailable (e.g. TUI binary doesn't
/// handle `--printenv`).  Uses markers to isolate `env` output from shell
/// startup noise.  Does NOT use `env -0` which is unavailable on macOS/BSD.
fn capture_env_via_shell_command(shell: &str) -> HashMap<String, String> {
    let marker = "___PIKI_ENV_8f3a___";
    let script = format!(
        "printf '\\n{marker}\\n'; env; printf '\\n{marker}\\n'"
    );

    let mut cmd = new_shell_command(shell, &script);
    cmd.stdout(std::process::Stdio::piped());

    let output = match cmd.output() {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            tracing::debug!(status = %out.status, "env fallback command failed");
            return HashMap::new();
        }
        Err(e) => {
            tracing::debug!(error = %e, "env fallback command failed to spawn");
            return HashMap::new();
        }
    };

    extract_env_between_markers(&output.stdout, marker)
}

/// Build a shell command with `-l -i -c` (interactive login) and safety
/// measures: `setsid()` to prevent the shell from stealing the TTY, and
/// null stdin/stderr.
fn new_shell_command(shell: &str, script: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(shell);
    cmd.args(["-l", "-i", "-c", script]);
    cmd.stdin(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    // Start a new session so the interactive shell doesn't try to become the
    // foreground process group of our terminal.
    // See: https://registerspill.thorstenball.com/p/how-to-lose-control-of-your-shell
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    cmd
}

/// Single-quote a string for POSIX shells.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// Marker-based env parsing (fallback)
// ---------------------------------------------------------------------------

/// Extract environment variables from output wrapped in markers.
fn extract_env_between_markers(stdout: &[u8], marker: &str) -> HashMap<String, String> {
    let needle = format!("\n{marker}\n");
    let needle_bytes = needle.as_bytes();

    let Some(start_pos) = stdout
        .windows(needle_bytes.len())
        .position(|w| w == needle_bytes)
    else {
        return HashMap::new();
    };
    let content_start = start_pos + needle_bytes.len();

    let remaining = &stdout[content_start..];
    let content_end = remaining
        .windows(needle_bytes.len())
        .position(|w| w == needle_bytes)
        .unwrap_or(remaining.len());

    parse_newline_separated(&remaining[..content_end])
}

/// Parse newline-separated `KEY=VALUE` entries from `env` output.
///
/// Handles multi-line values: a line without a valid `KEY=` prefix is treated
/// as a continuation of the previous entry's value.
fn parse_newline_separated(data: &[u8]) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(data);
    let mut map = HashMap::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for line in text.lines() {
        if let Some(eq_pos) = line.find('=') {
            let key = &line[..eq_pos];
            if !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                if let Some(k) = current_key.take() {
                    map.insert(k, current_value.clone());
                }
                current_key = Some(key.to_string());
                current_value = line[eq_pos + 1..].to_string();
                continue;
            }
        }
        if current_key.is_some() {
            current_value.push('\n');
            current_value.push_str(line);
        }
    }
    if let Some(k) = current_key {
        map.insert(k, current_value);
    }

    map
}

// ---------------------------------------------------------------------------
// PATH augmentation safety net
// ---------------------------------------------------------------------------

/// Append well-known tool directories to PATH if they exist on disk but are
/// not already present.  Acts as a last-resort safety net.
fn augment_path_with_common_dirs(env: &mut HashMap<String, String>) {
    let home = env
        .get("HOME")
        .cloned()
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_default();

    if home.is_empty() {
        return;
    }

    let candidates = [
        "/usr/local/bin".to_string(),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        format!("{home}/.local/bin"),
        format!("{home}/.cargo/bin"),
        format!("{home}/.volta/bin"),
        format!("{home}/.local/share/mise/shims"),
    ];

    let current_path = env.get("PATH").cloned().unwrap_or_default();
    let current_dirs: std::collections::HashSet<&str> = current_path.split(':').collect();

    let new_dirs: Vec<&str> = candidates
        .iter()
        .map(|s| s.as_str())
        .filter(|d| !current_dirs.contains(d) && std::path::Path::new(d).is_dir())
        .collect();

    if !new_dirs.is_empty() {
        let augmented = if current_path.is_empty() {
            new_dirs.join(":")
        } else {
            format!("{}:{}", current_path, new_dirs.join(":"))
        };
        tracing::debug!(added = new_dirs.join(":"), "Augmented PATH with common directories");
        env.insert("PATH".to_string(), augmented);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_json() {
        let data = br#"{"HOME":"/Users/test","PATH":"/usr/bin","SHELL":"/bin/zsh"}"#;
        let map = parse_env_json_from_output(data).unwrap();
        assert_eq!(map.get("HOME").unwrap(), "/Users/test");
        assert_eq!(map.get("PATH").unwrap(), "/usr/bin");
        assert_eq!(map.get("SHELL").unwrap(), "/bin/zsh");
    }

    #[test]
    fn test_parse_env_json_with_noise() {
        let data = b"[oh-my-zsh] loaded theme\nsome startup noise\n{\"HOME\":\"/Users/test\",\"PATH\":\"/usr/bin\"}\nmore noise";
        let map = parse_env_json_from_output(data).unwrap();
        assert_eq!(map.get("HOME").unwrap(), "/Users/test");
        assert_eq!(map.get("PATH").unwrap(), "/usr/bin");
    }

    #[test]
    fn test_parse_env_json_no_json() {
        let data = b"just some random output with no json";
        assert!(parse_env_json_from_output(data).is_err());
    }

    #[test]
    fn test_parse_newline_separated() {
        let data = b"HOME=/Users/test\nPATH=/usr/bin:/usr/local/bin\nSHELL=/bin/zsh\n";
        let map = parse_newline_separated(data);
        assert_eq!(map.get("HOME").unwrap(), "/Users/test");
        assert_eq!(map.get("PATH").unwrap(), "/usr/bin:/usr/local/bin");
        assert_eq!(map.get("SHELL").unwrap(), "/bin/zsh");
    }

    #[test]
    fn test_parse_newline_separated_multiline_value() {
        let data = b"SIMPLE=hello\nMULTI=line1\nline2\nline3\nAFTER=world\n";
        let map = parse_newline_separated(data);
        assert_eq!(map.get("SIMPLE").unwrap(), "hello");
        assert_eq!(map.get("MULTI").unwrap(), "line1\nline2\nline3");
        assert_eq!(map.get("AFTER").unwrap(), "world");
    }

    #[test]
    fn test_extract_env_between_markers() {
        let marker = "___PIKI_ENV_8f3a___";
        let data = format!(
            "startup noise\n\n{marker}\nHOME=/Users/test\nPATH=/usr/bin\n\n{marker}\n"
        );
        let map = extract_env_between_markers(data.as_bytes(), marker);
        assert_eq!(map.get("HOME").unwrap(), "/Users/test");
        assert_eq!(map.get("PATH").unwrap(), "/usr/bin");
    }

    #[test]
    fn test_extract_env_no_markers() {
        let map = extract_env_between_markers(b"random output", "___PIKI_ENV_8f3a___");
        assert!(map.is_empty());
    }

    #[test]
    fn test_shell_quote() {
        assert_eq!(shell_quote("/usr/bin/test"), "'/usr/bin/test'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(
            shell_quote("/path/with spaces/bin"),
            "'/path/with spaces/bin'"
        );
    }

    #[test]
    fn test_detect_user_shell_returns_something() {
        let shell = detect_user_shell();
        assert!(!shell.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_shell_from_passwd_returns_valid() {
        // On any Unix system, the current user should have a valid shell
        if let Some(shell) = shell_from_passwd() {
            assert!(shell.starts_with('/'));
        }
    }
}
