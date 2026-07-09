//! RustMine — A high-performance Minecraft Bedrock server written in Rust.
//!
//! Entrypoint for the server binary. Handles CLI parsing, configuration
//! loading, logging setup, and starts the main event loop.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod config;

/// Command-line arguments for the RustMine server.
#[derive(Parser, Debug)]
#[command(
    name = "rustmine",
    version,
    about = "A high-performance Minecraft Bedrock server"
)]
struct Cli {
    /// Path to the server configuration file (TOML).
    #[arg(short, long, default_value = "server.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let cfg = config::ServerConfig::load(&cli.config)
        .with_context(|| format!("Failed to load config from {}", cli.config.display()))?;

    info!(
        name = cfg.server.name.as_str(),
        port = cfg.server.port,
        motd = cfg.server.motd.as_str(),
        view_distance = cfg.game.view_distance,
        "RustMine server starting"
    );

    info!("Configuration loaded successfully. Server ready (Phase 0 — no networking yet).");

    Ok(())
}
