use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use brooks_core::market::{Direction, SecurityId};
use brooks_core::order::{Order, OrderId};

use crate::error::OmsError;

/// Represents a fill event from the execution venue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillEvent {
    /// The order that was filled (or partially filled).
    pub order_id: OrderId,
    /// Security that was traded.
    pub security: SecurityId,
    /// Direction of the fill (Long = buy, Short = sell).
    pub direction: Direction,
    /// Price at which the fill occurred.
    pub fill_price: Decimal,
    /// Quantity filled in this event.
    pub fill_quantity: u64,
    /// Timestamp of the fill.
    pub timestamp: DateTime<Utc>,
    /// Whether this is a partial fill (more quantity remains).
    pub is_partial: bool,
}

/// Trait for order execution backends.
///
/// Both `PaperExecutor` (local simulation) and `FutuExecutor` (live via FutuOpenD)
/// implement this trait. The `OrderManager` uses it through dynamic dispatch
/// (`Box<dyn OrderExecutor>`), making the OMS executor-agnostic.
#[async_trait]
pub trait OrderExecutor: Send + Sync {
    /// Submit an order for execution.
    ///
    /// Market orders may fill immediately (returned via `poll_fills`).
    /// Limit and stop orders are tracked internally until triggered.
    async fn submit(&mut self, order: &Order) -> Result<(), OmsError>;

    /// Cancel a pending order.
    async fn cancel(&mut self, order_id: &OrderId) -> Result<(), OmsError>;

    /// Modify a pending order's price and/or stop trigger.
    async fn modify(
        &mut self,
        order_id: &OrderId,
        new_price: Option<Decimal>,
        new_stop: Option<Decimal>,
    ) -> Result<(), OmsError>;

    /// Poll for fill events that have occurred since the last poll.
    ///
    /// Returns and drains the internal fill queue.
    async fn poll_fills(&mut self) -> Result<Vec<FillEvent>, OmsError>;

    /// Whether the executor is connected and ready to accept orders.
    fn is_ready(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;

    #[test]
    fn test_fill_event_construction() {
        let fill = FillEvent {
            order_id: OrderId::new(),
            security: SecurityId::etf("510050", Exchange::SH),
            direction: Direction::Long,
            fill_price: Decimal::new(3100, 3), // 3.100
            fill_quantity: 100,
            timestamp: Utc::now(),
            is_partial: false,
        };
        assert_eq!(fill.fill_quantity, 100);
        assert!(!fill.is_partial);
    }

    #[test]
    fn test_fill_event_partial() {
        let fill = FillEvent {
            order_id: OrderId::new(),
            security: SecurityId::etf("510300", Exchange::SH),
            direction: Direction::Short,
            fill_price: Decimal::new(4500, 3),
            fill_quantity: 50,
            timestamp: Utc::now(),
            is_partial: true,
        };
        assert!(fill.is_partial);
        assert_eq!(fill.fill_quantity, 50);
    }

    /// Verify OrderExecutor is object-safe.
    #[test]
    fn test_order_executor_is_object_safe() {
        // This test simply confirms that Box<dyn OrderExecutor> compiles.
        fn _assert_object_safe(_e: Box<dyn OrderExecutor>) {}
    }
}
