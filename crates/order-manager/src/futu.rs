use async_trait::async_trait;
use rust_decimal::Decimal;

use brooks_core::order::{Order, OrderId};

use crate::error::OmsError;
use crate::executor::{FillEvent, OrderExecutor};

/// Futu OpenD executor for live order execution.
///
/// This is currently a stub. When implemented, it will:
/// - Connect to FutuOpenD via TCP/protobuf
/// - Submit real orders to the exchange
/// - Receive fill callbacks asynchronously
/// - Support order modification and cancellation
pub struct FutuExecutor {
    connected: bool,
}

impl FutuExecutor {
    /// Create a new (unconnected) Futu executor.
    pub fn new() -> Self {
        Self { connected: false }
    }
}

impl Default for FutuExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OrderExecutor for FutuExecutor {
    async fn submit(&mut self, _order: &Order) -> Result<(), OmsError> {
        if !self.connected {
            return Err(OmsError::ExecutorError(
                "FutuExecutor is not connected".to_string(),
            ));
        }
        Err(OmsError::ExecutorError(
            "Futu order execution not yet implemented".to_string(),
        ))
    }

    async fn cancel(&mut self, _order_id: &OrderId) -> Result<(), OmsError> {
        if !self.connected {
            return Err(OmsError::ExecutorError(
                "FutuExecutor is not connected".to_string(),
            ));
        }
        Err(OmsError::ExecutorError(
            "Futu order cancellation not yet implemented".to_string(),
        ))
    }

    async fn modify(
        &mut self,
        _order_id: &OrderId,
        _new_price: Option<Decimal>,
        _new_stop: Option<Decimal>,
    ) -> Result<(), OmsError> {
        if !self.connected {
            return Err(OmsError::ExecutorError(
                "FutuExecutor is not connected".to_string(),
            ));
        }
        Err(OmsError::ExecutorError(
            "Futu order modification not yet implemented".to_string(),
        ))
    }

    async fn poll_fills(&mut self) -> Result<Vec<FillEvent>, OmsError> {
        if !self.connected {
            return Err(OmsError::ExecutorError(
                "FutuExecutor is not connected".to_string(),
            ));
        }
        Ok(vec![])
    }

    fn is_ready(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Direction, Exchange, SecurityId};

    #[test]
    fn test_is_ready_when_not_connected() {
        let exec = FutuExecutor::new();
        assert!(!exec.is_ready());
    }

    #[tokio::test]
    async fn test_submit_fails_when_not_connected() {
        let mut exec = FutuExecutor::new();
        let order = Order::market(
            SecurityId::etf("510050", Exchange::SH),
            Direction::Long,
            100,
        );
        let result = exec.submit(&order).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not connected"));
    }
}
