use brooks_core::bar::Bar;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::bar_analysis::{BarAnalysisConfig, BarAnalyzer};
use crate::bar_counting::BarCounter;
use crate::breakout::{BreakoutConfig, BreakoutDetector};
use crate::channel::{ChannelConfig, ChannelDetector};
use crate::context::MarketContext;
use crate::entry_bar::EntryBarDetector;
use crate::pattern::{PatternConfig, PatternDetector};
use crate::signal_bar::{SignalBarConfig, SignalBarDetector};
use crate::support_resistance::SRDetector;
use crate::trading_range::{TradingRangeConfig, TradingRangeDetector};
use crate::trend::{TrendAnalyzer, TrendConfig};

/// Configuration for the Price Action Analyzer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PAConfig {
    /// EMA period for trend reference
    pub ema_period: usize,
    /// Lookback for swing point detection
    pub swing_lookback: usize,
    /// Body/range ratio threshold for doji classification
    pub doji_threshold: Decimal,
    /// Price cluster tolerance for S/R level grouping
    pub sr_cluster_tolerance: Decimal,
    /// Maximum number of recent bar classifications to retain
    pub max_recent_classifications: usize,
    /// Climax threshold (consecutive bars in one direction)
    pub climax_threshold: u32,
    /// Minimum bars for trading range detection
    pub min_range_bars: u32,
}

impl Default for PAConfig {
    fn default() -> Self {
        Self {
            ema_period: 20,
            swing_lookback: 5,
            doji_threshold: dec!(0.20),
            sr_cluster_tolerance: dec!(0.005),
            max_recent_classifications: 50,
            climax_threshold: 8,
            min_range_bars: 20,
        }
    }
}

/// The main Price Action analysis engine.
/// Orchestrates all sub-analyzers and produces a unified MarketContext.
pub struct PriceActionAnalyzer {
    bar_analyzer: BarAnalyzer,
    bar_counter: BarCounter,
    trend_analyzer: TrendAnalyzer,
    sr_detector: SRDetector,
    breakout_detector: BreakoutDetector,
    trading_range_detector: TradingRangeDetector,
    channel_detector: ChannelDetector,
    pattern_detector: PatternDetector,
    signal_bar_detector: SignalBarDetector,
    entry_bar_detector: EntryBarDetector,
    config: PAConfig,
    context: MarketContext,
    prev_bar: Option<Bar>,
}

impl PriceActionAnalyzer {
    pub fn new(config: PAConfig) -> Self {
        let bar_config = BarAnalysisConfig {
            doji_threshold: config.doji_threshold,
            ..Default::default()
        };

        let trend_config = TrendConfig {
            swing_lookback: config.swing_lookback,
            ema_period: config.ema_period,
            ..Default::default()
        };

        let trading_range_config = TradingRangeConfig {
            min_bars: config.min_range_bars,
            ..Default::default()
        };

        Self {
            bar_analyzer: BarAnalyzer::new(bar_config),
            bar_counter: BarCounter::new(config.climax_threshold),
            trend_analyzer: TrendAnalyzer::new(trend_config),
            sr_detector: SRDetector::new(config.sr_cluster_tolerance, 50),
            breakout_detector: BreakoutDetector::new(BreakoutConfig::default()),
            trading_range_detector: TradingRangeDetector::new(trading_range_config),
            channel_detector: ChannelDetector::new(ChannelConfig::default()),
            pattern_detector: PatternDetector::new(PatternConfig::default()),
            signal_bar_detector: SignalBarDetector::new(SignalBarConfig::default()),
            entry_bar_detector: EntryBarDetector::new(),
            config,
            context: MarketContext::default(),
            prev_bar: None,
        }
    }

    /// Process a new bar and return the updated market context
    pub fn process_bar(&mut self, bar: &Bar) -> MarketContext {
        // 1. Classify the bar
        let mut classification = self.bar_analyzer.classify(bar, self.prev_bar.as_ref());

        // 2. Update bar counter
        self.bar_counter.update(bar, &classification);

        // 3. Update trend analysis (also detects swing points)
        let trend_state = self.trend_analyzer.update(bar);

        // 4. Update support/resistance from swing points
        let new_swings = self.collect_recent_swings();
        self.sr_detector.update(bar, &new_swings);

        // 5. Update breakout detection
        self.breakout_detector
            .update(bar, self.sr_detector.all_levels(), trend_state);

        // 6. Update trading range detection
        self.trading_range_detector.update(bar, trend_state);

        // 7. Update channel detection
        self.channel_detector.update(
            self.trend_analyzer.swing_highs(),
            self.trend_analyzer.swing_lows(),
        );

        // 8. Update pattern detection
        self.pattern_detector.update(
            self.trend_analyzer.swing_highs(),
            self.trend_analyzer.swing_lows(),
        );

        // 9. Build the market context (needed for signal bar detection)
        self.build_context(bar);

        // 10. Check signal bar status using the context
        classification.is_signal_bar = self
            .signal_bar_detector
            .is_bull_signal_bar(bar, &self.context)
            || self
                .signal_bar_detector
                .is_bear_signal_bar(bar, &self.context);

        // 11. Check entry bar (against previous bar as signal bar)
        if let Some(prev) = &self.prev_bar {
            let prev_was_signal = self
                .context
                .bar_classifications
                .back()
                .is_some_and(|c| c.is_signal_bar);

            if prev_was_signal {
                let bull_entry = self.entry_bar_detector.check_bull_entry(
                    bar,
                    prev,
                    crate::signal_bar::SignalQuality::Moderate,
                );
                let bear_entry = self.entry_bar_detector.check_bear_entry(
                    bar,
                    prev,
                    crate::signal_bar::SignalQuality::Moderate,
                );
                classification.is_entry_bar = bull_entry.is_some() || bear_entry.is_some();
            }
        }

        // 12. Store classification
        if self.context.bar_classifications.len() >= self.config.max_recent_classifications {
            self.context.bar_classifications.pop_front();
        }
        self.context.bar_classifications.push_back(classification);

        // Store previous bar for next iteration
        self.prev_bar = Some(bar.clone());

        self.context.clone()
    }

    /// Get the current market context without processing a new bar
    pub fn current_context(&self) -> &MarketContext {
        &self.context
    }

    /// Get the signal bar detector for external quality assessments
    pub fn signal_bar_detector(&self) -> &SignalBarDetector {
        &self.signal_bar_detector
    }

    /// Get the entry bar detector for external entry checks
    pub fn entry_bar_detector(&self) -> &EntryBarDetector {
        &self.entry_bar_detector
    }

    /// Get the trend analyzer for external queries
    pub fn trend_analyzer(&self) -> &TrendAnalyzer {
        &self.trend_analyzer
    }

    fn build_context(&mut self, bar: &Bar) {
        self.context.trend = self.trend_analyzer.current_trend();
        self.context.trend_strength = self.trend_analyzer.trend_strength();
        self.context.in_trading_range = self.trading_range_detector.is_in_trading_range();
        self.context.current_range = self.trading_range_detector.current_range().cloned();
        self.context.active_channel = self.channel_detector.current_channel().cloned();
        self.context.nearest_support = self.sr_detector.nearest_support(bar.close).cloned();
        self.context.nearest_resistance = self.sr_detector.nearest_resistance(bar.close).cloned();
        self.context.active_breakouts = self.breakout_detector.active_breakouts().to_vec();
        self.context.recent_patterns = self.pattern_detector.patterns().to_vec();
        self.context.consecutive_bull_bars = self.bar_counter.consecutive_bull_count();
        self.context.consecutive_bear_bars = self.bar_counter.consecutive_bear_count();
        self.context.is_climax = self.bar_counter.is_climax();
        self.context.ema = self.trend_analyzer.ema();
        self.context.current_price = bar.close;
        self.context.bar_count = self.bar_counter.total_bars();
    }

    fn collect_recent_swings(&self) -> Vec<crate::trend::SwingPoint> {
        // Return swing points from the trend analyzer that are new
        // (simple approach: return last few swing points)
        let mut swings = Vec::new();
        if let Some(sh) = self.trend_analyzer.last_swing_high() {
            swings.push(sh.clone());
        }
        if let Some(sl) = self.trend_analyzer.last_swing_low() {
            swings.push(sl.clone());
        }
        swings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Exchange, SecurityId};
    use brooks_core::timeframe::Timeframe;
    use chrono::Utc;

    fn make_bar(open: Decimal, high: Decimal, low: Decimal, close: Decimal) -> Bar {
        Bar {
            timestamp: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    #[test]
    fn test_initial_context() {
        let analyzer = PriceActionAnalyzer::new(PAConfig::default());
        let ctx = analyzer.current_context();
        assert_eq!(ctx.bar_count, 0);
        assert_eq!(ctx.trend, crate::trend::TrendState::TradingRange);
    }

    #[test]
    fn test_process_single_bar() {
        let mut analyzer = PriceActionAnalyzer::new(PAConfig::default());
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(3.08));
        let ctx = analyzer.process_bar(&bar);

        assert_eq!(ctx.bar_count, 1);
        assert_eq!(ctx.current_price, dec!(3.08));
        assert!(ctx.ema.is_some());
        assert!(!ctx.bar_classifications.is_empty());
    }

    #[test]
    fn test_process_multiple_bars() {
        let mut analyzer = PriceActionAnalyzer::new(PAConfig::default());

        let bars = vec![
            make_bar(dec!(3.00), dec!(3.05), dec!(2.98), dec!(3.04)),
            make_bar(dec!(3.04), dec!(3.08), dec!(3.02), dec!(3.07)),
            make_bar(dec!(3.07), dec!(3.12), dec!(3.05), dec!(3.10)),
            make_bar(dec!(3.10), dec!(3.15), dec!(3.08), dec!(3.13)),
            make_bar(dec!(3.13), dec!(3.18), dec!(3.11), dec!(3.16)),
        ];

        let mut last_ctx = MarketContext::default();
        for bar in &bars {
            last_ctx = analyzer.process_bar(bar);
        }

        assert_eq!(last_ctx.bar_count, 5);
        assert_eq!(last_ctx.current_price, dec!(3.16));
        assert_eq!(last_ctx.bar_classifications.len(), 5);
        assert!(last_ctx.consecutive_bull_bars > 0);
    }

    #[test]
    fn test_trend_develops_over_bars() {
        let config = PAConfig {
            ema_period: 5,
            swing_lookback: 2,
            ..Default::default()
        };
        let mut analyzer = PriceActionAnalyzer::new(config);

        // Generate a clear uptrend
        let mut price = dec!(3.00);
        for i in 0..20 {
            let pullback = i % 5 == 3 || i % 5 == 4;
            let (open, close, high, low) = if pullback {
                (
                    price,
                    price - dec!(0.02),
                    price + dec!(0.01),
                    price - dec!(0.03),
                )
            } else {
                (
                    price,
                    price + dec!(0.03),
                    price + dec!(0.04),
                    price - dec!(0.01),
                )
            };
            let bar = make_bar(open, high, low, close);
            analyzer.process_bar(&bar);
            price = close;
        }

        let ctx = analyzer.current_context();
        assert_eq!(ctx.bar_count, 20);
        // The context should have tracked bar data and classifications
        assert!(!ctx.bar_classifications.is_empty());
    }

    #[test]
    fn test_context_retains_limited_classifications() {
        let config = PAConfig {
            max_recent_classifications: 5,
            ..Default::default()
        };
        let mut analyzer = PriceActionAnalyzer::new(config);

        for i in 0..10 {
            let p = dec!(3.00) + Decimal::from(i) * dec!(0.01);
            let bar = make_bar(p, p + dec!(0.05), p - dec!(0.01), p + dec!(0.04));
            analyzer.process_bar(&bar);
        }

        assert_eq!(analyzer.current_context().bar_classifications.len(), 5);
    }
}
