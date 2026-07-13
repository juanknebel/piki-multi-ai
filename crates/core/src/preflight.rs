use std::time::Instant;

use crate::shell_env;

pub struct PreflightResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl PreflightResult {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Run pre-flight checks for required and optional dependencies.
/// Must be called before the TUI starts (uses sync I/O).
pub fn run_preflight_checks() -> PreflightResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // git (required, >= 2.20)
    let git_t0 = Instant::now();
    let git_result = shell_env::sync_command("git").arg("--version").output();
    tracing::info!(
        elapsed_ms = git_t0.elapsed().as_millis(),
        "preflight: git check done"
    );
    match git_result {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some((major, minor)) = parse_git_version(&stdout) {
                if major < 2 || (major == 2 && minor < 20) {
                    errors.push(format!(
                        "git version {}.{} is too old (requires >= 2.20)",
                        major, minor
                    ));
                }
            } else {
                warnings.push(format!(
                    "could not parse git version from: {}",
                    stdout.trim()
                ));
            }
        }
        _ => {
            errors.push("git is not installed or not in PATH (required)".to_string());
        }
    }

    // lazygit (optional, powers the TUI Git tab)
    if !timed_command_ok("lazygit") {
        warnings.push("lazygit not found — the Git tab needs it (https://github.com/jesseduffield/lazygit)".to_string());
    }

    // claude (optional — only needed for Claude agent tabs / dispatch)
    if !timed_command_ok("claude") {
        warnings.push(
            "claude not found — Claude agent tabs and dispatch are unavailable".to_string(),
        );
    }

    // jq (optional — required for the structured Claude integration; without
    // it Claude tabs fall back to the byte-silence idle heuristic)
    let jq_t0 = Instant::now();
    let jq_available = crate::cli_agent::install::jq_available();
    tracing::info!(
        elapsed_ms = jq_t0.elapsed().as_millis(),
        "preflight: jq check done"
    );
    if !jq_available {
        warnings.push(
            "jq not found — structured Claude integration disabled (idle heuristic fallback)"
                .to_string(),
        );
    }

    PreflightResult { errors, warnings }
}

/// `true` if `<cmd> --version` runs and exits 0. Spawns the process exactly
/// once (the previous version invoked it up to three times per check).
fn command_ok(cmd: &str) -> bool {
    shell_env::sync_command(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Same as `command_ok`, but logs elapsed time so a specific slow optional
/// dependency check is identifiable from cold-boot logs.
fn timed_command_ok(cmd: &str) -> bool {
    let t0 = Instant::now();
    let ok = command_ok(cmd);
    tracing::info!(
        cmd,
        elapsed_ms = t0.elapsed().as_millis(),
        "preflight: optional dependency check done"
    );
    ok
}

/// Parse "git version X.Y.Z" into (major, minor).
pub fn parse_git_version(version_str: &str) -> Option<(u32, u32)> {
    // Expected format: "git version 2.43.0" or "git version 2.43.0.windows.1"
    let version_part = version_str.trim().strip_prefix("git version ")?;
    let mut parts = version_part.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_version_standard() {
        assert_eq!(parse_git_version("git version 2.43.0"), Some((2, 43)));
    }

    #[test]
    fn test_parse_git_version_with_extra() {
        assert_eq!(
            parse_git_version("git version 2.39.2.windows.1"),
            Some((2, 39))
        );
    }

    #[test]
    fn test_parse_git_version_old() {
        assert_eq!(parse_git_version("git version 1.8.5"), Some((1, 8)));
    }

    #[test]
    fn test_parse_git_version_invalid() {
        assert_eq!(parse_git_version("not git"), None);
    }

    #[test]
    fn test_parse_git_version_empty() {
        assert_eq!(parse_git_version(""), None);
    }

    #[test]
    fn test_parse_git_version_with_newline() {
        assert_eq!(parse_git_version("git version 2.43.0\n"), Some((2, 43)));
    }
}
