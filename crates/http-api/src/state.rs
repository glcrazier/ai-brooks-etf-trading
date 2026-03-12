//! Application state shared across all HTTP handlers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use brooks_market_data::FutuConfig;
use brooks_strategy::StrategyConfig;
use brooks_backtester::BacktestConfig;
use tokio::sync::RwLock;

use crate::session::{
    BacktestSession, DataFetchSession, PaperTradingSession, SessionId, SharedSession,
};

/// Server configuration for the HTTP layer.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HttpServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

/// Combined application configuration (mirrors the CLI AppConfig, with HTTP section).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ServerAppConfig {
    pub futu: FutuConfig,
    pub market: MarketSection,
    pub strategy: StrategyConfig,
    pub backtest: BacktestConfig,
    #[serde(default)]
    pub http_server: HttpServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// The [market] section of the config file.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MarketSection {
    pub exchange: String,
    pub securities: Vec<String>,
    pub primary_timeframe: String,
    pub context_timeframe: String,
}

/// The [logging] section.
#[derive(Debug, Clone, serde::Deserialize)]
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

/// Shared application state accessible from all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ServerAppConfig>,
    pub start_time: Instant,
    pub backtest_sessions: Arc<RwLock<HashMap<SessionId, SharedSession<BacktestSession>>>>,
    pub data_fetch_sessions: Arc<RwLock<HashMap<SessionId, SharedSession<DataFetchSession>>>>,
    pub paper_sessions: Arc<RwLock<HashMap<SessionId, SharedSession<PaperTradingSession>>>>,
}

impl AppState {
    pub fn new(config: ServerAppConfig) -> Self {
        Self {
            config: Arc::new(config),
            start_time: Instant::now(),
            backtest_sessions: Arc::new(RwLock::new(HashMap::new())),
            data_fetch_sessions: Arc::new(RwLock::new(HashMap::new())),
            paper_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
