use std::collections::HashMap;

use brooks_core::bar::Bar;
use brooks_core::event::MarketEvent;
use brooks_core::market::SecurityId;
use brooks_core::signal::Signal;
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, Datelike, Utc};
use tracing::debug;

use brooks_core::order::OrderId;
use brooks_market_data::DataFeed;
use brooks_strategy::{Strategy, StrategyAction};

use crate::config::BacktestConfig;
use crate::error::BacktestError;
use crate::fill_model::{FillModel, NextBarOpenFill};
use crate::metrics::BacktestMetrics;
use crate::order_book::OrderBook;
use crate::portfolio::Portfolio;
use crate::trade_log::{TradeLog, TradeRecord};

/// Result of a completed backtest run.
pub struct BacktestResult {
    pub metrics: BacktestMetrics,
    pub trade_log: TradeLog,
    pub portfolio: Portfolio,
}

/// The main backtesting engine.
pub struct BacktestEngine {
    config: BacktestConfig,
    fill_model: Box<dyn FillModel>,
}

impl BacktestEngine {
    pub fn new(config: BacktestConfig) -> Self {
        let slippage = config.slippage_fraction();
        Self {
            config,
            fill_model: Box::new(NextBarOpenFill::new(slippage)),
        }
    }

    /// Create with a custom fill model (for testing).
    pub fn with_fill_model(config: BacktestConfig, fill_model: Box<dyn FillModel>) -> Self {
        Self { config, fill_model }
    }

    /// Run the backtest. Consumes bars from the feed, drives the strategy.
    pub fn run(
        &self,
        feed: &mut dyn DataFeed,
        strategy: &mut dyn Strategy,
        security: &SecurityId,
        timeframe: Timeframe,
    ) -> Result<BacktestResult, BacktestError> {
        if feed.is_empty() {
            return Err(BacktestError::NoData);
        }

        let mut portfolio = Portfolio::new(self.config.initial_capital);
        let mut order_book = OrderBook::new();
        let mut trade_log = TradeLog::new();
        let mut last_date: Option<u32> = None;
        // Track signals for trade logging
        let mut pending_signals: HashMap<OrderId, Signal> = HashMap::new();
        let mut position_signals: HashMap<SecurityId, (Signal, DateTime<Utc>)> = HashMap::new();

        while let Some(bar) = feed.next_bar() {
            let bar_date = bar.timestamp.date_naive().day();

            // Detect day boundary -> inject session events
            if let Some(prev_date) = last_date {
                if bar_date != prev_date {
                    // Session close for previous day
                    let close_event = MarketEvent::SessionClose {
                        exchange: security.exchange,
                    };
                    let close_actions = strategy.on_event(&close_event)?;
                    self.process_actions(
                        &close_actions,
                        &mut order_book,
                        &mut portfolio,
                        &mut trade_log,
                        strategy,
                        &mut pending_signals,
                        &mut position_signals,
                        &bar,
                    )?;

                    // T+1: Positions are held across day boundaries.
                    // Only cancel unfilled pending orders from previous day.
                    let stale_ids = order_book.order_ids();
                    for id in stale_ids {
                        let _ = order_book.cancel(&id);
                    }

                    // Session open for new day
                    let open_event = MarketEvent::SessionOpen {
                        exchange: security.exchange,
                    };
                    let open_actions = strategy.on_event(&open_event)?;
                    self.process_actions(
                        &open_actions,
                        &mut order_book,
                        &mut portfolio,
                        &mut trade_log,
                        strategy,
                        &mut pending_signals,
                        &mut position_signals,
                        &bar,
                    )?;
                }
            } else {
                // First bar: inject session open
                let open_event = MarketEvent::SessionOpen {
                    exchange: security.exchange,
                };
                let _ = strategy.on_event(&open_event)?;
            }
            last_date = Some(bar_date);

            // Try to fill pending orders against this bar
            self.try_fill_orders(
                &bar,
                &mut order_book,
                &mut portfolio,
                &mut trade_log,
                strategy,
                &mut pending_signals,
                &mut position_signals,
            )?;

            // Update position mark-to-market
            portfolio.update_price(security, bar.close);

            // Forward bar to strategy
            let event = MarketEvent::BarUpdate {
                security: security.clone(),
                bar: bar.clone(),
                timeframe,
            };
            let actions = strategy.on_event(&event)?;
            self.process_actions(
                &actions,
                &mut order_book,
                &mut portfolio,
                &mut trade_log,
                strategy,
                &mut pending_signals,
                &mut position_signals,
                &bar,
            )?;

            // Record equity snapshot
            portfolio.record_equity_snapshot(bar.timestamp);
        }

        // End of data: force close remaining positions
        let last_time = portfolio
            .equity_curve()
            .last()
            .map(|p| p.timestamp)
            .unwrap_or_else(Utc::now);
        self.force_close_all(
            &mut portfolio,
            &mut trade_log,
            strategy,
            &mut position_signals,
            last_time,
        )?;

        let metrics = BacktestMetrics::calculate(
            self.config.initial_capital,
            portfolio.equity_curve(),
            &trade_log,
        );

        Ok(BacktestResult {
            metrics,
            trade_log,
            portfolio,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn try_fill_orders(
        &self,
        bar: &Bar,
        order_book: &mut OrderBook,
        portfolio: &mut Portfolio,
        trade_log: &mut TradeLog,
        strategy: &mut dyn Strategy,
        pending_signals: &mut HashMap<OrderId, Signal>,
        position_signals: &mut HashMap<SecurityId, (Signal, DateTime<Utc>)>,
    ) -> Result<(), BacktestError> {
        let order_ids = order_book.order_ids();
        for order_id in order_ids {
            // Check if order would fill on this bar
            let should_fill = {
                let pending = match order_book.iter().find(|po| po.order.id == order_id) {
                    Some(po) => po,
                    None => continue,
                };
                self.fill_model.try_fill(&pending.order, bar)
            };

            if let Some(fill) = should_fill {
                let pending = order_book.remove(&order_id).unwrap();
                let order = pending.order;
                let signal = pending.signal;

                // Open position in portfolio
                portfolio.open_position(
                    order.security.clone(),
                    order.direction,
                    order.quantity,
                    fill.fill_price,
                    signal.stop_price,
                    signal.target_price,
                    fill.fill_time,
                )?;

                // Store signal for trade log when position closes
                position_signals.insert(
                    order.security.clone(),
                    (signal.clone(), fill.fill_time),
                );

                // Notify strategy of fill
                let fill_actions = strategy.on_fill(
                    &order.security,
                    order.direction,
                    fill.fill_price,
                    order.quantity,
                )?;

                // Process any actions from on_fill
                for action in &fill_actions {
                    self.process_single_action(
                        action,
                        order_book,
                        portfolio,
                        trade_log,
                        strategy,
                        pending_signals,
                        position_signals,
                        bar,
                    )?;
                }

                debug!(
                    security = %order.security,
                    price = %fill.fill_price,
                    qty = order.quantity,
                    "Order filled"
                );
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn process_actions(
        &self,
        actions: &[StrategyAction],
        order_book: &mut OrderBook,
        portfolio: &mut Portfolio,
        trade_log: &mut TradeLog,
        strategy: &mut dyn Strategy,
        pending_signals: &mut HashMap<OrderId, Signal>,
        position_signals: &mut HashMap<SecurityId, (Signal, DateTime<Utc>)>,
        bar: &Bar,
    ) -> Result<(), BacktestError> {
        for action in actions {
            self.process_single_action(
                action,
                order_book,
                portfolio,
                trade_log,
                strategy,
                pending_signals,
                position_signals,
                bar,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn process_single_action(
        &self,
        action: &StrategyAction,
        order_book: &mut OrderBook,
        portfolio: &mut Portfolio,
        trade_log: &mut TradeLog,
        strategy: &mut dyn Strategy,
        _pending_signals: &mut HashMap<OrderId, Signal>,
        position_signals: &mut HashMap<SecurityId, (Signal, DateTime<Utc>)>,
        bar: &Bar,
    ) -> Result<(), BacktestError> {
        match action {
            StrategyAction::SubmitOrder { order, signal } => {
                order_book.add(order.clone(), *signal.clone());
            }
            StrategyAction::CancelOrder { order_id, reason } => {
                let _ = order_book.cancel(order_id);
                debug!(order_id = ?order_id, reason = reason, "Order cancelled");
            }
            StrategyAction::ModifyOrder {
                order_id,
                new_price,
                new_stop,
            } => {
                let _ = order_book.modify(order_id, *new_price, *new_stop);
            }
            StrategyAction::UpdateStopLoss { security, new_stop } => {
                let _ = portfolio.update_stop_loss(security, *new_stop);
            }
            StrategyAction::ClosePosition { security, reason } => {
                if portfolio.has_position(security) {
                    let exit_price = bar.close;
                    let pnl = portfolio.close_position(security, exit_price, bar.timestamp)?;

                    // Record trade
                    if let Some((signal, entry_time)) = position_signals.remove(security) {
                        trade_log.record(TradeRecord {
                            security: security.clone(),
                            direction: signal.direction,
                            quantity: 0, // Not tracked at this level
                            entry_price: signal.entry_price,
                            exit_price,
                            entry_time,
                            exit_time: bar.timestamp,
                            realized_pnl: pnl,
                            signal_type: signal.signal_type,
                            exit_reason: reason.clone(),
                        });
                    }

                    // Cancel any pending orders for this security
                    order_book.cancel_all_for_security(security);

                    // Notify strategy
                    strategy.on_position_closed(security, pnl)?;
                }
            }
        }
        Ok(())
    }

    fn force_close_all(
        &self,
        portfolio: &mut Portfolio,
        trade_log: &mut TradeLog,
        strategy: &mut dyn Strategy,
        position_signals: &mut HashMap<SecurityId, (Signal, DateTime<Utc>)>,
        close_time: DateTime<Utc>,
    ) -> Result<(), BacktestError> {
        let securities = portfolio.position_securities();
        for security in securities {
            if let Some(pos) = portfolio.get_position(&security) {
                let exit_price = pos.current_price;
                let pnl = portfolio.close_position(&security, exit_price, close_time)?;

                if let Some((signal, entry_time)) = position_signals.remove(&security) {
                    trade_log.record(TradeRecord {
                        security: security.clone(),
                        direction: signal.direction,
                        quantity: 0,
                        entry_price: signal.entry_price,
                        exit_price,
                        entry_time,
                        exit_time: close_time,
                        realized_pnl: pnl,
                        signal_type: signal.signal_type,
                        exit_reason: "Force close (end of data)".to_string(),
                    });
                }

                strategy.on_position_closed(&security, pnl)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Direction, Exchange};
    use brooks_market_data::VecDataFeed;
    use brooks_strategy::StrategyError;
    use chrono::{FixedOffset, TimeZone};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    /// Make a bar at a specific CST hour:minute on a given day offset.
    fn make_bar_at(close: Decimal, day: i64, hour: u32, min: u32) -> Bar {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let dt = cst
            .with_ymd_and_hms(2024, 6, 10 + day as u32, hour, min, 0)
            .single()
            .unwrap();
        Bar {
            timestamp: dt.with_timezone(&Utc),
            open: close - dec!(0.01),
            high: close + dec!(0.02),
            low: close - dec!(0.02),
            close,
            volume: 10000,
            timeframe: Timeframe::Minute5,
            security: security(),
        }
    }

    /// A no-op strategy that never generates signals.
    struct NoOpStrategy;
    impl Strategy for NoOpStrategy {
        fn on_event(
            &mut self,
            _: &MarketEvent,
        ) -> Result<Vec<StrategyAction>, StrategyError> {
            Ok(vec![])
        }
        fn on_fill(
            &mut self,
            _: &SecurityId,
            _: Direction,
            _: Decimal,
            _: u64,
        ) -> Result<Vec<StrategyAction>, StrategyError> {
            Ok(vec![])
        }
        fn on_position_closed(
            &mut self,
            _: &SecurityId,
            _: Decimal,
        ) -> Result<(), StrategyError> {
            Ok(())
        }
        fn open_position_count(&self) -> usize {
            0
        }
        fn name(&self) -> &str {
            "noop"
        }
        fn reset(&mut self) {}
    }

    #[test]
    fn test_empty_feed_returns_error() {
        let engine = BacktestEngine::new(BacktestConfig::default());
        let mut feed = VecDataFeed::new(vec![]);
        let mut strategy = NoOpStrategy;
        let result = engine.run(&mut feed, &mut strategy, &security(), Timeframe::Minute5);
        assert!(matches!(result, Err(BacktestError::NoData)));
    }

    #[test]
    fn test_noop_strategy_preserves_capital() {
        let config = BacktestConfig {
            initial_capital: dec!(100000),
            ..Default::default()
        };
        let engine = BacktestEngine::new(config);
        let bars = (0..10)
            .map(|i| make_bar_at(dec!(3.100) + Decimal::new(i, 3), 0, 10, i as u32))
            .collect();
        let mut feed = VecDataFeed::new(bars);
        let mut strategy = NoOpStrategy;
        let result = engine
            .run(&mut feed, &mut strategy, &security(), Timeframe::Minute5)
            .unwrap();
        assert_eq!(result.metrics.total_pnl, Decimal::ZERO);
        assert_eq!(result.metrics.total_trades, 0);
        assert_eq!(result.portfolio.cash(), dec!(100000));
    }

    #[test]
    fn test_equity_curve_has_one_point_per_bar() {
        let config = BacktestConfig {
            initial_capital: dec!(100000),
            ..Default::default()
        };
        let engine = BacktestEngine::new(config);
        let bars: Vec<Bar> = (0..5)
            .map(|i| make_bar_at(dec!(3.100), 0, 10, i as u32))
            .collect();
        let n = bars.len();
        let mut feed = VecDataFeed::new(bars);
        let mut strategy = NoOpStrategy;
        let result = engine
            .run(&mut feed, &mut strategy, &security(), Timeframe::Minute5)
            .unwrap();
        assert_eq!(result.portfolio.equity_curve().len(), n);
    }
}
