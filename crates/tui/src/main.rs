mod action;
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
mod theme;
mod ui;
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let paths = match cli.data_dir {
        Some(dir) => piki_core::paths::DataPaths::new(dir),
        None => piki_core::paths::DataPaths::default_paths(),
    };

    if let Some(command) = cli.command {
        match command {
            Commands::GenerateConfig => {
                println!("{}", config::Config::generate_default_toml());
                return Ok(());
            }
            Commands::Version => {
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
                return Ok(());
            }
            Commands::Migrate => {
                let db_path = paths.db_path();
                std::fs::create_dir_all(db_path.parent().unwrap())?;
                let storage = piki_core::storage::sqlite::SqliteStorage::open(&db_path)?;
                let count = storage.migrate_from_json(&paths)?;
                println!("Migrated {count} workspaces from JSON to SQLite");
                println!("Database: {}", db_path.display());
                return Ok(());
            }
        }
    }

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
    let preflight = piki_core::preflight::run_preflight_checks();
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
    let kitty_keyboard = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
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
        let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture);
        ratatui::restore();
        original_hook(panic_info);
    }));

    let terminal = ratatui::init();
    crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture)?;
    if kitty_keyboard {
        crossterm::execute!(
            std::io::stderr(),
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        )?;
    }
    let result = event_loop::run(terminal, preflight.warnings, log_buffer, paths).await;
    if kitty_keyboard {
        crossterm::execute!(
            std::io::stderr(),
            crossterm::event::PopKeyboardEnhancementFlags
        )?;
    }
    crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    tracing::info!("piki-multi-ai shutdown");
    result
}
