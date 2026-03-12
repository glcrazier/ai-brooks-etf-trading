//! Brooks ETF Trading HTTP Server
//!
//! Exposes all trading system features as JSON REST endpoints.
//! Run with: cargo run --bin brooks-server

use std::net::SocketAddr;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use brooks_http_api::app::build_router;
use brooks_http_api::state::{AppState, ServerAppConfig};

/// Brooks ETF Trading HTTP Server
#[derive(Parser, Debug)]
#[command(name = "brooks-server")]
#[command(version, about = "Brooks ETF Trading System - HTTP API Server")]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/default.toml")]
    config: String,

    /// Log level override (trace, debug, info, warn, error)
    #[arg(long)]
    log_level: Option<String>,

    /// Override server host
    #[arg(long)]
    host: Option<String>,

    /// Override server port
    #[arg(long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let contents = std::fs::read_to_string(&args.config)
        .with_context(|| format!("failed to read config file: {}", args.config))?;
    let mut config: ServerAppConfig =
        toml::from_str(&contents).with_context(|| "failed to parse config file")?;

    // Apply CLI overrides
    if let Some(ref host) = args.host {
        config.http_server.host = host.clone();
    }
    if let Some(port) = args.port {
        config.http_server.port = port;
    }

    // Initialize logging
    let level = args
        .log_level
        .as_deref()
        .unwrap_or(&config.logging.level);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let addr: SocketAddr = format!("{}:{}", config.http_server.host, config.http_server.port)
        .parse()
        .with_context(|| {
            format!(
                "invalid server address: {}:{}",
                config.http_server.host, config.http_server.port
            )
        })?;

    info!("Starting Brooks ETF Trading HTTP Server");
    info!("Listening on http://{}", addr);
    info!("Config: {}", args.config);

    let state = AppState::new(config);
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind to {}", addr))?;

    info!("Server ready. Press Ctrl+C to stop.");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    info!("Server shut down gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
    info!("Shutdown signal received");
}
