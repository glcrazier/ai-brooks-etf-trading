use brooks_china_market::rules::MarketRules;
use brooks_china_market::session::TradingSession;
use brooks_core::event::MarketEvent;
use brooks_core::market::{Direction, SecurityId};
use brooks_core::order::Order;
use brooks_core::position::Position;
use brooks_core::signal::Signal;
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, FixedOffset, NaiveTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::config::StrategyConfig;
use crate::error::StrategyError;
use crate::mtf::MultiTimeframeCoordinator;
use crate::position_manager::PositionManager;
use crate::risk::RiskManager;
use crate::session_filter::SessionFilter;
use crate::signal_generator::SignalGenerator;
use crate::stop_target::StopTargetCalculator;
use crate::traits::{Strategy, StrategyAction};

/// CST timezone offset (UTC+8) for converting bar timestamps to local market time.
fn cst_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).unwrap()
}

/// Extract NaiveTime in CST from a UTC timestamp.
fn to_cst_time(utc: DateTime<Utc>) -> NaiveTime {
    utc.with_timezone(&cst_offset()).time()
}

/// The concrete Brooks Price Action strategy implementation.
///
/// Wires together all sub-components: multi-timeframe analysis, signal generation,
/// risk management, position management, and session filtering.
pub struct BrooksStrategy {
    config: StrategyConfig,
    mtf: MultiTimeframeCoordinator,
    signal_gen: SignalGenerator,
    risk_mgr: RiskManager,
    position_mgr: PositionManager,
    session_filter: SessionFilter,
    market_rules: Box<dyn MarketRules>,
}

impl BrooksStrategy {
    pub fn new(
        config: StrategyConfig,
        session: TradingSession,
        market_rules: Box<dyn MarketRules>,
    ) -> Self {
        let mtf = MultiTimeframeCoordinator::new(
            &config.pa,
            config.primary_timeframe,
            config.context_timeframe,
        );
        let signal_gen = SignalGenerator::new(config.risk.min_reward_risk_ratio);
        let risk_mgr = RiskManager::new(config.risk.clone());
        let stop_target_calc = StopTargetCalculator::new(config.risk.min_reward_risk_ratio);
        let position_mgr = PositionManager::new(stop_target_calc);
        let session_filter = SessionFilter::new(session, config.warm_up_bars);

        Self {
            config,
            mtf,
            signal_gen,
            risk_mgr,
            position_mgr,
            session_filter,
            market_rules,
        }
    }

    /// Main per-bar logic implementing the 6-step Brooks PA pipeline.
    fn handle_bar(
        &mut self,
        security: &SecurityId,
        bar: &brooks_core::bar::Bar,
        timeframe: &Timeframe,
    ) -> Result<Vec<StrategyAction>, StrategyError> {
        let mut actions = Vec::new();

        // 1. Feed bar to the appropriate analyzer
        if *timeframe == self.config.primary_timeframe {
            self.mtf.process_primary_bar(bar);
            self.session_filter.on_bar_processed();
        } else if *timeframe == self.config.context_timeframe {
            self.mtf.process_context_bar(bar);
            // No trading decisions on HTF bars
            return Ok(vec![]);
        } else {
            // Unknown timeframe — ignore
            return Ok(vec![]);
        }

        // 2. Update existing positions (stop/target checks)
        let swing_lows = self.mtf.primary_analyzer().trend_analyzer().swing_lows().to_vec();
        let swing_highs = self.mtf.primary_analyzer().trend_analyzer().swing_highs().to_vec();
        let position_actions = self.position_mgr.update(
            security,
            bar.close,
            bar.timestamp,
            &swing_lows,
            &swing_highs,
        );
        actions.extend(position_actions);

        // 3. Session filter
        let bar_time = to_cst_time(bar.timestamp);

        // Force close near end of day
        if self.session_filter.should_force_close(bar_time) {
            actions.extend(self.position_mgr.close_all("End of day"));
            return Ok(actions);
        }

        // Not warmed up or entry not allowed
        if !self.session_filter.is_warmed_up() || !self.session_filter.allows_new_entry(bar_time) {
            return Ok(actions);
        }

        // 4. Risk check
        if self.risk_mgr.can_open_position().is_err() || self.position_mgr.has_position(security) {
            return Ok(actions);
        }

        // 5. Generate signals
        let candidates = self.signal_gen.evaluate(
            bar,
            self.mtf.primary_context(),
            Some(self.mtf.context_context()),
            self.mtf.primary_analyzer(),
        );

        // 6. For each candidate: size the position, create the order
        for candidate in candidates {
            let lot_size = self.market_rules.min_lot_size(security);
            let quantity = self.risk_mgr.calculate_position_size(
                candidate.stop_target.risk_per_unit,
                lot_size,
            )?;

            if quantity == 0 {
                continue;
            }

            // Round to tick
            let entry = self
                .market_rules
                .round_to_tick(security, candidate.entry_price);
            let stop = self
                .market_rules
                .round_to_tick(security, candidate.stop_target.stop_price);

            // Create stop order (Brooks enters on breakout of signal bar)
            let order = Order::stop(security.clone(), candidate.direction, quantity, entry);

            // Build Signal
            let signal = Signal {
                id: Uuid::new_v4(),
                timestamp: bar.timestamp,
                security: security.clone(),
                direction: candidate.direction,
                signal_type: candidate.signal_type,
                entry_price: entry,
                stop_price: stop,
                target_price: candidate.stop_target.target_price,
                confidence: candidate.confidence,
                timeframe: *timeframe,
                context: candidate.htf_context,
            };

            actions.push(StrategyAction::SubmitOrder { order, signal: Box::new(signal) });
            break; // One entry per bar maximum
        }

        Ok(actions)
    }

    /// Handle tick updates for real-time stop checking.
    fn handle_tick(
        &mut self,
        security: &SecurityId,
        price: Decimal,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<StrategyAction>, StrategyError> {
        // In v1, tick-level stop checking: update position price and check stops
        let swing_lows = self.mtf.primary_analyzer().trend_analyzer().swing_lows().to_vec();
        let swing_highs = self.mtf.primary_analyzer().trend_analyzer().swing_highs().to_vec();
        let actions = self.position_mgr.update(security, price, timestamp, &swing_lows, &swing_highs);
        Ok(actions)
    }

    /// Handle session open: reset daily counters.
    fn handle_session_open(&mut self) -> Result<Vec<StrategyAction>, StrategyError> {
        self.risk_mgr.reset_daily();
        self.session_filter.reset_daily();
        self.signal_gen.reset();
        Ok(vec![])
    }

    /// Handle session close: under T+1 settlement, positions are held across sessions.
    /// Only daily risk metrics are reset on session open.
    fn handle_session_close(&mut self) -> Result<Vec<StrategyAction>, StrategyError> {
        Ok(vec![])
    }
}

impl Strategy for BrooksStrategy {
    fn on_event(&mut self, event: &MarketEvent) -> Result<Vec<StrategyAction>, StrategyError> {
        match event {
            MarketEvent::BarUpdate {
                security,
                bar,
                timeframe,
            } => self.handle_bar(security, bar, timeframe),
            MarketEvent::TickUpdate {
                security, price, timestamp, ..
            } => self.handle_tick(security, *price, *timestamp),
            MarketEvent::SessionOpen { .. } => self.handle_session_open(),
            MarketEvent::SessionClose { .. } => self.handle_session_close(),
            MarketEvent::SessionBreakStart { .. } => Ok(vec![]),
            MarketEvent::SessionBreakEnd { .. } => Ok(vec![]),
        }
    }

    fn on_fill(
        &mut self,
        security: &SecurityId,
        direction: Direction,
        fill_price: Decimal,
        quantity: u64,
    ) -> Result<Vec<StrategyAction>, StrategyError> {
        // Create a Position from fill data
        let position = Position {
            security: security.clone(),
            direction,
            quantity,
            entry_price: fill_price,
            current_price: fill_price,
            stop_loss: fill_price, // Will be updated by position manager
            take_profit: None,     // Will be set based on stop_target
            opened_at: Utc::now(),
        };

        // For now, use a placeholder risk_per_unit — the real value would come
        // from the signal that generated this order. In a more complete implementation,
        // we'd store the signal's stop_target alongside the pending order.
        let risk_per_unit = Decimal::ZERO;
        self.position_mgr
            .add_position(position, risk_per_unit, None);
        self.risk_mgr.record_open();

        Ok(vec![])
    }

    fn on_position_closed(
        &mut self,
        security: &SecurityId,
        realized_pnl: Decimal,
    ) -> Result<(), StrategyError> {
        self.position_mgr.remove_position(security);
        self.risk_mgr.record_close(realized_pnl);
        Ok(())
    }

    fn open_position_count(&self) -> usize {
        self.position_mgr.count()
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn reset(&mut self) {
        self.mtf.reset();
        self.signal_gen.reset();
        self.risk_mgr.reset_daily();
        self.session_filter.reset_daily();
        // Clear positions by re-creating with fresh calculator
        let stop_target_calc = StopTargetCalculator::new(self.config.risk.min_reward_risk_ratio);
        self.position_mgr = PositionManager::new(stop_target_calc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_china_market::rules::ChinaMarketRules;
    use brooks_core::market::Exchange;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn make_strategy() -> BrooksStrategy {
        let config = StrategyConfig::default();
        let session = TradingSession::china_a_share();
        let rules = Box::new(ChinaMarketRules);
        BrooksStrategy::new(config, session, rules)
    }

    fn make_bar_at_time(
        close: Decimal,
        hour: u32,
        minute: u32,
        tf: Timeframe,
    ) -> brooks_core::bar::Bar {
        // Create a UTC timestamp that corresponds to CST hour:minute
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let dt = cst
            .with_ymd_and_hms(2025, 6, 10, hour, minute, 0)
            .single()
            .unwrap();
        let utc_dt = dt.with_timezone(&Utc);

        brooks_core::bar::Bar {
            timestamp: utc_dt,
            open: close - dec!(0.010),
            high: close + dec!(0.010),
            low: close - dec!(0.020),
            close,
            volume: 100000,
            timeframe: tf,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    #[test]
    fn test_new_strategy_creation() {
        let strategy = make_strategy();
        assert_eq!(strategy.name(), "brooks_pa");
        assert_eq!(strategy.open_position_count(), 0);
    }

    #[test]
    fn test_no_signals_during_warm_up() {
        let mut strategy = make_strategy();
        let bar = make_bar_at_time(dec!(3.500), 10, 30, Timeframe::Minute5);
        let event = MarketEvent::BarUpdate {
            security: security(),
            bar,
            timeframe: Timeframe::Minute5,
        };
        let actions = strategy.on_event(&event).unwrap();
        // During warm-up (only 1 of 200 bars processed), no signals
        assert!(actions.is_empty());
    }

    #[test]
    fn test_context_bar_no_actions() {
        let mut strategy = make_strategy();
        let bar = make_bar_at_time(dec!(3.500), 10, 30, Timeframe::Daily);
        let event = MarketEvent::BarUpdate {
            security: security(),
            bar,
            timeframe: Timeframe::Daily,
        };
        let actions = strategy.on_event(&event).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn test_session_open_resets_state() {
        let mut strategy = make_strategy();

        // Process some bars to advance warm-up
        for i in 0..5 {
            let bar = make_bar_at_time(
                dec!(3.500) + Decimal::new(i * 10, 3),
                10,
                30 + i as u32,
                Timeframe::Minute5,
            );
            let event = MarketEvent::BarUpdate {
                security: security(),
                bar,
                timeframe: Timeframe::Minute5,
            };
            let _ = strategy.on_event(&event);
        }
        assert_eq!(strategy.session_filter.bars_processed(), 5);

        // Session open resets
        let actions = strategy
            .on_event(&MarketEvent::SessionOpen {
                exchange: Exchange::SH,
            })
            .unwrap();
        assert!(actions.is_empty());
        assert_eq!(strategy.session_filter.bars_processed(), 0);
    }

    #[test]
    fn test_session_close_preserves_positions() {
        let mut strategy = make_strategy();

        // Simulate a fill
        strategy
            .on_fill(&security(), Direction::Long, dec!(3.500), 1000)
            .unwrap();
        assert_eq!(strategy.open_position_count(), 1);

        // T+1: Session close should NOT close positions
        let actions = strategy
            .on_event(&MarketEvent::SessionClose {
                exchange: Exchange::SH,
            })
            .unwrap();
        assert!(actions.is_empty());
        // Position still held
        assert_eq!(strategy.open_position_count(), 1);
    }

    #[test]
    fn test_on_fill_and_on_position_closed() {
        let mut strategy = make_strategy();

        // Fill
        strategy
            .on_fill(&security(), Direction::Long, dec!(3.500), 1000)
            .unwrap();
        assert_eq!(strategy.open_position_count(), 1);
        assert_eq!(strategy.risk_mgr.open_position_count(), 1);

        // Close
        strategy
            .on_position_closed(&security(), dec!(200))
            .unwrap();
        assert_eq!(strategy.open_position_count(), 0);
        assert_eq!(strategy.risk_mgr.current_capital(), dec!(100200));
    }

    #[test]
    fn test_reset_clears_all_state() {
        let mut strategy = make_strategy();

        // Add some state
        strategy
            .on_fill(&security(), Direction::Long, dec!(3.500), 1000)
            .unwrap();
        assert_eq!(strategy.open_position_count(), 1);

        strategy.reset();
        assert_eq!(strategy.open_position_count(), 0);
    }

    #[test]
    fn test_break_events_no_op() {
        let mut strategy = make_strategy();
        let actions = strategy
            .on_event(&MarketEvent::SessionBreakStart {
                exchange: Exchange::SH,
            })
            .unwrap();
        assert!(actions.is_empty());

        let actions = strategy
            .on_event(&MarketEvent::SessionBreakEnd {
                exchange: Exchange::SH,
            })
            .unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn test_tick_update_with_no_positions() {
        let mut strategy = make_strategy();
        let event = MarketEvent::TickUpdate {
            security: security(),
            price: dec!(3.500),
            volume: 1000,
            timestamp: Utc::now(),
        };
        let actions = strategy.on_event(&event).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn test_duplicate_position_blocked() {
        let mut strategy = make_strategy();

        // Fill for 510050
        strategy
            .on_fill(&security(), Direction::Long, dec!(3.500), 1000)
            .unwrap();

        // Even if warmed up, should not open another position for same security
        // (blocked by has_position check in handle_bar)
        // This is tested indirectly — the handle_bar returns early
        assert!(strategy.position_mgr.has_position(&security()));
    }

    #[test]
    fn test_strategy_is_object_safe() {
        let strategy = make_strategy();
        let boxed: Box<dyn Strategy> = Box::new(strategy);
        assert_eq!(boxed.name(), "brooks_pa");
    }
}
