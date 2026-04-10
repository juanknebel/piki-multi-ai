use std::env;
use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .expect("HOME not set")
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
