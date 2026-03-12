//! Request and response data transfer objects for the HTTP API.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ─── Backtest ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BacktestRequest {
    /// Path to CSV data file on server
    pub data_file: String,
    /// Securities to backtest (e.g. ["510050"])
    #[serde(default)]
    pub securities: Option<Vec<String>>,
    /// Timeframe: 1min, 5min, 15min, 30min, 60min, daily, weekly
    #[serde(default)]
    pub timeframe: Option<String>,
    /// Override start date (YYYY-MM-DD)
    #[serde(default)]
    pub start_date: Option<String>,
    /// Override end date (YYYY-MM-DD)
    #[serde(default)]
    pub end_date: Option<String>,
    /// Override initial capital
    #[serde(default)]
    pub capital: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct BacktestJobResponse {
    pub session_id: String,
    pub status: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct BacktestStatusResponse {
    pub session_id: String,
    pub status: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct BacktestResultsResponse {
    pub session_id: String,
    pub status: String,
    pub metrics: Option<BacktestMetricsDto>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BacktestMetricsDto {
    pub initial_capital: Decimal,
    pub final_equity: Decimal,
    pub total_pnl: Decimal,
    pub total_return_pct: Decimal,
    pub max_drawdown_pct: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: usize,
    pub num_winners: usize,
    pub num_losers: usize,
    pub avg_win: Decimal,
    pub avg_loss: Decimal,
    pub max_consecutive_losses: usize,
}

impl From<&brooks_backtester::BacktestMetrics> for BacktestMetricsDto {
    fn from(m: &brooks_backtester::BacktestMetrics) -> Self {
        Self {
            initial_capital: m.initial_capital,
            final_equity: m.final_equity,
            total_pnl: m.total_pnl,
            total_return_pct: m.total_return_pct,
            max_drawdown_pct: m.max_drawdown_pct,
            sharpe_ratio: m.sharpe_ratio,
            sortino_ratio: m.sortino_ratio,
            win_rate: m.win_rate,
            profit_factor: m.profit_factor,
            total_trades: m.total_trades,
            num_winners: m.num_winners,
            num_losers: m.num_losers,
            avg_win: m.avg_win,
            avg_loss: m.avg_loss,
            max_consecutive_losses: m.max_consecutive_losses,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TradeRecordDto {
    pub security: String,
    pub direction: String,
    pub quantity: u64,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub realized_pnl: Decimal,
    pub pnl_pct: Decimal,
    pub signal_type: String,
    pub exit_reason: String,
}

impl From<&brooks_backtester::TradeRecord> for TradeRecordDto {
    fn from(t: &brooks_backtester::TradeRecord) -> Self {
        Self {
            security: t.security.to_string(),
            direction: format!("{:?}", t.direction),
            quantity: t.quantity,
            entry_price: t.entry_price,
            exit_price: t.exit_price,
            entry_time: t.entry_time,
            exit_time: t.exit_time,
            realized_pnl: t.realized_pnl,
            pnl_pct: t.pnl_pct(),
            signal_type: format!("{:?}", t.signal_type),
            exit_reason: t.exit_reason.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EquityPointDto {
    pub timestamp: DateTime<Utc>,
    pub equity: Decimal,
    pub cash: Decimal,
    pub unrealized_pnl: Decimal,
}

impl From<&brooks_backtester::EquityPoint> for EquityPointDto {
    fn from(p: &brooks_backtester::EquityPoint) -> Self {
        Self {
            timestamp: p.timestamp,
            equity: p.equity,
            cash: p.cash,
            unrealized_pnl: p.unrealized_pnl,
        }
    }
}

// ─── Analysis ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AnalysisRequest {
    /// Path to CSV data file on server
    pub data_file: String,
    /// Security code (e.g. "510050")
    #[serde(default)]
    pub security: Option<String>,
    /// Timeframe: 1min, 5min, etc.
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
}

fn default_timeframe() -> String {
    "5min".to_string()
}

// ─── Data Fetch ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DataFetchRequest {
    /// Securities to fetch (e.g. ["510050"])
    #[serde(default)]
    pub securities: Option<Vec<String>>,
    /// Timeframe: 1min, 5min, 15min, 30min, 60min, daily, weekly
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    /// Start date (YYYY-MM-DD)
    pub start_date: String,
    /// End date (YYYY-MM-DD)
    pub end_date: String,
    /// Output directory for CSV files
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

fn default_output_dir() -> String {
    "./data".to_string()
}

#[derive(Debug, Serialize)]
pub struct DataFetchJobResponse {
    pub job_id: String,
    pub status: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct DataFetchStatusResponse {
    pub job_id: String,
    pub status: String,
    pub files: Vec<DataFetchFileResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataFetchFileResult {
    pub security: String,
    pub bar_count: usize,
    pub file_path: String,
}

// ─── Paper Trading ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaperTradingStartRequest {
    /// Securities to trade (e.g. ["510050"])
    #[serde(default)]
    pub securities: Option<Vec<String>>,
    /// Override initial capital
    #[serde(default)]
    pub capital: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PaperTradingStatusResponse {
    pub session_id: String,
    pub status: String,
    pub securities: Vec<String>,
    pub capital: Decimal,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct PaperTradingStopResponse {
    pub session_id: String,
    pub status: String,
    pub message: String,
}

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ConfigValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
}

// ─── Health ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct InfoResponse {
    pub version: String,
    pub features: Vec<String>,
}
