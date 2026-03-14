use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use rust_decimal::Decimal;
use tracing::debug;

use brooks_china_market::rules::MarketRules;
use brooks_core::market::{Direction, SecurityId};
use brooks_core::order::{Order, OrderId, OrderStatus, OrderType};

use crate::error::OmsError;
use crate::executor::{FillEvent, OrderExecutor};

/// Paper trading executor that simulates order fills locally.
///
/// Market orders fill immediately at the current price (+ slippage).
/// Limit and stop orders are held until `update_price()` is called
/// with a price that triggers them.
pub struct PaperExecutor {
    /// Pending (unfilled) orders, keyed by OrderId.
    pending_orders: HashMap<OrderId, Order>,
    /// Fill events waiting to be polled.
    fill_queue: Vec<FillEvent>,
    /// Latest known price per security (for limit/stop trigger checks).
    current_prices: HashMap<SecurityId, Decimal>,
    /// Market rules for lot size validation.
    market_rules: Box<dyn MarketRules>,
    /// Slippage in basis points (e.g., 5 = 0.05%).
    slippage_bps: u32,
    /// Tracks open Long positions by their open timestamp for T+1 validation.
    long_positions: HashMap<SecurityId, DateTime<Utc>>,
}

impl PaperExecutor {
    /// Create a new paper executor.
    pub fn new(market_rules: Box<dyn MarketRules>, slippage_bps: u32) -> Self {
        Self {
            pending_orders: HashMap::new(),
            fill_queue: Vec::new(),
            current_prices: HashMap::new(),
            market_rules,
            slippage_bps,
            long_positions: HashMap::new(),
        }
    }

    /// Update the current market price for a security.
    ///
    /// This checks all pending limit/stop orders for the security
    /// and generates fill events for those that trigger.
    pub fn update_price(&mut self, security: &SecurityId, price: Decimal) {
        self.current_prices.insert(security.clone(), price);

        // Collect order IDs that should fill at this price
        let triggered: Vec<OrderId> = self
            .pending_orders
            .iter()
            .filter(|(_, order)| order.security == *security)
            .filter(|(_, order)| self.should_trigger(order, price))
            .map(|(id, _)| id.clone())
            .collect();

        for order_id in triggered {
            if let Some(mut order) = self.pending_orders.remove(&order_id) {
                let fill_price = self.apply_slippage(price, order.direction);
                order.status = OrderStatus::Filled;
                order.filled_price = Some(fill_price);
                order.filled_quantity = order.quantity;
                order.filled_at = Some(Utc::now());

                debug!(
                    order_id = ?order.id,
                    security = %order.security,
                    direction = ?order.direction,
                    price = %fill_price,
                    qty = order.quantity,
                    "Paper fill triggered"
                );

                self.fill_queue.push(FillEvent {
                    order_id: order.id.clone(),
                    security: order.security.clone(),
                    direction: order.direction,
                    fill_price,
                    fill_quantity: order.quantity,
                    timestamp: Utc::now(),
                    is_partial: false,
                });
            }
        }
    }

    /// Check if a limit/stop order should trigger at the given price.
    fn should_trigger(&self, order: &Order, price: Decimal) -> bool {
        match (order.order_type, order.direction) {
            // Limit buy: fills when price drops to or below the limit
            (OrderType::Limit, Direction::Long) => {
                order.price.is_some_and(|limit| price <= limit)
            }
            // Limit sell: fills when price rises to or above the limit
            (OrderType::Limit, Direction::Short) => {
                order.price.is_some_and(|limit| price >= limit)
            }
            // Stop buy: triggers when price rises to or above the stop
            (OrderType::Stop, Direction::Long) => {
                order.stop_price.is_some_and(|stop| price >= stop)
            }
            // Stop sell: triggers when price drops to or below the stop
            (OrderType::Stop, Direction::Short) => {
                order.stop_price.is_some_and(|stop| price <= stop)
            }
            // StopLimit: same trigger as Stop
            (OrderType::StopLimit, Direction::Long) => {
                order.stop_price.is_some_and(|stop| price >= stop)
            }
            (OrderType::StopLimit, Direction::Short) => {
                order.stop_price.is_some_and(|stop| price <= stop)
            }
            // Market orders should not be in pending (filled immediately on submit)
            (OrderType::Market, _) => false,
        }
    }

    /// Apply slippage to a fill price.
    /// Buys pay slightly more; sells receive slightly less.
    fn apply_slippage(&self, price: Decimal, direction: Direction) -> Decimal {
        if self.slippage_bps == 0 {
            return price;
        }
        let fraction =
            Decimal::from(self.slippage_bps) / Decimal::from(10_000);
        match direction {
            Direction::Long => price * (Decimal::ONE + fraction),
            Direction::Short => price * (Decimal::ONE - fraction),
        }
    }

    /// Validate an order against market rules.
    ///
    /// Also enforces T+1: if this is a sell order (Direction::Short) for a
    /// security with a tracked Long position opened today (same CST date),
    /// the order is rejected.
    fn validate_order(&self, order: &Order) -> Result<(), OmsError> {
        // T+1 validation for sell orders on Long positions
        if order.direction == Direction::Short {
            if let Some(opened_at) = self.long_positions.get(&order.security) {
                let cst = FixedOffset::east_opt(8 * 3600).unwrap();
                let open_date = opened_at.with_timezone(&cst).date_naive();
                let now_date = Utc::now().with_timezone(&cst).date_naive();
                if now_date <= open_date {
                    return Err(OmsError::MarketRulesViolation(format!(
                        "T+1 violation: cannot sell {} on the same day it was bought",
                        order.security
                    )));
                }
            }
        }

        let lot_size = self.market_rules.min_lot_size(&order.security);
        if lot_size > 0 && !order.quantity.is_multiple_of(lot_size) {
            return Err(OmsError::MarketRulesViolation(format!(
                "quantity {} is not a multiple of lot size {}",
                order.quantity, lot_size
            )));
        }
        if order.quantity == 0 {
            return Err(OmsError::OrderRejected(
                "quantity cannot be zero".to_string(),
            ));
        }
        Ok(())
    }

    /// Get the number of pending orders.
    pub fn pending_count(&self) -> usize {
        self.pending_orders.len()
    }

    /// Register a Long position for T+1 settlement tracking.
    ///
    /// Call this when a Long fill is received so that subsequent sell
    /// orders can be validated against T+1 rules.
    pub fn register_long_position(&mut self, security: SecurityId, opened_at: DateTime<Utc>) {
        self.long_positions.insert(security, opened_at);
    }

    /// Unregister a position after it has been closed.
    pub fn unregister_position(&mut self, security: &SecurityId) {
        self.long_positions.remove(security);
    }
}

#[async_trait]
impl OrderExecutor for PaperExecutor {
    async fn submit(&mut self, order: &Order) -> Result<(), OmsError> {
        // Reject duplicates
        if self.pending_orders.contains_key(&order.id) {
            return Err(OmsError::DuplicateOrder(format!("{:?}", order.id)));
        }

        // Validate against market rules
        self.validate_order(order)?;

        let mut order = order.clone();
        order.status = OrderStatus::Submitted;

        // Market orders fill immediately
        if order.order_type == OrderType::Market {
            let price = self
                .current_prices
                .get(&order.security)
                .copied()
                .unwrap_or_else(|| {
                    // Use the order's limit price as fallback, or zero
                    order.price.unwrap_or(Decimal::ZERO)
                });

            let fill_price = self.apply_slippage(price, order.direction);
            order.status = OrderStatus::Filled;
            order.filled_price = Some(fill_price);
            order.filled_quantity = order.quantity;
            order.filled_at = Some(Utc::now());

            debug!(
                order_id = ?order.id,
                security = %order.security,
                direction = ?order.direction,
                price = %fill_price,
                qty = order.quantity,
                "Paper market order filled immediately"
            );

            self.fill_queue.push(FillEvent {
                order_id: order.id.clone(),
                security: order.security.clone(),
                direction: order.direction,
                fill_price,
                fill_quantity: order.quantity,
                timestamp: Utc::now(),
                is_partial: false,
            });
            return Ok(());
        }

        // Non-market orders go to pending
        debug!(
            order_id = ?order.id,
            security = %order.security,
            order_type = ?order.order_type,
            "Paper order submitted as pending"
        );
        self.pending_orders.insert(order.id.clone(), order);
        Ok(())
    }

    async fn cancel(&mut self, order_id: &OrderId) -> Result<(), OmsError> {
        match self.pending_orders.remove(order_id) {
            Some(_) => {
                debug!(order_id = ?order_id, "Paper order cancelled");
                Ok(())
            }
            None => Err(OmsError::OrderNotFound(format!("{:?}", order_id))),
        }
    }

    async fn modify(
        &mut self,
        order_id: &OrderId,
        new_price: Option<Decimal>,
        new_stop: Option<Decimal>,
    ) -> Result<(), OmsError> {
        match self.pending_orders.get_mut(order_id) {
            Some(order) => {
                if let Some(price) = new_price {
                    order.price = Some(price);
                }
                if let Some(stop) = new_stop {
                    order.stop_price = Some(stop);
                }
                debug!(
                    order_id = ?order_id,
                    new_price = ?new_price,
                    new_stop = ?new_stop,
                    "Paper order modified"
                );
                Ok(())
            }
            None => Err(OmsError::OrderNotFound(format!("{:?}", order_id))),
        }
    }

    async fn poll_fills(&mut self) -> Result<Vec<FillEvent>, OmsError> {
        let fills = std::mem::take(&mut self.fill_queue);
        Ok(fills)
    }

    fn is_ready(&self) -> bool {
        true // Paper executor is always ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_china_market::rules::ChinaMarketRules;
    use brooks_core::market::Exchange;
    use rust_decimal_macros::dec;

    fn make_executor() -> PaperExecutor {
        PaperExecutor::new(Box::new(ChinaMarketRules), 5) // 5 bps slippage
    }

    fn make_executor_no_slippage() -> PaperExecutor {
        PaperExecutor::new(Box::new(ChinaMarketRules), 0)
    }

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    #[tokio::test]
    async fn test_market_order_fills_immediately() {
        let mut exec = make_executor_no_slippage();
        exec.current_prices.insert(security(), dec!(3.100));

        let order = Order::market(security(), Direction::Long, 100);
        exec.submit(&order).await.unwrap();

        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, dec!(3.100));
        assert_eq!(fills[0].fill_quantity, 100);
        assert!(!fills[0].is_partial);
        assert_eq!(exec.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_limit_buy_fills_when_price_drops() {
        let mut exec = make_executor_no_slippage();
        exec.current_prices.insert(security(), dec!(3.200));

        let order = Order::limit(security(), Direction::Long, 100, dec!(3.100));
        exec.submit(&order).await.unwrap();
        assert_eq!(exec.pending_count(), 1);

        // Price above limit — should not trigger
        exec.update_price(&security(), dec!(3.150));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 0);

        // Price at limit — should trigger
        exec.update_price(&security(), dec!(3.100));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, dec!(3.100));
        assert_eq!(exec.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_limit_buy_no_fill_above_limit() {
        let mut exec = make_executor_no_slippage();

        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        exec.submit(&order).await.unwrap();

        exec.update_price(&security(), dec!(3.100));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 0);
        assert_eq!(exec.pending_count(), 1);
    }

    #[tokio::test]
    async fn test_stop_buy_fills_when_price_rises() {
        let mut exec = make_executor_no_slippage();

        let order = Order::stop(security(), Direction::Long, 100, dec!(3.200));
        exec.submit(&order).await.unwrap();

        // Price below stop — no trigger
        exec.update_price(&security(), dec!(3.150));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 0);

        // Price at stop — trigger
        exec.update_price(&security(), dec!(3.200));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].direction, Direction::Long);
    }

    #[tokio::test]
    async fn test_stop_sell_fills_when_price_drops() {
        let mut exec = make_executor_no_slippage();

        let order = Order::stop(security(), Direction::Short, 100, dec!(3.000));
        exec.submit(&order).await.unwrap();

        // Price above stop — no trigger
        exec.update_price(&security(), dec!(3.050));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 0);

        // Price at stop — trigger
        exec.update_price(&security(), dec!(3.000));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].direction, Direction::Short);
    }

    #[tokio::test]
    async fn test_cancel_removes_pending() {
        let mut exec = make_executor_no_slippage();

        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order_id = order.id.clone();
        exec.submit(&order).await.unwrap();
        assert_eq!(exec.pending_count(), 1);

        exec.cancel(&order_id).await.unwrap();
        assert_eq!(exec.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_returns_error() {
        let mut exec = make_executor_no_slippage();
        let result = exec.cancel(&OrderId::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_modify_updates_price() {
        let mut exec = make_executor_no_slippage();

        // Limit buy at 3.000
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order_id = order.id.clone();
        exec.submit(&order).await.unwrap();

        // Modify limit down to 2.950 (stricter — needs a lower price to fill)
        exec.modify(&order_id, Some(dec!(2.950)), None)
            .await
            .unwrap();

        // Price at 3.000 — would have filled with original limit, but not with 2.950
        exec.update_price(&security(), dec!(3.000));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 0);

        // Price at 2.950 — now it fills
        exec.update_price(&security(), dec!(2.950));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
    }

    #[tokio::test]
    async fn test_modify_raises_limit() {
        let mut exec = make_executor_no_slippage();

        // Limit buy at 3.000
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order_id = order.id.clone();
        exec.submit(&order).await.unwrap();

        // Price at 3.010 — above original 3.000 limit, no fill
        exec.update_price(&security(), dec!(3.010));
        assert_eq!(exec.poll_fills().await.unwrap().len(), 0);

        // Modify limit up to 3.020. Now 3.010 <= 3.020 → should trigger on next update
        exec.modify(&order_id, Some(dec!(3.020)), None)
            .await
            .unwrap();

        exec.update_price(&security(), dec!(3.010));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
    }

    #[tokio::test]
    async fn test_slippage_applied_correctly() {
        let mut exec = make_executor(); // 5 bps
        exec.current_prices.insert(security(), dec!(3.100));

        // Buy: price goes up by 5 bps = 3.100 * 1.0005 = 3.10155
        let buy_order = Order::market(security(), Direction::Long, 100);
        exec.submit(&buy_order).await.unwrap();
        let fills = exec.poll_fills().await.unwrap();
        assert!(fills[0].fill_price > dec!(3.100));

        // Sell: price goes down by 5 bps = 3.100 * 0.9995 = 3.09845
        let sell_order = Order::market(security(), Direction::Short, 100);
        exec.submit(&sell_order).await.unwrap();
        let fills = exec.poll_fills().await.unwrap();
        assert!(fills[0].fill_price < dec!(3.100));
    }

    #[tokio::test]
    async fn test_lot_size_validation() {
        let mut exec = make_executor_no_slippage();
        exec.current_prices.insert(security(), dec!(3.100));

        // ETF lot size is 100. Quantity 150 is not a multiple of 100.
        let order = Order::market(security(), Direction::Long, 150);
        let result = exec.submit(&order).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a multiple of lot size"));
    }

    #[tokio::test]
    async fn test_zero_quantity_rejected() {
        let mut exec = make_executor_no_slippage();
        let mut order = Order::market(security(), Direction::Long, 100);
        order.quantity = 0;
        let result = exec.submit(&order).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("quantity cannot be zero"));
    }

    #[tokio::test]
    async fn test_poll_fills_drains_queue() {
        let mut exec = make_executor_no_slippage();
        exec.current_prices.insert(security(), dec!(3.100));

        let order = Order::market(security(), Direction::Long, 100);
        exec.submit(&order).await.unwrap();

        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);

        // Second poll should return empty
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 0);
    }

    #[tokio::test]
    async fn test_duplicate_order_rejected() {
        let mut exec = make_executor_no_slippage();

        let order = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        exec.submit(&order).await.unwrap();

        // Submit same order again
        let result = exec.submit(&order).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate order"));
    }

    #[tokio::test]
    async fn test_multiple_pending_orders() {
        let mut exec = make_executor_no_slippage();

        let order1 = Order::limit(security(), Direction::Long, 100, dec!(3.000));
        let order2 = Order::limit(security(), Direction::Long, 200, dec!(2.950));
        exec.submit(&order1).await.unwrap();
        exec.submit(&order2).await.unwrap();
        assert_eq!(exec.pending_count(), 2);

        // Price drops to 3.000 — only order1 triggers
        exec.update_price(&security(), dec!(3.000));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_quantity, 100);
        assert_eq!(exec.pending_count(), 1);

        // Price drops to 2.950 — order2 triggers
        exec.update_price(&security(), dec!(2.950));
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_quantity, 200);
        assert_eq!(exec.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_is_ready() {
        let exec = make_executor_no_slippage();
        assert!(exec.is_ready());
    }

    #[tokio::test]
    async fn test_t1_blocks_same_day_sell() {
        let mut exec = make_executor_no_slippage();
        exec.current_prices.insert(security(), dec!(3.200));

        // Register a Long position opened now
        exec.register_long_position(security(), Utc::now());

        // Try to submit a sell (Short direction) order — should be rejected
        let order = Order::market(security(), Direction::Short, 100);
        let result = exec.submit(&order).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("T+1 violation"));
    }

    #[tokio::test]
    async fn test_t1_allows_next_day_sell() {
        let mut exec = make_executor_no_slippage();
        exec.current_prices.insert(security(), dec!(3.200));

        // Register a Long position opened yesterday
        let yesterday = Utc::now() - chrono::Duration::days(1);
        exec.register_long_position(security(), yesterday);

        // Sell order should succeed
        let order = Order::market(security(), Direction::Short, 100);
        exec.submit(&order).await.unwrap();
        let fills = exec.poll_fills().await.unwrap();
        assert_eq!(fills.len(), 1);
    }

    #[tokio::test]
    async fn test_register_unregister_position() {
        let mut exec = make_executor_no_slippage();
        exec.register_long_position(security(), Utc::now());
        exec.unregister_position(&security());

        // After unregistering, sell orders should be allowed
        exec.current_prices.insert(security(), dec!(3.200));
        let order = Order::market(security(), Direction::Short, 100);
        exec.submit(&order).await.unwrap();
    }
}
