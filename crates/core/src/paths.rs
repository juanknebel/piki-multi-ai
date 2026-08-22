use std::path::{Path, PathBuf};

/// Centralized directory paths for the application.
///
/// When `--data-dir` is provided, ALL paths (data and config) resolve under
/// that single directory, giving full isolation for nightly/test instances.
/// When using defaults, data goes to `~/.local/share/piki-multi` and config
/// goes to `~/.config/piki-multi` following XDG conventions.
#[derive(Debug, Clone)]
pub struct DataPaths {
    base: PathBuf,
    config_base: PathBuf,
}

impl DataPaths {
    /// Create from an explicit base directory (e.g. `--data-dir` override).
    /// Both data and config paths resolve under this single directory.
    pub fn new(base: PathBuf) -> Self {
        let config_base = base.join("config");
        Self { base, config_base }
    }

    /// Create using XDG defaults:
    /// - Data: `$XDG_DATA_HOME/piki` or `~/.local/share/piki`
    /// - Config: `$XDG_CONFIG_HOME/piki` or `~/.config/piki`
    pub fn default_paths() -> Self {
        let base = crate::xdg::data_dir();
        let config_base = crate::xdg::config_dir();
        Self { base, config_base }
    }

    /// The base data directory.
    pub fn base(&self) -> &Path {
        &self.base
    }

    /// SQLite database path: `<base>/piki.db`.
    pub fn db_path(&self) -> PathBuf {
        self.base.join("piki.db")
    }

    /// Log directory: `<base>/logs`.
    pub fn log_dir(&self) -> PathBuf {
        self.base.join("logs")
    }

    /// Worktrees base for a project: `<base>/worktrees/<project_name>`.
    pub fn worktrees_dir(&self, project_name: &str) -> PathBuf {
        self.base.join("worktrees").join(project_name)
    }

    /// Default destination for full GitHub clones (not worktrees):
    /// `<base>/repos`. The actual clone lands under `<base>/repos/<repo>`;
    /// this method returns the *parent* used as the dialog hint.
    pub fn repos_dir(&self) -> PathBuf {
        self.base.join("repos")
    }

    /// Legacy JSON workspace config directory: `<base>/workspaces`.
    pub fn legacy_workspaces_dir(&self) -> PathBuf {
        self.base.join("workspaces")
    }

    /// Config file path: `<config_base>/config.toml`.
    pub fn config_path(&self) -> PathBuf {
        self.config_base.join("config.toml")
    }

    /// Config directory (for themes, etc.): `<config_base>`.
    pub fn config_dir(&self) -> &Path {
        &self.config_base
    }

    /// Providers configuration file: `<config_base>/providers.toml`.
    pub fn providers_path(&self) -> PathBuf {
        self.config_base.join("providers.toml")
    }

    /// Shell integration directory: `<base>/shell-integration`. Holds the
    /// materialized init scripts and bridge files that piki tells the user's
    /// shell to source on startup.
    pub fn shell_integration_dir(&self) -> PathBuf {
        self.base.join("shell-integration")
    }

    /// Claude Code hooks directory: `<base>/claude-hooks`. Holds the
    /// materialized hook scripts and the generated `settings.json` that piki
    /// passes via `claude --settings` to drive the structured cli-agent
    /// (OSC 777) channel.
    pub fn claude_hooks_dir(&self) -> PathBuf {
        self.base.join("claude-hooks")
    }

    /// Antigravity hooks directory: `<base>/antigravity-hooks`. Only holds the
    /// per-tab FIFOs — the hook scripts themselves must live in agy's own
    /// customization root (it has no `--settings` equivalent), so the bridge
    /// plugin is written to
    /// [`cli_agent::install_antigravity::plugins_root`](crate::cli_agent::install_antigravity::plugins_root).
    pub fn antigravity_hooks_dir(&self) -> PathBuf {
        self.base.join("antigravity-hooks")
    }

    /// Persistent-session daemon state: `<base>/sessions` (socket, lock, pid
    /// file). Lives under the data dir so `--data-dir` instances get their
    /// own daemon.
    pub fn sessions_dir(&self) -> PathBuf {
        self.base.join("sessions")
    }

    /// Unix socket the session daemon listens on: `<base>/sessions/daemon.sock`.
    ///
    /// A `sockaddr_un` path is limited to 108 bytes on Linux and 104 on
    /// macOS. When the data dir is deep enough to blow that, the socket moves
    /// to a short per-user location (`$XDG_RUNTIME_DIR` or `/tmp`) named by a
    /// hash of the data dir, so distinct data dirs still get distinct
    /// daemons. Deterministic: every client of the same data dir computes
    /// the same path.
    pub fn session_socket(&self) -> PathBuf {
        let primary = self.sessions_dir().join("daemon.sock");
        if primary.as_os_str().len() <= SUN_PATH_BUDGET {
            return primary;
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        runtime
            .join(format!("piki-multi-{}", current_uid()))
            .join(format!(
                "{:016x}.sock",
                fnv1a64(self.base.as_os_str().as_encoded_bytes())
            ))
    }

    /// Exclusive-lock file the daemon holds while running: `<base>/sessions/daemon.lock`.
    pub fn session_lock(&self) -> PathBuf {
        self.sessions_dir().join("daemon.lock")
    }

    /// Pid of the running daemon, for `sessions stop`: `<base>/sessions/daemon.pid`.
    pub fn session_pid_file(&self) -> PathBuf {
        self.sessions_dir().join("daemon.pid")
    }

    /// Daemon log file: `<base>/logs/sessions.log`.
    pub fn session_log_path(&self) -> PathBuf {
        self.log_dir().join("sessions.log")
    }

    /// Ad-hoc PR checkouts for code review: `<base>/review-checkouts`. Each
    /// repo gets one base clone (`<owner>__<repo>`) with one `git worktree`
    /// per PR (`<owner>__<repo>--pr-<N>`), managed by
    /// [`github::ReviewCheckoutManager`](crate::github::ReviewCheckoutManager).
    /// Unlike `repos_dir()` (user-initiated clones meant to be kept), this
    /// directory is fully owned by piki and safe to prune/overwrite.
    pub fn review_checkouts_dir(&self) -> PathBuf {
        self.base.join("review-checkouts")
    }
}

/// Conservative `sun_path` budget (Linux allows 108 bytes, macOS 104, both
/// including the trailing NUL); anything above this falls back to the short
/// per-user socket location.
const SUN_PATH_BUDGET: usize = 100;

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_paths_live_under_the_data_dir() {
        let paths = DataPaths::new(PathBuf::from("/tmp/piki-test-data"));
        assert_eq!(
            paths.session_socket(),
            PathBuf::from("/tmp/piki-test-data/sessions/daemon.sock")
        );
        assert_eq!(
            paths.session_lock(),
            PathBuf::from("/tmp/piki-test-data/sessions/daemon.lock")
        );
        assert_eq!(
            paths.session_pid_file(),
            PathBuf::from("/tmp/piki-test-data/sessions/daemon.pid")
        );
        assert_eq!(
            paths.session_log_path(),
            PathBuf::from("/tmp/piki-test-data/logs/sessions.log")
        );
    }

    #[test]
    fn deep_data_dir_falls_back_to_a_short_deterministic_socket() {
        let deep = PathBuf::from(format!("/tmp/{}", "d".repeat(150)));
        let a = DataPaths::new(deep.clone()).session_socket();
        let b = DataPaths::new(deep).session_socket();
        assert_eq!(a, b, "same data dir → same socket");
        assert!(a.as_os_str().len() <= SUN_PATH_BUDGET, "{a:?}");
        assert!(
            !a.starts_with("/tmp/dddd"),
            "must not live under the deep dir"
        );
        let other =
            DataPaths::new(PathBuf::from(format!("/tmp/{}", "e".repeat(150)))).session_socket();
        assert_ne!(a, other, "different data dirs → different sockets");
    }

    #[test]
    fn fnv1a64_matches_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
