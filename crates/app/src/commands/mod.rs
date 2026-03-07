pub mod analyze;
pub mod backtest;
pub mod config_cmd;
pub mod fetch_data;
pub mod paper;

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

use crate::cli::Args;

/// Combined application configuration loaded from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub futu: brooks_market_data::FutuConfig,
    pub market: MarketSection,
    pub strategy: brooks_strategy::StrategyConfig,
    pub backtest: brooks_backtester::BacktestConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// The [market] section of the config file.
#[derive(Debug, Clone, Deserialize)]
pub struct MarketSection {
    pub exchange: String,
    pub securities: Vec<String>,
    pub primary_timeframe: String,
    pub context_timeframe: String,
}

/// The [logging] section of the config file.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    pub file: Option<String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

/// Load and parse configuration from a TOML file path.
pub fn load_config(args: &Args) -> Result<AppConfig> {
    let config_path = &args.config;
    let contents = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read config file: {}", config_path))?;
    let app_config: AppConfig =
        toml::from_str(&contents).with_context(|| "failed to parse config file")?;
    Ok(app_config)
}

/// Initialize the tracing/logging subsystem.
pub fn init_logging(config: &LoggingConfig, level_override: Option<&str>) {
    let level = level_override.unwrap_or(&config.level);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    match config.format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_target(true)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .init();
        }
    }
}

/// Parse an exchange string into an Exchange enum.
pub fn parse_exchange(s: &str) -> Result<brooks_core::market::Exchange> {
    match s.to_uppercase().as_str() {
        "SH" => Ok(brooks_core::market::Exchange::SH),
        "SZ" => Ok(brooks_core::market::Exchange::SZ),
        _ => anyhow::bail!("unknown exchange '{}': expected SH or SZ", s),
    }
}

/// Parse a comma-separated list of security codes into SecurityId instances.
pub fn parse_securities(
    codes: &[String],
    exchange: brooks_core::market::Exchange,
) -> Vec<brooks_core::market::SecurityId> {
    codes
        .iter()
        .map(|code| brooks_core::market::SecurityId::etf(code.trim(), exchange))
        .collect()
}

/// Parse a timeframe string into a Timeframe enum.
pub fn parse_timeframe(s: &str) -> Result<brooks_core::timeframe::Timeframe> {
    s.parse::<brooks_core::timeframe::Timeframe>()
        .map_err(|e| anyhow::anyhow!(e))
}
