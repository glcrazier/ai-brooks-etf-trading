use std::collections::HashMap;

use tracing::{debug, info};

use brooks_core::market::SecurityId;
use brooks_core::order::{Order, OrderId, OrderStatus};
use rust_decimal::Decimal;

use crate::error::OmsError;
use crate::executor::{FillEvent, OrderExecutor};

/// Central order management system.
///
/// The `OrderManager` coordinates between higher-level trading logic
/// (strategy, app layer) and the `OrderExecutor` (paper or live).
/// It tracks active orders, dispatches them to the executor, processes
/// fill events, and maintains an order history.
pub struct OrderManager {
    /// The underlying executor (PaperExecutor or FutuExecutor).
    executor: Box<dyn OrderExecutor>,
    /// Currently active (non-terminal) orders.
    active_orders: HashMap<OrderId, Order>,
    /// Completed/cancelled order history.
    order_history: Vec<Order>,
}

impl OrderManager {
    /// Create a new order manager with the given executor.
    pub fn new(executor: Box<dyn OrderExecutor>) -> Self {
        Self {
            executor,
            active_orders: HashMap::new(),
            order_history: Vec::new(),
        }
    }

    /// Submit an order for execution.
    ///
    /// The order is forwarded to the executor and tracked as active.
    pub async fn submit_order(&mut self, order: Order) -> Result<(), OmsError> {
        if self.active_orders.contains_key(&order.id) {
            return Err(OmsError::DuplicateOrder(format!("{:?}", order.id)));
        }

        self.executor.submit(&order).await?;

        let mut tracked = order;
        if tracked.status == OrderStatus::Pending {
            tracked.status = OrderStatus::Submitted;
        }

        debug!(
            order_id = ?tracked.id,
            security = %tracked.security,
            direction = ?tracked.direction,
            order_type = ?tracked.order_type,
            qty = tracked.quantity,
            "Order submitted to executor"
        );

        self.active_orders.insert(tracked.id.clone(), tracked);
        Ok(())
    }

    /// Cancel a pending order.
    pub async fn cancel_order(
        &mut self,
        order_id: &OrderId,
        reason: &str,
    ) -> Result<(), OmsError> {
        if !self.active_orders.contains_key(order_id) {
            return Err(OmsError::OrderNotFound(format!("{:?}", order_id)));
        }

        self.executor.cancel(order_id).await?;

        if let Some(mut order) = self.active_orders.remove(order_id) {
            order.status = OrderStatus::Cancelled;
            debug!(
                order_id = ?order_id,
                reason = reason,
                "Order cancelled"
            );
            self.order_history.push(order);
        }

        Ok(())
    }

    /// Modify a pending order's price and/or stop trigger.
    pub async fn modify_order(
        &mut self,
        order_id: &OrderId,
        new_price: Option<Decimal>,
        new_stop: Option<Decimal>,
    ) -> Result<(), OmsError> {
        if !self.active_orders.contains_key(order_id) {
            return Err(OmsError::OrderNotFound(format!("{:?}", order_id)));
        }

        self.executor.modify(order_id, new_price, new_stop).await?;

        if let Some(order) = self.active_orders.get_mut(order_id) {
            if let Some(price) = new_price {
                order.price = Some(price);
            }
            if let Some(stop) = new_stop {
                order.stop_price = Some(stop);
            }
        }

        Ok(())
    }

    /// Poll the executor for fills, update order states, and return fill events.
    ///
    /// Filled orders are moved from active_orders to order_history.
    pub async fn poll_fills(&mut self) -> Result<Vec<FillEvent>, OmsError> {
        let fills = self.executor.poll_fills().await?;

        for fill in &fills {
            if let Some(mut order) = self.active_orders.remove(&fill.order_id) {
                order.filled_price = Some(fill.fill_price);
                order.filled_quantity += fill.fill_quantity;
                order.filled_at = Some(fill.timestamp);

                if fill.is_partial {
                    order.status = OrderStatus::PartiallyFilled;
                    // Put back — still active
                    self.active_orders.insert(order.id.clone(), order);
                } else {
                    order.status = OrderStatus::Filled;
                    info!(
                        order_id = ?order.id,
                        security = %order.security,
                        fill_price = %fill.fill_price,
                        fill_qty = fill.fill_quantity,
                        "Order filled"
                    );
                    self.order_history.push(order);
                }
            }
        }

        Ok(fills)
    }

    /// Cancel all active orders for a specific security.
    pub async fn cancel_all_for_security(
        &mut self,
        security: &SecurityId,
    ) -> Result<Vec<OrderId>, OmsError> {
        let matching_ids: Vec<OrderId> = self
            .active_orders
            .iter()
            .filter(|(_, order)| order.security == *security)
            .map(|(id, _)| id.clone())
            .collect();

        let mut cancelled = Vec::new();
        for order_id in &matching_ids {
            // Best effort — ignore errors for individual cancellations
            if self.executor.cancel(order_id).await.is_ok() {
                if let Some(mut order) = self.active_orders.remove(order_id) {
                    order.status = OrderStatus::Cancelled;
                    self.order_history.push(order);
                    cancelled.push(order_id.clone());
                }
            }
        }

        Ok(cancelled)
    }

    /// Cancel all active orders.
    pub async fn cancel_all_orders(&mut self) -> Result<Vec<OrderId>, OmsError> {
        let all_ids: Vec<OrderId> = self.active_orders.keys().cloned().collect();

        let mut cancelled = Vec::new();
        for order_id in &all_ids {
            if self.executor.cancel(order_id).await.is_ok() {
                if let Some(mut order) = self.active_orders.remove(order_id) {
                    order.status = OrderStatus::Cancelled;
                    self.order_history.push(order);
                    cancelled.push(order_id.clone());
                }
            }
        }

        Ok(cancelled)
    }

    /// Get an active order by ID.
    pub fn get_order(&self, order_id: &OrderId) -> Option<&Order> {
        self.active_orders.get(order_id)
    }

    /// Get all active orders.
    pub fn active_orders(&self) -> &HashMap<OrderId, Order> {
        &self.active_orders
    }

    /// Get the completed/cancelled order history.
    pub fn order_history(&self) -> &[Order] {
        &self.order_history
    }

    /// Number of active orders.
    pub fn active_count(&self) -> usize {
        self.active_orders.len()
    }

    /// Whether the underlying executor is ready.
    pub fn is_ready(&self) -> bool {
        self.executor.is_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper::PaperExecutor;
    use brooks_china_market::rules::ChinaMarketRules;
    use brooks_core::market::{Direction, Exchange};
    use rust_decimal_macros::dec;

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    fn make_manager() -> OrderManager {
        let executor = PaperExecutor::new(Box::new(ChinaMarketRules), 0);
        OrderManager::new(Box::new(executor))
    }

    fn make_manager_with_price() -> OrderManager {
        let mut executor = PaperExecutor::new(Box::new(ChinaMarketRules), 0);
        executor.update_price(&security(), dec!(3.100));
        OrderManager::new(Box::new(executor))
    }

    #[tokio::test]
    async fn test_submit_order_adds_to_active() {
        let mut mgr = make_manager();
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order_id = order.id.clone();

        mgr.submit_order(order).await.unwrap();
        assert_eq!(mgr.active_count(), 1);
        assert!(mgr.get_order(&order_id).is_some());
    }

    #[tokio::test]
    async fn test_cancel_order_removes_from_active() {
        let mut mgr = make_manager();
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order_id = order.id.clone();

        mgr.submit_order(order).await.unwrap();
        mgr.cancel_order(&order_id, "test").await.unwrap();

        assert_eq!(mgr.active_count(), 0);
        assert!(mgr.get_order(&order_id).is_none());
        assert_eq!(mgr.order_history().len(), 1);
        assert_eq!(mgr.order_history()[0].status, OrderStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_returns_error() {
        let mut mgr = make_manager();
        let result = mgr.cancel_order(&OrderId::new(), "test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_modify_order_updates_price() {
        let mut mgr = make_manager();
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order_id = order.id.clone();

        mgr.submit_order(order).await.unwrap();
        mgr.modify_order(&order_id, Some(dec!(3.050)), None)
            .await
            .unwrap();

        let active = mgr.get_order(&order_id).unwrap();
        assert_eq!(active.price, Some(dec!(3.050)));
    }

    #[tokio::test]
    async fn test_market_order_fills_and_moves_to_history() {
        let mut mgr = make_manager_with_price();

        let order = Order::market(security(), Direction::Long, 100);
        mgr.submit_order(order).await.unwrap();

        let fills = mgr.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, dec!(3.100));

        // Should have moved from active to history
        assert_eq!(mgr.active_count(), 0);
        assert_eq!(mgr.order_history().len(), 1);
        assert_eq!(mgr.order_history()[0].status, OrderStatus::Filled);
    }

    #[tokio::test]
    async fn test_cancel_all_for_security() {
        let mut mgr = make_manager();
        let sec2 = SecurityId::etf("510300", Exchange::SH);

        let order1 = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order2 = Order::limit(security(), Direction::Long, 200, dec!(2.950));
        let order3 = Order::limit(sec2.clone(), Direction::Long, 100, dec!(4.500));

        mgr.submit_order(order1).await.unwrap();
        mgr.submit_order(order2).await.unwrap();
        mgr.submit_order(order3).await.unwrap();
        assert_eq!(mgr.active_count(), 3);

        let cancelled = mgr.cancel_all_for_security(&security()).await.unwrap();
        assert_eq!(cancelled.len(), 2);
        assert_eq!(mgr.active_count(), 1);
        // The remaining order is for sec2
    }

    #[tokio::test]
    async fn test_cancel_all_orders() {
        let mut mgr = make_manager();

        let order1 = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order2 = Order::limit(security(), Direction::Short, 200, dec!(3.200));

        mgr.submit_order(order1).await.unwrap();
        mgr.submit_order(order2).await.unwrap();
        assert_eq!(mgr.active_count(), 2);

        let cancelled = mgr.cancel_all_orders().await.unwrap();
        assert_eq!(cancelled.len(), 2);
        assert_eq!(mgr.active_count(), 0);
        assert_eq!(mgr.order_history().len(), 2);
    }

    #[tokio::test]
    async fn test_get_order_returns_none_for_unknown() {
        let mgr = make_manager();
        assert!(mgr.get_order(&OrderId::new()).is_none());
    }

    #[tokio::test]
    async fn test_is_ready_delegates_to_executor() {
        let mgr = make_manager();
        assert!(mgr.is_ready()); // PaperExecutor is always ready
    }

    #[tokio::test]
    async fn test_duplicate_submit_rejected() {
        let mut mgr = make_manager();
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));

        mgr.submit_order(order.clone()).await.unwrap();
        let result = mgr.submit_order(order).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate"));
    }
}
