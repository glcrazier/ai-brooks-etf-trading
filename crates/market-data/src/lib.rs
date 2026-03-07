//! Market data pipeline and Futu OpenD client.
//!
//! This crate provides:
//! - **Traits**: `MarketDataProvider` (async data access) and `DataFeed` (sync bar iteration)
//! - **Futu integration**: TCP/protobuf client for Futu OpenD
//! - **Bar aggregation**: Tick-to-bar conversion respecting China market sessions
//! - **Data feeds**: `VecDataFeed`, `HistoricalDataFeed` for backtesting

pub mod aggregator;
pub mod config;
pub mod error;
pub mod feed;
pub mod futu;
pub mod provider;
pub mod traits;

// Re-exports for convenience
pub use config::{FutuConfig, MarketDataConfig};
pub use error::MarketDataError;
pub use feed::{HistoricalDataFeed, VecDataFeed};
pub use provider::{FutuMarketDataProvider, MockMarketDataProvider};
pub use traits::{DataFeed, MarketDataProvider};
