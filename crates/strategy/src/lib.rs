// Strategy framework and Brooks PA strategy implementation

pub mod brooks;
pub mod config;
pub mod error;
pub mod mtf;
pub mod position_manager;
pub mod risk;
pub mod session_filter;
pub mod signal_generator;
pub mod stop_target;
pub mod traits;

// Re-exports
pub use brooks::BrooksStrategy;
pub use config::StrategyConfig;
pub use error::StrategyError;
pub use traits::{Strategy, StrategyAction};
