use std::process::Command;

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
    match Command::new("git").arg("--version").output() {
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

    // Optional tools
    check_optional(
        &mut warnings,
        "claude",
        "AI assistant (claude) not found — Claude tabs will fail to spawn",
    );
    check_optional(
        &mut warnings,
        "gemini",
        "AI assistant (gemini) not found — Gemini tabs will fail to spawn",
    );
    check_optional(
        &mut warnings,
        "codex",
        "AI assistant (codex) not found — Codex tabs will fail to spawn",
    );

    // delta (optional, affects diff display)
    if Command::new("delta").arg("--version").output().is_err()
        || Command::new("delta")
            .arg("--version")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
    {
        warnings.push("delta not found — diffs will use plain git diff".to_string());
    }

    PreflightResult { errors, warnings }
}

fn check_optional(warnings: &mut Vec<String>, cmd: &str, message: &str) {
    match Command::new(cmd).arg("--version").output() {
        Ok(output) if output.status.success() => {}
        _ => {
            warnings.push(message.to_string());
        }
    }
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
