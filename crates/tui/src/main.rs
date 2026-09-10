mod action;
mod action_catalog;
mod app;
mod clipboard;
pub(crate) mod code_review;
mod command_palette;
mod config;
mod dialog_state;
mod event_loop;
mod helpers;
mod input;
mod log_buffer;
mod pty;
mod syntax;
mod term_guard;
#[cfg(test)]
mod test_support;
mod text;
mod theme;
mod ui;
mod watchdog;
mod workspace_switcher;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "piki-multi-ai")]
#[command(version, about = "Terminal UI for orchestrating multiple AI assistants in parallel", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Logging level: trace, debug, info, warn, error
    #[arg(long, default_value = "info", global = true)]
    log_level: String,

    /// Override the data directory (database, worktrees, logs).
    /// Defaults to ~/.local/share/piki-multi.
    /// Useful for running a nightly/test instance alongside stable.
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generates the default configuration file to stdout
    GenerateConfig,
    /// Shows version and author information (same as About in-app)
    Version,
    /// Migrate workspace config from JSON files to SQLite database
    Migrate,
    /// Run the persistent-session daemon (normally started automatically).
    Serve {
        /// Stay in the foreground (don't daemonize); logs go to stderr.
        #[arg(long)]
        foreground: bool,
    },
    /// Inspect or control persistent sessions running in the daemon.
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
}

#[derive(Subcommand)]
enum SessionsAction {
    /// List all sessions the daemon is holding.
    List,
    /// Kill a session by id (its child dies; the session is retained).
    Kill { id: String },
    /// Stop the session daemon (kills all sessions).
    Stop,
}

/// Entry point. Kept **synchronous** so the `serve` subcommand can daemonize
/// (which forks) before any tokio runtime — forking a multi-threaded process
/// is unsafe. The interactive app path builds its own runtime and hands off to
/// [`run_tui`].
fn main() -> anyhow::Result<()> {
    piki_core::notifications::set_appname("piki-multi-ai");
    let cli = Cli::parse();

    // Clone the data dir so `cli` stays whole to move into `run_tui` below.
    let paths = match &cli.data_dir {
        Some(dir) => piki_core::paths::DataPaths::new(dir.clone()),
        None => piki_core::paths::DataPaths::default_paths(),
    };

    if let Some(command) = &cli.command {
        match command {
            Commands::GenerateConfig => {
                println!("{}", config::Config::generate_default_toml());
                return Ok(());
            }
            Commands::Version => {
                print_version();
                return Ok(());
            }
            Commands::Migrate => {
                return run_migrate(&paths);
            }
            Commands::Serve { foreground } => {
                // MUST run before any tokio runtime exists (it forks).
                #[cfg(unix)]
                {
                    return piki_core::session::daemon::run(
                        &paths.daemon_paths(),
                        *foreground,
                        Some(piki_core::cli_agent::sidecar_config()),
                    );
                }
                #[cfg(not(unix))]
                {
                    let _ = foreground;
                    anyhow::bail!("persistent sessions require a Unix platform");
                }
            }
            Commands::Sessions { action } => {
                return run_sessions_cli(&paths, action);
            }
        }
    }

    // Interactive app: build the runtime here (after the fork-sensitive
    // subcommands are handled) and run the event loop.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_tui(cli, paths))
}

fn print_version() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("██████╗ ██╗██╗  ██╗██╗");
    println!("██╔══██╗██║██║ ██╔╝██║");
    println!("██████╔╝██║█████╔╝ ██║");
    println!("██╔═══╝ ██║██╔═██╗ ██║");
    println!("██║     ██║██║  ██╗██║");
    println!("╚═╝     ╚═╝╚═╝  ╚═╝╚═╝");
    println!();
    println!("piki-multi-ai v{version}");
    println!();
    println!("Author: Juan Knebel");
    println!("Contact: juanknebel@gmail.com");
    println!("Web: github.com/juanknebel/piki-multi-ai");
    println!("License: GPL-2.0");
    println!();
}

fn run_migrate(paths: &piki_core::paths::DataPaths) -> anyhow::Result<()> {
    let db_path = paths.db_path();
    let parent = db_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("db path has no parent directory: {}", db_path.display()))?;
    std::fs::create_dir_all(parent)?;
    let storage = piki_core::storage::sqlite::SqliteStorage::open(&db_path)?;
    let count = storage.migrate_from_json(paths)?;
    println!("Migrated {count} workspaces from JSON to SQLite");
    println!("Database: {}", db_path.display());
    Ok(())
}

/// The `sessions` subcommand: a thin CLI over the daemon control protocol.
fn run_sessions_cli(
    paths: &piki_core::paths::DataPaths,
    action: &SessionsAction,
) -> anyhow::Result<()> {
    use piki_core::session::client::{ClientError, Daemon};
    let daemon = Daemon::new(paths.session_socket());
    let not_running = |e: ClientError| -> anyhow::Error {
        match e {
            ClientError::NotRunning => anyhow::anyhow!("session daemon is not running"),
            other => anyhow::anyhow!("{other}"),
        }
    };
    match action {
        SessionsAction::List => match daemon.list() {
            Ok(sessions) if sessions.is_empty() => println!("no sessions"),
            Ok(sessions) => {
                println!(
                    "{:<28}  {:<8}  {:<12}  WORKSPACE",
                    "ID", "STATE", "PROVIDER"
                );
                for s in sessions {
                    let state = if s.state.is_live() {
                        format!("live×{}", s.attached)
                    } else {
                        "exited".to_string()
                    };
                    println!(
                        "{:<28}  {:<8}  {:<12}  {}",
                        s.id,
                        state,
                        s.meta.provider,
                        s.meta.workspace_path.display()
                    );
                }
            }
            Err(ClientError::NotRunning) => println!("session daemon is not running"),
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        },
        SessionsAction::Kill { id } => {
            daemon.kill(id).map_err(not_running)?;
            println!("killed {id}");
        }
        SessionsAction::Stop => match daemon.shutdown(true) {
            Ok(()) => println!("session daemon stopped"),
            Err(ClientError::NotRunning) => println!("session daemon is not running"),
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        },
    }
    Ok(())
}

async fn run_tui(cli: Cli, paths: piki_core::paths::DataPaths) -> anyhow::Result<()> {
    // Initialize structured logging to file
    let log_dir = paths.log_dir();
    let file_appender = tracing_appender::rolling::daily(&log_dir, "piki-multi.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let level_filter = match cli.log_level.to_lowercase().as_str() {
        "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
        "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
        "warn" => tracing_subscriber::filter::LevelFilter::WARN,
        "error" => tracing_subscriber::filter::LevelFilter::ERROR,
        _ => tracing_subscriber::filter::LevelFilter::INFO,
    };

    use tracing_subscriber::prelude::*;

    let log_buffer = log_buffer::new_buffer();

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true);

    let memory_layer = log_buffer::MemoryLayer::new(std::sync::Arc::clone(&log_buffer));

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(memory_layer)
        .with(level_filter)
        .init();

    tracing::info!(log_level = %cli.log_level, "piki-multi-ai starting");

    // Pre-flight dependency checks
    let startup_t0 = std::time::Instant::now();
    let preflight_t0 = std::time::Instant::now();
    let preflight = piki_core::preflight::run_preflight_checks();
    tracing::info!(
        elapsed_ms = preflight_t0.elapsed().as_millis(),
        "startup: preflight checks done"
    );
    if preflight.has_errors() {
        for error in &preflight.errors {
            tracing::error!("{}", error);
            eprintln!("FATAL: {}", error);
        }
        std::process::exit(1);
    }
    for warning in &preflight.warnings {
        tracing::warn!("{}", warning);
    }

    // Check if terminal supports the Kitty keyboard protocol (for Shift+Enter detection)
    let kitty_probe_t0 = std::time::Instant::now();
    let kitty_keyboard = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    tracing::info!(
        elapsed_ms = kitty_probe_t0.elapsed().as_millis(),
        supported = kitty_keyboard,
        "startup: keyboard-enhancement probe done"
    );
    if kitty_keyboard {
        tracing::info!("terminal supports Kitty keyboard protocol");
    } else {
        tracing::info!(
            "terminal does not support Kitty keyboard protocol; use Ctrl+Enter for newline"
        );
    }

    // Install panic hook that restores terminal before printing panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if kitty_keyboard {
            let _ = crossterm::execute!(
                std::io::stderr(),
                crossterm::event::PopKeyboardEnhancementFlags
            );
        }
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableFocusChange,
        );
        ratatui::restore();
        original_hook(panic_info);
    }));

    // Must run before `ratatui::init` flips raw mode: it snapshots the cooked
    // termios that a SIGTERM/SIGHUP handler will put back.
    term_guard::install();
    let terminal = ratatui::init();
    crossterm::execute!(
        std::io::stderr(),
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableFocusChange,
    )?;
    if kitty_keyboard {
        crossterm::execute!(
            std::io::stderr(),
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        )?;
    }
    tracing::info!(
        elapsed_ms = startup_t0.elapsed().as_millis(),
        "startup: pre-event-loop setup done, entering event_loop::run"
    );
    watchdog::start();
    let result = event_loop::run(terminal, preflight.warnings, log_buffer, paths).await;
    // Best-effort cleanup: an early `?` here used to skip DisableMouseCapture,
    // leaving the shell spammed with mouse-report escape sequences after an
    // error exit.
    if kitty_keyboard {
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::event::PopKeyboardEnhancementFlags
        );
    }
    let _ = crossterm::execute!(
        std::io::stderr(),
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableFocusChange,
    );
    ratatui::restore();
    tracing::info!("piki-multi-ai shutdown");
    result
}
