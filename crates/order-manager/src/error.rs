use thiserror::Error;

/// Errors that can occur in the order management system.
#[derive(Debug, Error)]
pub enum OmsError {
    /// Order with the given ID was not found.
    #[error("Order not found: {0}")]
    OrderNotFound(String),

    /// Order was rejected by the executor or validation.
    #[error("Order rejected: {0}")]
    OrderRejected(String),

    /// An order with this ID already exists.
    #[error("Duplicate order ID: {0}")]
    DuplicateOrder(String),

    /// A Chinese market rule was violated (lot size, T+1, price limits, etc.).
    #[error("Market rules violation: {0}")]
    MarketRulesViolation(String),

    /// The underlying executor returned an error.
    #[error("Executor error: {0}")]
    ExecutorError(String),

    /// No position exists for the given security.
    #[error("Position not found: {0}")]
    PositionNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_not_found_display() {
        let err = OmsError::OrderNotFound("abc-123".to_string());
        assert_eq!(err.to_string(), "Order not found: abc-123");
    }

    #[test]
    fn test_order_rejected_display() {
        let err = OmsError::OrderRejected("insufficient margin".to_string());
        assert_eq!(err.to_string(), "Order rejected: insufficient margin");
    }

    #[test]
    fn test_duplicate_order_display() {
        let err = OmsError::DuplicateOrder("order-42".to_string());
        assert_eq!(err.to_string(), "Duplicate order ID: order-42");
    }

    #[test]
    fn test_market_rules_violation_display() {
        let err = OmsError::MarketRulesViolation("lot size must be multiple of 100".to_string());
        assert_eq!(
            err.to_string(),
            "Market rules violation: lot size must be multiple of 100"
        );
    }

    #[test]
    fn test_executor_error_display() {
        let err = OmsError::ExecutorError("connection refused".to_string());
        assert_eq!(err.to_string(), "Executor error: connection refused");
    }

    #[test]
    fn test_position_not_found_display() {
        let err = OmsError::PositionNotFound("510050.SH".to_string());
        assert_eq!(err.to_string(), "Position not found: 510050.SH");
    }
}
