use brooks_core::event::MarketEvent;
use brooks_core::market::{Direction, SecurityId};
use brooks_core::order::{Order, OrderId};
use brooks_core::signal::Signal;
use rust_decimal::Decimal;

use crate::error::StrategyError;

/// An action the strategy wants the executor to perform.
///
/// The strategy never calls the order manager directly — it returns
/// structured actions and the caller (backtester or live runner) dispatches them.
#[derive(Debug, Clone)]
pub enum StrategyAction {
    /// Submit a new order (entry)
    SubmitOrder { order: Order, signal: Box<Signal> },
    /// Cancel a pending order
    CancelOrder { order_id: OrderId, reason: String },
    /// Modify an existing order's price
    ModifyOrder {
        order_id: OrderId,
        new_price: Option<Decimal>,
        new_stop: Option<Decimal>,
    },
    /// Update a position's stop loss
    UpdateStopLoss {
        security: SecurityId,
        new_stop: Decimal,
    },
    /// Close a position (market order to exit)
    ClosePosition { security: SecurityId, reason: String },
}

/// The core strategy trait. Identical interface for backtest and live.
///
/// Implementations receive market events and return zero or more actions.
/// The trait is object-safe to allow `Box<dyn Strategy>`.
pub trait Strategy: Send {
    /// Process a market event and return zero or more actions.
    fn on_event(&mut self, event: &MarketEvent) -> Result<Vec<StrategyAction>, StrategyError>;

    /// Notify the strategy that an order was filled (position now open).
    fn on_fill(
        &mut self,
        security: &SecurityId,
        direction: Direction,
        fill_price: Decimal,
        quantity: u64,
    ) -> Result<Vec<StrategyAction>, StrategyError>;

    /// Notify the strategy that a position was closed.
    fn on_position_closed(
        &mut self,
        security: &SecurityId,
        realized_pnl: Decimal,
    ) -> Result<(), StrategyError>;

    /// Number of currently open positions.
    fn open_position_count(&self) -> usize;

    /// Strategy name for logging.
    fn name(&self) -> &str;

    /// Reset strategy state (for new trading day or backtest re-run).
    fn reset(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_action_debug() {
        let action = StrategyAction::ClosePosition {
            security: SecurityId::etf("510050", brooks_core::market::Exchange::SH),
            reason: "End of day".to_string(),
        };
        let debug_str = format!("{action:?}");
        assert!(debug_str.contains("ClosePosition"));
        assert!(debug_str.contains("510050"));
        assert!(debug_str.contains("End of day"));
    }

    #[test]
    fn test_strategy_action_update_stop_loss_debug() {
        let action = StrategyAction::UpdateStopLoss {
            security: SecurityId::etf("510300", brooks_core::market::Exchange::SH),
            new_stop: Decimal::new(4500, 3), // 4.500
        };
        let debug_str = format!("{action:?}");
        assert!(debug_str.contains("UpdateStopLoss"));
        assert!(debug_str.contains("510300"));
    }

    /// Verify the Strategy trait is object-safe by constructing Box<dyn Strategy>.
    #[test]
    fn test_strategy_trait_is_object_safe() {
        struct MockStrategy;

        impl Strategy for MockStrategy {
            fn on_event(
                &mut self,
                _event: &MarketEvent,
            ) -> Result<Vec<StrategyAction>, StrategyError> {
                Ok(vec![])
            }
            fn on_fill(
                &mut self,
                _security: &SecurityId,
                _direction: Direction,
                _fill_price: Decimal,
                _quantity: u64,
            ) -> Result<Vec<StrategyAction>, StrategyError> {
                Ok(vec![])
            }
            fn on_position_closed(
                &mut self,
                _security: &SecurityId,
                _realized_pnl: Decimal,
            ) -> Result<(), StrategyError> {
                Ok(())
            }
            fn open_position_count(&self) -> usize {
                0
            }
            fn name(&self) -> &str {
                "mock"
            }
            fn reset(&mut self) {}
        }

        let strategy: Box<dyn Strategy> = Box::new(MockStrategy);
        assert_eq!(strategy.name(), "mock");
        assert_eq!(strategy.open_position_count(), 0);
    }
}
