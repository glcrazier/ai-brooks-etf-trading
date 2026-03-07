//! Order management system for the Brooks PA trading system.
//!
//! Provides an `OrderExecutor` trait with two implementations:
//! - `PaperExecutor`: simulates fills locally for paper trading
//! - `FutuExecutor`: stub for live execution via Futu OpenD

pub mod error;
pub mod executor;
pub mod futu;
pub mod manager;
pub mod paper;

// Re-exports
pub use error::OmsError;
pub use executor::{FillEvent, OrderExecutor};
pub use futu::FutuExecutor;
pub use manager::OrderManager;
pub use paper::PaperExecutor;
