mod action;
mod app;
mod clipboard;
mod config;
mod dialog_state;
mod event_loop;
mod helpers;
mod input;
mod pty;
mod theme;
mod ui;

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
}

#[derive(Subcommand)]
enum Commands {
    /// Generates the default configuration file to stdout
    GenerateConfig,
    /// Shows version and author information (same as About in-app)
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

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
        }
    }

    // Initialize structured logging to file
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("piki-multi/logs");
    let file_appender = tracing_appender::rolling::daily(&log_dir, "piki-multi.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let level_filter = match cli.log_level.to_lowercase().as_str() {
        "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
        "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
        "warn" => tracing_subscriber::filter::LevelFilter::WARN,
        "error" => tracing_subscriber::filter::LevelFilter::ERROR,
        _ => tracing_subscriber::filter::LevelFilter::INFO,
    };

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_max_level(level_filter)
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

    // Install panic hook that restores terminal before printing panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture);
        ratatui::restore();
        original_hook(panic_info);
    }));

    let terminal = ratatui::init();
    crossterm::execute!(std::io::stderr(), crossterm::event::EnableMouseCapture)?;
    let result = event_loop::run(terminal, preflight.warnings).await;
    crossterm::execute!(std::io::stderr(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    tracing::info!("piki-multi-ai shutdown");
    result
}
