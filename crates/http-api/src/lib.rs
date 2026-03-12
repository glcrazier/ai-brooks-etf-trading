//! HTTP API layer for the Brooks ETF Trading System.
//!
//! Wraps existing CLI features (backtest, paper trading, data fetch, analysis,
//! configuration) as JSON REST endpoints using Axum.

pub mod app;
pub mod dto;
pub mod error;
pub mod handlers;
pub mod session;
pub mod state;
