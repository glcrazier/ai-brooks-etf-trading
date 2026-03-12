//! Session lifecycle management for async jobs (backtest, data fetch, paper trading).

use std::sync::Arc;

use brooks_backtester::{BacktestMetrics, EquityPoint, TradeRecord};
use tokio::sync::RwLock;

use crate::dto::DataFetchFileResult;

/// Unique session identifier.
pub type SessionId = String;

/// Status of an async job session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Completed => write!(f, "completed"),
            SessionStatus::Failed => write!(f, "failed"),
            SessionStatus::Stopped => write!(f, "stopped"),
        }
    }
}

/// Holds the result of a completed or running backtest session.
#[derive(Debug, Clone)]
pub struct BacktestSession {
    pub status: SessionStatus,
    pub metrics: Option<BacktestMetrics>,
    pub trades: Vec<TradeRecord>,
    pub equity_curve: Vec<EquityPoint>,
    pub error: Option<String>,
}

/// Holds the state of a data fetch job.
#[derive(Debug, Clone)]
pub struct DataFetchSession {
    pub status: SessionStatus,
    pub files: Vec<DataFetchFileResult>,
    pub error: Option<String>,
}

/// Holds the state of a paper trading session.
pub struct PaperTradingSession {
    pub status: SessionStatus,
    pub securities: Vec<String>,
    pub capital: rust_decimal::Decimal,
    /// Cancellation handle to stop the event loop
    pub cancel_token: tokio::sync::watch::Sender<bool>,
    pub error: Option<String>,
}

/// Thread-safe session wrapper.
pub type SharedSession<T> = Arc<RwLock<T>>;
