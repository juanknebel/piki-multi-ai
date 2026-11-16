use std::time::Instant;

use crate::paths::DataPaths;
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
        warnings.push(
            "lazygit not found — the Git tab needs it (https://github.com/jesseduffield/lazygit)"
                .to_string(),
        );
    }

    // claude (optional — only needed for Claude agent tabs / dispatch)
    if !timed_command_ok("claude") {
        warnings
            .push("claude not found — Claude agent tabs and dispatch are unavailable".to_string());
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

/// One `doctor` line: what was checked, how bad it is, and the detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: DoctorStatus,
    pub detail: String,
}

/// Severity of a [`DoctorCheck`]. Anything but `Ok` is worth printing;
/// only `Error` fails the command (non-zero exit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorStatus {
    Ok,
    Warn,
    Error,
}

/// The full `doctor` report: one [`DoctorCheck`] per area.
#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn errors(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == DoctorStatus::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == DoctorStatus::Warn)
            .count()
    }

    pub fn has_errors(&self) -> bool {
        self.errors() > 0
    }
}

/// On-demand health check for scripting and bug reports: optional
/// dependencies (`gh`, `rg`), the session daemon, the database, the
/// providers file and directory writability — on top of everything
/// [`run_preflight_checks`] covers at startup. Read-only: it never creates
/// sessions, writes config, or migrates anything.
pub fn run_doctor_checks(paths: &DataPaths) -> DoctorReport {
    let mut checks = Vec::new();

    // Startup preflight, mapped onto named checks.
    let preflight = run_preflight_checks();
    for error in &preflight.errors {
        checks.push(DoctorCheck {
            name: "git",
            status: DoctorStatus::Error,
            detail: error.clone(),
        });
    }
    if preflight.errors.is_empty() {
        let version = shell_env::sync_command("git")
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        checks.push(DoctorCheck {
            name: "git",
            status: DoctorStatus::Ok,
            detail: version,
        });
    }
    for warning in &preflight.warnings {
        let name = if warning.starts_with("lazygit") {
            "lazygit"
        } else if warning.starts_with("claude") {
            "claude"
        } else if warning.starts_with("jq") {
            "jq"
        } else {
            "preflight"
        };
        checks.push(DoctorCheck {
            name,
            status: DoctorStatus::Warn,
            detail: warning.clone(),
        });
    }
    for present in ["lazygit", "claude", "jq"] {
        if !preflight.warnings.iter().any(|w| w.starts_with(present)) {
            checks.push(DoctorCheck {
                name: present,
                status: DoctorStatus::Ok,
                detail: version_line(present),
            });
        }
    }

    // Optional extras preflight doesn't cover at startup.
    for (name, why) in [
        ("gh", "code review needs it"),
        ("rg", "project search falls back to grep -rn without it"),
    ] {
        match probe_version(name) {
            Some(v) => checks.push(DoctorCheck {
                name,
                status: DoctorStatus::Ok,
                detail: v,
            }),
            None => checks.push(DoctorCheck {
                name,
                status: DoctorStatus::Warn,
                detail: format!("not found — {why}"),
            }),
        }
    }

    // Session daemon: reachable with N sessions, or politely absent (it
    // starts automatically — absence is information, not a failure).
    {
        use piki_multiplex::session::client::{ClientError, Daemon};
        let daemon = Daemon::new(paths.session_socket());
        match daemon.list() {
            Ok(sessions) => checks.push(DoctorCheck {
                name: "daemon",
                status: DoctorStatus::Ok,
                detail: format!("running, holding {} session(s)", sessions.len()),
            }),
            Err(ClientError::NotRunning) => checks.push(DoctorCheck {
                name: "daemon",
                status: DoctorStatus::Ok,
                detail: "not running — starts automatically with the app".to_string(),
            }),
            Err(e) => checks.push(DoctorCheck {
                name: "daemon",
                status: DoctorStatus::Warn,
                detail: format!("unreachable: {e}"),
            }),
        }
    }

    // Database: missing means "first launch creates it"; present means it
    // must open read-only and pass integrity_check.
    {
        let db = paths.db_path();
        if !db.exists() {
            checks.push(DoctorCheck {
                name: "database",
                status: DoctorStatus::Ok,
                detail: "not created yet — first launch creates it".to_string(),
            });
        } else {
            let result = (|| -> Result<(), String> {
                let conn = rusqlite::Connection::open_with_flags(
                    &db,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .map_err(|e| format!("cannot open read-only: {e}"))?;
                let integrity: String = conn
                    .query_row("PRAGMA integrity_check", [], |r| r.get(0))
                    .map_err(|e| format!("integrity_check failed: {e}"))?;
                if integrity.eq_ignore_ascii_case("ok") {
                    Ok(())
                } else {
                    Err(format!("integrity_check: {integrity}"))
                }
            })();
            match result {
                Ok(()) => checks.push(DoctorCheck {
                    name: "database",
                    status: DoctorStatus::Ok,
                    detail: db.display().to_string(),
                }),
                Err(detail) => checks.push(DoctorCheck {
                    name: "database",
                    status: DoctorStatus::Error,
                    detail,
                }),
            }
        }
        // The parent must exist (or be creatable) either way.
        if let Some(parent) = db.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            checks.push(DoctorCheck {
                name: "data-dir",
                status: DoctorStatus::Error,
                detail: format!("cannot create {}", parent.display()),
            });
        }
    }

    // Providers file: missing seeds defaults on first launch (fine);
    // present but unparsable breaks every provider lookup (error).
    {
        let path = paths.providers_path();
        match std::fs::read_to_string(&path) {
            Err(_) => checks.push(DoctorCheck {
                name: "providers",
                status: DoctorStatus::Ok,
                detail: "no providers.toml — defaults will be seeded".to_string(),
            }),
            Ok(contents) => match crate::providers::validate_file_contents(&contents) {
                Ok(n) => checks.push(DoctorCheck {
                    name: "providers",
                    status: DoctorStatus::Ok,
                    detail: format!("{n} provider(s) in {}", path.display()),
                }),
                Err(reason) => checks.push(DoctorCheck {
                    name: "providers",
                    status: DoctorStatus::Error,
                    detail: format!("{}: {reason}", path.display()),
                }),
            },
        }
    }

    // Worktrees base must be creatable/writable — every worktree workspace
    // lands under it.
    {
        let base = paths.base().join("worktrees");
        match std::fs::create_dir_all(&base) {
            Ok(()) => checks.push(DoctorCheck {
                name: "worktrees",
                status: DoctorStatus::Ok,
                detail: format!("writable {}", base.display()),
            }),
            Err(e) => checks.push(DoctorCheck {
                name: "worktrees",
                status: DoctorStatus::Error,
                detail: format!("cannot write {}: {e}", base.display()),
            }),
        }
    }

    DoctorReport { checks }
}

/// First line of `<cmd> --version`, or `None` when it doesn't run.
fn probe_version(cmd: &str) -> Option<String> {
    shell_env::sync_command(cmd)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or(cmd)
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
}

/// Same as [`probe_version`], defaulting to the bare name for the ok-detail.
fn version_line(cmd: &str) -> String {
    probe_version(cmd).unwrap_or_else(|| cmd.to_string())
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

    fn check<'a>(report: &'a DoctorReport, name: &str) -> &'a DoctorCheck {
        report
            .checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing check {name}"))
    }

    #[test]
    fn doctor_covers_every_area_on_a_fresh_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        let report = run_doctor_checks(&paths);

        // Env-independent areas on a fresh dir: nothing created yet, no
        // daemon on a fresh socket, no providers file.
        assert_eq!(check(&report, "database").status, DoctorStatus::Ok);
        assert_eq!(check(&report, "daemon").status, DoctorStatus::Ok);
        assert!(check(&report, "daemon").detail.contains("not running"));
        assert_eq!(check(&report, "providers").status, DoctorStatus::Ok);
        assert_eq!(check(&report, "worktrees").status, DoctorStatus::Ok);
        // The git check is always present (ok or error, depending on PATH).
        assert!(report.checks.iter().any(|c| c.name == "git"));
        assert_eq!(
            report.errors(),
            report
                .checks
                .iter()
                .filter(|c| c.status == DoctorStatus::Error)
                .count()
        );
    }

    #[test]
    fn doctor_flags_an_unparsable_providers_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        let providers = paths.providers_path();
        std::fs::create_dir_all(providers.parent().unwrap()).unwrap();
        std::fs::write(&providers, "not = [valid").unwrap();

        let report = run_doctor_checks(&paths);
        let check = check(&report, "providers");
        assert_eq!(check.status, DoctorStatus::Error);
        assert!(check.detail.contains("invalid TOML"));
        assert!(report.has_errors());
    }

    #[test]
    fn doctor_counts_a_valid_providers_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        let providers = paths.providers_path();
        std::fs::create_dir_all(providers.parent().unwrap()).unwrap();
        std::fs::write(&providers, "[[providers]]\nname = \"A\"\ncommand = \"a\"\n").unwrap();

        let report = run_doctor_checks(&paths);
        let check = check(&report, "providers");
        assert_eq!(check.status, DoctorStatus::Ok);
        assert!(check.detail.contains("1 provider(s)"));
    }
}
