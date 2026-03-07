//! Backtesting engine for the Brooks PA trading system.
//!
//! Replays historical bars through a `Strategy`, simulates order fills,
//! tracks portfolio state, and computes performance metrics.

pub mod config;
pub mod engine;
pub mod error;
pub mod fill_model;
pub mod metrics;
pub mod order_book;
pub mod portfolio;
pub mod trade_log;

// Re-exports
pub use config::BacktestConfig;
pub use engine::{BacktestEngine, BacktestResult};
pub use error::BacktestError;
pub use fill_model::{FillModel, FillResult, NextBarOpenFill};
pub use metrics::BacktestMetrics;
pub use order_book::OrderBook;
pub use portfolio::{EquityPoint, Portfolio};
pub use trade_log::{TradeLog, TradeRecord};
