use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::market::{Direction, SecurityId};

/// Unique order identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub Uuid);

impl OrderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

/// Type of order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Execute at current market price
    Market,
    /// Execute at specified price or better
    Limit,
    /// Trigger when price reaches stop level, then becomes market order
    Stop,
    /// Trigger when price reaches stop level, then becomes limit order
    StopLimit,
}

/// Current status of an order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Created but not yet submitted
    Pending,
    /// Submitted to exchange/executor
    Submitted,
    /// Partially filled
    PartiallyFilled,
    /// Completely filled
    Filled,
    /// Cancelled
    Cancelled,
    /// Rejected by exchange/executor
    Rejected,
}

/// A trading order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub security: SecurityId,
    pub direction: Direction,
    pub order_type: OrderType,
    pub quantity: u64,
    /// Limit price (None for market orders)
    pub price: Option<Decimal>,
    /// Stop trigger price
    pub stop_price: Option<Decimal>,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub filled_at: Option<DateTime<Utc>>,
    pub filled_price: Option<Decimal>,
    pub filled_quantity: u64,
}

impl Order {
    /// Create a new market order
    pub fn market(security: SecurityId, direction: Direction, quantity: u64) -> Self {
        Self {
            id: OrderId::new(),
            security,
            direction,
            order_type: OrderType::Market,
            quantity,
            price: None,
            stop_price: None,
            status: OrderStatus::Pending,
            created_at: Utc::now(),
            filled_at: None,
            filled_price: None,
            filled_quantity: 0,
        }
    }

    /// Create a new limit order
    pub fn limit(
        security: SecurityId,
        direction: Direction,
        quantity: u64,
        price: Decimal,
    ) -> Self {
        Self {
            id: OrderId::new(),
            security,
            direction,
            order_type: OrderType::Limit,
            quantity,
            price: Some(price),
            stop_price: None,
            status: OrderStatus::Pending,
            created_at: Utc::now(),
            filled_at: None,
            filled_price: None,
            filled_quantity: 0,
        }
    }

    /// Create a new stop order
    pub fn stop(
        security: SecurityId,
        direction: Direction,
        quantity: u64,
        stop_price: Decimal,
    ) -> Self {
        Self {
            id: OrderId::new(),
            security,
            direction,
            order_type: OrderType::Stop,
            quantity,
            price: None,
            stop_price: Some(stop_price),
            status: OrderStatus::Pending,
            created_at: Utc::now(),
            filled_at: None,
            filled_price: None,
            filled_quantity: 0,
        }
    }

    /// Whether the order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Rejected
        )
    }

    /// Whether the order is still active (can be filled or cancelled)
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Pending | OrderStatus::Submitted | OrderStatus::PartiallyFilled
        )
    }

    /// Remaining unfilled quantity
    pub fn remaining_quantity(&self) -> u64 {
        self.quantity.saturating_sub(self.filled_quantity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Exchange;
    use rust_decimal_macros::dec;

    #[test]
    fn test_market_order() {
        let order = Order::market(
            SecurityId::etf("510050", Exchange::SH),
            Direction::Long,
            100,
        );
        assert_eq!(order.order_type, OrderType::Market);
        assert_eq!(order.quantity, 100);
        assert!(order.price.is_none());
        assert_eq!(order.status, OrderStatus::Pending);
        assert!(order.is_active());
        assert!(!order.is_terminal());
    }

    #[test]
    fn test_limit_order() {
        let order = Order::limit(
            SecurityId::etf("510050", Exchange::SH),
            Direction::Long,
            200,
            dec!(3.100),
        );
        assert_eq!(order.order_type, OrderType::Limit);
        assert_eq!(order.price, Some(dec!(3.100)));
    }

    #[test]
    fn test_stop_order() {
        let order = Order::stop(
            SecurityId::etf("510050", Exchange::SH),
            Direction::Short,
            100,
            dec!(3.050),
        );
        assert_eq!(order.order_type, OrderType::Stop);
        assert_eq!(order.stop_price, Some(dec!(3.050)));
    }

    #[test]
    fn test_remaining_quantity() {
        let mut order = Order::market(
            SecurityId::etf("510050", Exchange::SH),
            Direction::Long,
            300,
        );
        order.filled_quantity = 100;
        assert_eq!(order.remaining_quantity(), 200);
    }

    #[test]
    fn test_terminal_states() {
        let mut order = Order::market(
            SecurityId::etf("510050", Exchange::SH),
            Direction::Long,
            100,
        );
        order.status = OrderStatus::Filled;
        assert!(order.is_terminal());
        assert!(!order.is_active());
    }
}
