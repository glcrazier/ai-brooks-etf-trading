use brooks_strategy::StrategyError;
use thiserror::Error;

/// Errors that can occur during backtesting.
#[derive(Debug, Error)]
pub enum BacktestError {
    #[error("No data: the data feed is empty")]
    NoData,

    #[error("Strategy error: {0}")]
    Strategy(#[from] StrategyError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Insufficient funds: need {required}, have {available}")]
    InsufficientFunds {
        required: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Position not found: {0}")]
    PositionNotFound(String),

    #[error("T+1 settlement violation: cannot sell {security} on the same day it was bought")]
    T1SettlementViolation { security: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_no_data_display() {
        let err = BacktestError::NoData;
        assert_eq!(err.to_string(), "No data: the data feed is empty");
    }

    #[test]
    fn test_invalid_config_display() {
        let err = BacktestError::InvalidConfig("slippage must be non-negative".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid configuration: slippage must be non-negative"
        );
    }

    #[test]
    fn test_insufficient_funds_display() {
        let err = BacktestError::InsufficientFunds {
            required: dec!(50000),
            available: dec!(10000),
        };
        assert_eq!(
            err.to_string(),
            "Insufficient funds: need 50000, have 10000"
        );
    }

    #[test]
    fn test_order_not_found_display() {
        let err = BacktestError::OrderNotFound("abc-123".to_string());
        assert_eq!(err.to_string(), "Order not found: abc-123");
    }

    #[test]
    fn test_position_not_found_display() {
        let err = BacktestError::PositionNotFound("510050.SH".to_string());
        assert_eq!(err.to_string(), "Position not found: 510050.SH");
    }

    #[test]
    fn test_from_strategy_error() {
        let strategy_err = StrategyError::MarketClosed;
        let backtest_err: BacktestError = strategy_err.into();
        assert!(backtest_err.to_string().contains("Market not open"));
    }

    #[test]
    fn test_t1_settlement_violation_display() {
        let err = BacktestError::T1SettlementViolation {
            security: "510050.SH".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "T+1 settlement violation: cannot sell 510050.SH on the same day it was bought"
        );
    }
}
