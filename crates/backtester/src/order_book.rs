use std::collections::HashMap;

use brooks_core::market::SecurityId;
use brooks_core::order::{Order, OrderId};
use brooks_core::signal::Signal;
use rust_decimal::Decimal;

use crate::error::BacktestError;

/// An order paired with its originating signal for trade logging.
#[derive(Debug, Clone)]
pub struct PendingOrder {
    pub order: Order,
    pub signal: Signal,
}

/// Tracks all pending (unfilled) orders during a backtest.
pub struct OrderBook {
    orders: HashMap<OrderId, PendingOrder>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
        }
    }

    /// Add a new order to the book.
    pub fn add(&mut self, order: Order, signal: Signal) {
        let id = order.id.clone();
        self.orders.insert(id, PendingOrder { order, signal });
    }

    /// Cancel an order by ID. Returns the cancelled order or error.
    pub fn cancel(&mut self, order_id: &OrderId) -> Result<PendingOrder, BacktestError> {
        self.orders
            .remove(order_id)
            .ok_or_else(|| BacktestError::OrderNotFound(format!("{:?}", order_id)))
    }

    /// Modify an order's price and/or stop.
    pub fn modify(
        &mut self,
        order_id: &OrderId,
        new_price: Option<Decimal>,
        new_stop: Option<Decimal>,
    ) -> Result<(), BacktestError> {
        let pending = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| BacktestError::OrderNotFound(format!("{:?}", order_id)))?;

        if let Some(price) = new_price {
            pending.order.price = Some(price);
        }
        if let Some(stop) = new_stop {
            pending.order.stop_price = Some(stop);
        }
        Ok(())
    }

    /// Remove and return an order (used after fill).
    pub fn remove(&mut self, order_id: &OrderId) -> Option<PendingOrder> {
        self.orders.remove(order_id)
    }

    /// Iterate over all pending orders (immutable).
    pub fn iter(&self) -> impl Iterator<Item = &PendingOrder> {
        self.orders.values()
    }

    /// Collect all order IDs (for iteration + mutation pattern).
    pub fn order_ids(&self) -> Vec<OrderId> {
        self.orders.keys().cloned().collect()
    }

    /// Number of pending orders.
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Cancel all pending orders for a given security.
    pub fn cancel_all_for_security(&mut self, security: &SecurityId) -> Vec<PendingOrder> {
        let ids_to_remove: Vec<OrderId> = self
            .orders
            .iter()
            .filter(|(_, po)| po.order.security == *security)
            .map(|(id, _)| id.clone())
            .collect();

        ids_to_remove
            .into_iter()
            .filter_map(|id| self.orders.remove(&id))
            .collect()
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Direction, Exchange};
    use brooks_core::signal::{SignalContext, SignalType};
    use brooks_core::timeframe::Timeframe;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    fn make_signal() -> Signal {
        Signal {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            security: security(),
            direction: Direction::Long,
            signal_type: SignalType::PullbackEntry,
            entry_price: dec!(3.100),
            stop_price: dec!(3.050),
            target_price: Some(dec!(3.200)),
            confidence: 0.8,
            timeframe: Timeframe::Minute5,
            context: SignalContext::default(),
        }
    }

    #[test]
    fn test_add_and_len() {
        let mut book = OrderBook::new();
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        book.add(order, make_signal());
        assert_eq!(book.len(), 1);
        assert!(!book.is_empty());
    }

    #[test]
    fn test_cancel_existing() {
        let mut book = OrderBook::new();
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        let id = order.id.clone();
        book.add(order, make_signal());

        let cancelled = book.cancel(&id).unwrap();
        assert_eq!(cancelled.order.id, id);
        assert!(book.is_empty());
    }

    #[test]
    fn test_cancel_nonexistent() {
        let mut book = OrderBook::new();
        let result = book.cancel(&OrderId::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_modify_price() {
        let mut book = OrderBook::new();
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        let id = order.id.clone();
        book.add(order, make_signal());

        book.modify(&id, Some(dec!(3.150)), None).unwrap();
        let po = book.iter().next().unwrap();
        assert_eq!(po.order.price, Some(dec!(3.150)));
    }

    #[test]
    fn test_modify_stop() {
        let mut book = OrderBook::new();
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        let id = order.id.clone();
        book.add(order, make_signal());

        book.modify(&id, None, Some(dec!(3.050))).unwrap();
        let po = book.iter().next().unwrap();
        assert_eq!(po.order.stop_price, Some(dec!(3.050)));
    }

    #[test]
    fn test_remove() {
        let mut book = OrderBook::new();
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        let id = order.id.clone();
        book.add(order, make_signal());

        let removed = book.remove(&id);
        assert!(removed.is_some());
        assert!(book.is_empty());
    }

    #[test]
    fn test_cancel_all_for_security() {
        let mut book = OrderBook::new();
        let o1 = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        let o2 = Order::stop(security(), Direction::Long, 200, dec!(3.200));
        let other_sec = SecurityId::etf("510300", Exchange::SH);
        let o3 = Order::stop(other_sec, Direction::Long, 100, dec!(4.100));

        book.add(o1, make_signal());
        book.add(o2, make_signal());
        book.add(o3, make_signal());
        assert_eq!(book.len(), 3);

        let cancelled = book.cancel_all_for_security(&security());
        assert_eq!(cancelled.len(), 2);
        assert_eq!(book.len(), 1);
    }

    #[test]
    fn test_order_ids() {
        let mut book = OrderBook::new();
        let o1 = Order::stop(security(), Direction::Long, 100, dec!(3.100));
        let id1 = o1.id.clone();
        book.add(o1, make_signal());

        let ids = book.order_ids();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], id1);
    }
}
