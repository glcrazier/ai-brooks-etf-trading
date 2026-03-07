use brooks_core::bar::Bar;
use brooks_core::market::Direction;
use brooks_core::timeframe::Timeframe;
use brooks_pa_engine::analyzer::{PAConfig, PriceActionAnalyzer};
use brooks_pa_engine::context::MarketContext;
use rust_decimal::Decimal;

use crate::config::PAStrategyConfig;

/// Coordinates primary (e.g. 5min) and context (e.g. daily) timeframe analyzers.
///
/// Each timeframe gets its own `PriceActionAnalyzer` instance.
/// The strategy feeds bars to the appropriate analyzer and queries both
/// for multi-timeframe alignment decisions.
pub struct MultiTimeframeCoordinator {
    primary_analyzer: PriceActionAnalyzer,
    context_analyzer: PriceActionAnalyzer,
    primary_timeframe: Timeframe,
    context_timeframe: Timeframe,
    primary_context: MarketContext,
    context_context: MarketContext,
}

impl MultiTimeframeCoordinator {
    pub fn new(
        pa_config: &PAStrategyConfig,
        primary_timeframe: Timeframe,
        context_timeframe: Timeframe,
    ) -> Self {
        let pa = pa_config.to_pa_config();
        Self {
            primary_analyzer: PriceActionAnalyzer::new(pa.clone()),
            context_analyzer: PriceActionAnalyzer::new(pa),
            primary_timeframe,
            context_timeframe,
            primary_context: MarketContext::default(),
            context_context: MarketContext::default(),
        }
    }

    /// Process a bar on the primary timeframe. Returns updated context.
    pub fn process_primary_bar(&mut self, bar: &Bar) -> &MarketContext {
        self.primary_context = self.primary_analyzer.process_bar(bar);
        &self.primary_context
    }

    /// Process a bar on the context (higher) timeframe. Returns updated context.
    pub fn process_context_bar(&mut self, bar: &Bar) -> &MarketContext {
        self.context_context = self.context_analyzer.process_bar(bar);
        &self.context_context
    }

    /// Get the primary timeframe context.
    pub fn primary_context(&self) -> &MarketContext {
        &self.primary_context
    }

    /// Get the higher timeframe context.
    pub fn context_context(&self) -> &MarketContext {
        &self.context_context
    }

    /// Whether the primary and context timeframes have aligned trends.
    /// Both must be bull or both bear; trading range counts as unaligned.
    pub fn is_htf_aligned(&self) -> bool {
        let primary_bull = self.primary_context.trend.is_bull();
        let primary_bear = self.primary_context.trend.is_bear();
        let context_bull = self.context_context.trend.is_bull();
        let context_bear = self.context_context.trend.is_bear();

        (primary_bull && context_bull) || (primary_bear && context_bear)
    }

    /// Get the primary analyzer (for signal/entry bar detection).
    pub fn primary_analyzer(&self) -> &PriceActionAnalyzer {
        &self.primary_analyzer
    }

    /// Get the higher-timeframe trend direction, or None if in trading range.
    pub fn htf_trend_direction(&self) -> Option<Direction> {
        if self.context_context.trend.is_bull() {
            Some(Direction::Long)
        } else if self.context_context.trend.is_bear() {
            Some(Direction::Short)
        } else {
            None
        }
    }

    /// Get key S/R levels from the higher timeframe.
    pub fn htf_key_levels(&self) -> Vec<Decimal> {
        let mut levels = Vec::new();
        if let Some(sr) = &self.context_context.nearest_support {
            levels.push(sr.price);
        }
        if let Some(sr) = &self.context_context.nearest_resistance {
            levels.push(sr.price);
        }
        levels
    }

    pub fn primary_timeframe(&self) -> Timeframe {
        self.primary_timeframe
    }

    pub fn context_timeframe(&self) -> Timeframe {
        self.context_timeframe
    }

    /// Reset both analyzers for a new trading day or backtest re-run.
    pub fn reset(&mut self) {
        let pa_config = PAConfig::default();
        self.primary_analyzer = PriceActionAnalyzer::new(pa_config.clone());
        self.context_analyzer = PriceActionAnalyzer::new(pa_config);
        self.primary_context = MarketContext::default();
        self.context_context = MarketContext::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Exchange, SecurityId};
    use rust_decimal_macros::dec;

    fn make_bar(close: Decimal, timeframe: Timeframe) -> Bar {
        use chrono::Utc;
        Bar {
            timestamp: Utc::now(),
            open: close - dec!(0.010),
            high: close + dec!(0.010),
            low: close - dec!(0.020),
            close,
            volume: 10000,
            timeframe,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    fn make_coordinator() -> MultiTimeframeCoordinator {
        MultiTimeframeCoordinator::new(
            &PAStrategyConfig::default(),
            Timeframe::Minute5,
            Timeframe::Daily,
        )
    }

    #[test]
    fn test_process_primary_bar_updates_primary_context() {
        let mut coord = make_coordinator();
        let bar = make_bar(dec!(3.500), Timeframe::Minute5);
        coord.process_primary_bar(&bar);
        assert_eq!(coord.primary_context().current_price, dec!(3.500));
        // Context timeframe should remain at default
        assert_eq!(coord.context_context().current_price, Decimal::ZERO);
    }

    #[test]
    fn test_process_context_bar_updates_context_only() {
        let mut coord = make_coordinator();
        let bar = make_bar(dec!(3.600), Timeframe::Daily);
        coord.process_context_bar(&bar);
        assert_eq!(coord.context_context().current_price, dec!(3.600));
        assert_eq!(coord.primary_context().current_price, Decimal::ZERO);
    }

    #[test]
    fn test_htf_aligned_both_default_trading_range() {
        let coord = make_coordinator();
        // Both start as TradingRange -> not aligned
        assert!(!coord.is_htf_aligned());
    }

    #[test]
    fn test_htf_trend_direction_default() {
        let coord = make_coordinator();
        // Default is TradingRange -> None
        assert_eq!(coord.htf_trend_direction(), None);
    }

    #[test]
    fn test_htf_key_levels_empty_initially() {
        let coord = make_coordinator();
        assert!(coord.htf_key_levels().is_empty());
    }

    #[test]
    fn test_timeframe_getters() {
        let coord = make_coordinator();
        assert_eq!(coord.primary_timeframe(), Timeframe::Minute5);
        assert_eq!(coord.context_timeframe(), Timeframe::Daily);
    }

    #[test]
    fn test_reset_clears_context() {
        let mut coord = make_coordinator();
        let bar = make_bar(dec!(3.500), Timeframe::Minute5);
        coord.process_primary_bar(&bar);
        assert_eq!(coord.primary_context().current_price, dec!(3.500));

        coord.reset();
        assert_eq!(coord.primary_context().current_price, Decimal::ZERO);
        assert_eq!(coord.context_context().current_price, Decimal::ZERO);
    }

    #[test]
    fn test_multiple_primary_bars() {
        let mut coord = make_coordinator();
        for i in 0..5 {
            let price = dec!(3.500) + Decimal::new(i * 10, 3);
            let bar = make_bar(price, Timeframe::Minute5);
            coord.process_primary_bar(&bar);
        }
        // After 5 bars, price should be 3.500 + 0.040 = 3.540
        assert_eq!(coord.primary_context().current_price, dec!(3.540));
        assert_eq!(coord.primary_context().bar_count, 5);
    }
}
