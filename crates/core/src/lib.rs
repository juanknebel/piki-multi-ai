pub mod agent_scan;
pub mod agent_state_detect;
pub mod app_settings;
pub mod chat;
pub mod chat_providers;
pub mod cli_agent;
pub mod diff;
pub mod domain;
pub mod external_agents;
pub mod git;
pub mod github;
pub mod idle_watcher;
pub mod notifications;
pub mod paths;
pub mod preflight;
pub mod providers;
pub mod pty;
pub mod search;
pub mod shell_env;
pub mod sound;
pub mod storage;
pub mod sysinfo;
pub mod workspace;
pub mod xdg;

pub use domain::*;

/// The terminal-multiplexer engine: PTY spawning, persistent-session daemon,
/// OSC 133/7 shell integration. See `piki-multiplex`.
pub use piki_multiplex::session;
pub use piki_multiplex::shell_integration;
