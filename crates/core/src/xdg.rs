use std::env;
use std::path::PathBuf;

/// The user's home directory.
///
/// Falls back to `/tmp` rather than panicking: `HOME` is genuinely absent
/// under systemd units, `env -i`, and some cron/launchd contexts, and aborting
/// the whole app at startup over it is worse than running with scratch paths.
pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

pub fn data_dir() -> PathBuf {
    env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".local/share"))
        .join("piki-multi")
}

pub fn config_dir() -> PathBuf {
    env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".config"))
        .join("piki-multi")
}
