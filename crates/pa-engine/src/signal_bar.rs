use brooks_core::bar::Bar;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::context::MarketContext;

/// Quality rating of a signal bar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalQuality {
    Strong,
    Moderate,
    Weak,
}

/// Configuration for signal bar detection
#[derive(Debug, Clone)]
pub struct SignalBarConfig {
    /// Minimum body/range ratio for a signal bar
    pub min_body_ratio: Decimal,
    /// Maximum tail/range ratio on the "wrong" side (e.g., upper tail on bull signal bar)
    pub max_wrong_side_tail: Decimal,
    /// Minimum bar range relative to recent average to qualify
    pub min_range_ratio: Decimal,
}

impl Default for SignalBarConfig {
    fn default() -> Self {
        Self {
            min_body_ratio: dec!(0.30),
            max_wrong_side_tail: dec!(0.30),
            min_range_ratio: dec!(0.50),
        }
    }
}

/// Identifies signal bars according to Brooks PA methodology.
///
/// A signal bar sets up a potential trade entry. For example:
/// - Bull signal bar: closes on or near its high, has a body of reasonable size,
///   and appears at a location where a long entry makes sense (e.g., near support,
///   at a pullback in a bull trend).
/// - Bear signal bar: closes on or near its low, appears where a short makes sense.
pub struct SignalBarDetector {
    config: SignalBarConfig,
}

impl SignalBarDetector {
    pub fn new(config: SignalBarConfig) -> Self {
        Self { config }
    }

    /// Check if the bar is a bull signal bar given the current market context
    pub fn is_bull_signal_bar(&self, bar: &Bar, context: &MarketContext) -> bool {
        // Must be a bull bar (or doji closing in upper half)
        if !bar.is_bull() && !bar.closes_in_upper_half() {
            return false;
        }

        // Must have a reasonable body
        if bar.body_ratio() < self.config.min_body_ratio {
            return false;
        }

        // Upper tail should not be too large (bar should close near its high)
        if bar.upper_tail_ratio() > self.config.max_wrong_side_tail {
            return false;
        }

        // Context must favor a long entry
        self.context_favors_bull_signal(context)
    }

    /// Check if the bar is a bear signal bar given the current market context
    pub fn is_bear_signal_bar(&self, bar: &Bar, context: &MarketContext) -> bool {
        // Must be a bear bar (or doji closing in lower half)
        if !bar.is_bear() && !bar.closes_in_lower_half() {
            return false;
        }

        // Must have a reasonable body
        if bar.body_ratio() < self.config.min_body_ratio {
            return false;
        }

        // Lower tail should not be too large (bar should close near its low)
        if bar.lower_tail_ratio() > self.config.max_wrong_side_tail {
            return false;
        }

        self.context_favors_bear_signal(context)
    }

    /// Assess the quality of a bull signal bar
    pub fn bull_signal_quality(&self, bar: &Bar, context: &MarketContext) -> SignalQuality {
        let mut score = 0u32;

        // Body ratio — larger body is better
        if bar.body_ratio() >= dec!(0.60) {
            score += 2;
        } else if bar.body_ratio() >= dec!(0.40) {
            score += 1;
        }

        // Close near the high — smaller upper tail is better
        if bar.upper_tail_ratio() <= dec!(0.10) {
            score += 2;
        } else if bar.upper_tail_ratio() <= dec!(0.20) {
            score += 1;
        }

        // Trend alignment
        if context.trend.is_bull() {
            score += 2;
        }

        // Near support
        if context.nearest_support.is_some() {
            score += 1;
        }

        if score >= 5 {
            SignalQuality::Strong
        } else if score >= 3 {
            SignalQuality::Moderate
        } else {
            SignalQuality::Weak
        }
    }

    /// Assess the quality of a bear signal bar
    pub fn bear_signal_quality(&self, bar: &Bar, context: &MarketContext) -> SignalQuality {
        let mut score = 0u32;

        if bar.body_ratio() >= dec!(0.60) {
            score += 2;
        } else if bar.body_ratio() >= dec!(0.40) {
            score += 1;
        }

        if bar.lower_tail_ratio() <= dec!(0.10) {
            score += 2;
        } else if bar.lower_tail_ratio() <= dec!(0.20) {
            score += 1;
        }

        if context.trend.is_bear() {
            score += 2;
        }

        if context.nearest_resistance.is_some() {
            score += 1;
        }

        if score >= 5 {
            SignalQuality::Strong
        } else if score >= 3 {
            SignalQuality::Moderate
        } else {
            SignalQuality::Weak
        }
    }

    fn context_favors_bull_signal(&self, context: &MarketContext) -> bool {
        // Favored when:
        // 1. In a bull trend (pullback entry)
        // 2. Near support in a trading range
        // 3. Failed bear breakout
        // 4. After a bear climax (exhaustion reversal)
        context.trend.is_bull()
            || (context.in_trading_range && context.near_support())
            || context.is_climax && context.consecutive_bear_bars > 0
            || !context.active_breakouts.is_empty()
    }

    fn context_favors_bear_signal(&self, context: &MarketContext) -> bool {
        context.trend.is_bear()
            || (context.in_trading_range && context.near_resistance())
            || context.is_climax && context.consecutive_bull_bars > 0
            || !context.active_breakouts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trend::TrendState;
    use brooks_core::market::{Exchange, SecurityId};
    use brooks_core::timeframe::Timeframe;
    use chrono::Utc;
    use rust_decimal_macros::dec;

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

    fn bull_context() -> MarketContext {
        MarketContext {
            trend: TrendState::BullTrend,
            trend_strength: 0.7,
            ..Default::default()
        }
    }

    fn bear_context() -> MarketContext {
        MarketContext {
            trend: TrendState::BearTrend,
            trend_strength: 0.7,
            ..Default::default()
        }
    }

    #[test]
    fn test_bull_signal_bar() {
        let detector = SignalBarDetector::new(SignalBarConfig::default());
        // Strong bull bar: closes near high, decent body
        // range=0.10, body=0.07, upper_tail=0.01
        let bar = make_bar(dec!(3.02), dec!(3.10), dec!(3.00), dec!(3.09));
        assert!(detector.is_bull_signal_bar(&bar, &bull_context()));
    }

    #[test]
    fn test_not_bull_signal_when_bear_bar() {
        let detector = SignalBarDetector::new(SignalBarConfig::default());
        let bar = make_bar(dec!(3.08), dec!(3.10), dec!(3.00), dec!(3.02));
        assert!(!detector.is_bull_signal_bar(&bar, &bull_context()));
    }

    #[test]
    fn test_not_bull_signal_with_large_upper_tail() {
        let detector = SignalBarDetector::new(SignalBarConfig::default());
        // Large upper tail: close far from high
        // range=0.20, upper_tail=0.10 (50% > 30% threshold)
        let bar = make_bar(dec!(3.00), dec!(3.20), dec!(3.00), dec!(3.10));
        assert!(!detector.is_bull_signal_bar(&bar, &bull_context()));
    }

    #[test]
    fn test_bear_signal_bar() {
        let detector = SignalBarDetector::new(SignalBarConfig::default());
        // Strong bear bar: closes near low
        let bar = make_bar(dec!(3.08), dec!(3.10), dec!(3.00), dec!(3.01));
        assert!(detector.is_bear_signal_bar(&bar, &bear_context()));
    }

    #[test]
    fn test_signal_quality_strong() {
        let detector = SignalBarDetector::new(SignalBarConfig::default());
        // Very strong bull bar
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.99), dec!(3.10));
        let quality = detector.bull_signal_quality(&bar, &bull_context());
        assert_eq!(quality, SignalQuality::Strong);
    }

    #[test]
    fn test_signal_quality_weak() {
        let detector = SignalBarDetector::new(SignalBarConfig::default());
        // Weak bull bar: small body, large tails
        let bar = make_bar(dec!(3.04), dec!(3.10), dec!(3.00), dec!(3.06));
        let ctx = MarketContext {
            trend: TrendState::TradingRange,
            ..Default::default()
        };
        let quality = detector.bull_signal_quality(&bar, &ctx);
        assert_eq!(quality, SignalQuality::Weak);
    }
}
