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

    /// Create using the platform defaults:
    /// - Data: `dirs::data_dir()/piki-multi`
    /// - Config: `$HOME/.config/piki-multi`
    pub fn default_paths() -> Self {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("piki-multi");
        let config_base = {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".config/piki-multi")
        };
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
}
