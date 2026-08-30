//! Persistent sessions: a headless daemon owns every PTY (and a terminal
//! emulator per session) so tabs survive the frontend that opened them;
//! the TUI and the desktop app attach over a Unix socket and receive a
//! restore buffer that rebuilds the screen + scrollback.
//!
//! Design: `docs/persistent-sessions.md`. Layout:
//! - [`protocol`] — wire types + framing shared by daemon and clients.
//! - [`restore`] — turning a daemon-side `vt100` parser into the byte stream
//!   a freshly attached client replays.
//! - `uds` — AF_UNIX bind/connect that keep the socket in the data dir even
//!   when the path overflows `sun_path` (internal).

pub mod client;
pub mod daemon;
pub mod protocol;
pub mod restore;
mod uds;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A process-unique session id (doubles as the daemon-side tab id): pid + a
/// process-global counter + wall-clock nanos. Collision-free across
/// concurrent spawns in one process (counter) and across separate processes
/// (pid + nanos). Same shape as the cli-agent FIFO naming.
pub fn new_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{pid}-{n}-{nanos}")
}

/// Whether persistent sessions are enabled in `config.toml` (`[sessions]
/// enabled`, default `true`).
///
/// Both frontends must answer this the same way — the TUI's full `Config`
/// parse and this minimal lookup read the same file and key. The desktop
/// (which keeps its own settings in SQLite and doesn't parse `config.toml`
/// otherwise) calls this before connecting to the daemon. Any failure —
/// missing file, unparseable TOML, absent key — means the default: enabled.
pub fn sessions_enabled(config_path: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(config_path) else {
        return true;
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return true;
    };
    table
        .get("sessions")
        .and_then(|s| s.get("enabled"))
        .and_then(|e| e.as_bool())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::sessions_enabled;

    fn write(dir: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join("config.toml");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn disabled_only_by_an_explicit_false() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!sessions_enabled(&write(
            &dir,
            "[sessions]\nenabled = false\n"
        )));
        assert!(sessions_enabled(&write(
            &dir,
            "[sessions]\nenabled = true\n"
        )));
    }

    #[test]
    fn missing_file_key_or_broken_toml_defaults_to_enabled() {
        let dir = tempfile::tempdir().unwrap();
        assert!(sessions_enabled(&dir.path().join("nope.toml")));
        assert!(sessions_enabled(&write(&dir, "theme = \"nord\"\n")));
        assert!(sessions_enabled(&write(&dir, "[sessions]\n")));
        assert!(sessions_enabled(&write(&dir, "not [ valid toml")));
        assert!(sessions_enabled(&write(
            &dir,
            "[sessions]\nenabled = \"yes\"\n"
        )));
    }
}
