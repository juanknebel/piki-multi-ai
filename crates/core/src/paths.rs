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
}
