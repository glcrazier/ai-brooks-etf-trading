use rust_decimal::Decimal;
use thiserror::Error;

/// Errors that can occur during strategy execution.
#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("Insufficient capital: need {required}, have {available}")]
    InsufficientCapital { required: Decimal, available: Decimal },

    #[error("Maximum open positions reached: {max}")]
    MaxPositionsReached { max: usize },

    #[error("Daily loss limit reached: {current_loss_pct}% >= {limit_pct}%")]
    DailyLossLimitReached {
        current_loss_pct: Decimal,
        limit_pct: Decimal,
    },

    #[error("Market not open for trading")]
    MarketClosed,

    #[error("Security {0} follows T+1 settlement (no same-day round trip)")]
    NoIntradayRoundTrip(String),

    #[error("Warm-up period not complete: {bars_processed}/{bars_required}")]
    WarmUpIncomplete {
        bars_processed: u64,
        bars_required: u64,
    },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Position not found: {0}")]
    PositionNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_insufficient_capital_display() {
        let err = StrategyError::InsufficientCapital {
            required: dec!(50000),
            available: dec!(10000),
        };
        assert_eq!(
            err.to_string(),
            "Insufficient capital: need 50000, have 10000"
        );
    }

    #[test]
    fn test_max_positions_display() {
        let err = StrategyError::MaxPositionsReached { max: 3 };
        assert_eq!(err.to_string(), "Maximum open positions reached: 3");
    }

    #[test]
    fn test_daily_loss_limit_display() {
        let err = StrategyError::DailyLossLimitReached {
            current_loss_pct: dec!(3.5),
            limit_pct: dec!(3.0),
        };
        assert_eq!(
            err.to_string(),
            "Daily loss limit reached: 3.5% >= 3.0%"
        );
    }

    #[test]
    fn test_market_closed_display() {
        let err = StrategyError::MarketClosed;
        assert_eq!(err.to_string(), "Market not open for trading");
    }

    #[test]
    fn test_no_intraday_round_trip_display() {
        let err = StrategyError::NoIntradayRoundTrip("600000".to_string());
        assert_eq!(
            err.to_string(),
            "Security 600000 follows T+1 settlement (no same-day round trip)"
        );
    }

    #[test]
    fn test_warm_up_incomplete_display() {
        let err = StrategyError::WarmUpIncomplete {
            bars_processed: 50,
            bars_required: 200,
        };
        assert_eq!(
            err.to_string(),
            "Warm-up period not complete: 50/200"
        );
    }

    #[test]
    fn test_invalid_config_display() {
        let err = StrategyError::InvalidConfig("missing ema_period".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid configuration: missing ema_period"
        );
    }

    #[test]
    fn test_position_not_found_display() {
        let err = StrategyError::PositionNotFound("510050".to_string());
        assert_eq!(err.to_string(), "Position not found: 510050");
    }
}
