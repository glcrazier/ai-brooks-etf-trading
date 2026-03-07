use brooks_core::bar::Bar;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::trend::TrendState;

/// A detected trading range (horizontal price channel)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingRange {
    pub high: Decimal,
    pub low: Decimal,
    pub start_index: usize,
    pub bar_count: u32,
    /// Whether this is a tight trading range (narrow relative to bar size)
    pub is_tight: bool,
}

impl TradingRange {
    /// Range width (high - low)
    pub fn width(&self) -> Decimal {
        self.high - self.low
    }

    /// Midpoint of the range
    pub fn midpoint(&self) -> Decimal {
        (self.high + self.low) / Decimal::TWO
    }

    /// Whether a price is within this range
    pub fn contains(&self, price: Decimal) -> bool {
        price >= self.low && price <= self.high
    }

    /// Whether a price is in the upper third of the range
    pub fn in_upper_third(&self, price: Decimal) -> bool {
        let third = self.width() / Decimal::from(3);
        price >= self.high - third
    }

    /// Whether a price is in the lower third of the range
    pub fn in_lower_third(&self, price: Decimal) -> bool {
        let third = self.width() / Decimal::from(3);
        price <= self.low + third
    }
}

/// Configuration for trading range detection
#[derive(Debug, Clone)]
pub struct TradingRangeConfig {
    /// Minimum bars for a valid trading range
    pub min_bars: u32,
    /// Fraction of bars that must stay within the range boundaries
    pub containment_ratio: Decimal,
    /// Range width / average bar range ratio below which it's "tight"
    pub tight_range_threshold: Decimal,
}

impl Default for TradingRangeConfig {
    fn default() -> Self {
        Self {
            min_bars: 20,
            containment_ratio: dec!(0.70),
            tight_range_threshold: dec!(3.0),
        }
    }
}

/// Detects trading ranges (periods where price oscillates within boundaries)
pub struct TradingRangeDetector {
    /// Currently active trading ranges
    active_ranges: Vec<TradingRange>,
    /// Recent bar data for range analysis
    recent_highs: Vec<Decimal>,
    recent_lows: Vec<Decimal>,
    recent_ranges: Vec<Decimal>,
    bar_index: usize,
    config: TradingRangeConfig,
}

impl TradingRangeDetector {
    pub fn new(config: TradingRangeConfig) -> Self {
        Self {
            active_ranges: Vec::new(),
            recent_highs: Vec::new(),
            recent_lows: Vec::new(),
            recent_ranges: Vec::new(),
            bar_index: 0,
            config,
        }
    }

    /// Update with a new bar and current trend state
    pub fn update(&mut self, bar: &Bar, trend: TrendState) {
        self.recent_highs.push(bar.high);
        self.recent_lows.push(bar.low);
        self.recent_ranges.push(bar.range());

        // First, invalidate ranges broken by this bar
        self.update_active_ranges(bar);

        // Only try to detect new ranges when in a trading range state
        // (don't create ranges during strong trends)
        if trend == TrendState::TradingRange {
            let min_bars = self.config.min_bars as usize;
            if self.recent_highs.len() >= min_bars {
                self.detect_range(min_bars);
            }
        }

        self.bar_index += 1;
    }

    /// Whether we're currently in any trading range
    pub fn is_in_trading_range(&self) -> bool {
        !self.active_ranges.is_empty()
    }

    /// Get the most recent (current) trading range
    pub fn current_range(&self) -> Option<&TradingRange> {
        self.active_ranges.last()
    }

    /// Get all active ranges
    pub fn active_ranges(&self) -> &[TradingRange] {
        &self.active_ranges
    }

    fn detect_range(&mut self, lookback: usize) {
        let n = self.recent_highs.len();
        let start = n - lookback;

        let window_highs = &self.recent_highs[start..];
        let window_lows = &self.recent_lows[start..];

        let max_high = window_highs.iter().copied().max().unwrap_or(Decimal::ZERO);
        let min_low = window_lows.iter().copied().min().unwrap_or(Decimal::ZERO);

        let range_width = max_high - min_low;
        if range_width.is_zero() {
            return;
        }

        // Count how many bars are contained within the range
        let contained = window_highs
            .iter()
            .zip(window_lows.iter())
            .filter(|(&h, &l)| h <= max_high && l >= min_low)
            .count();

        let containment = Decimal::from(contained as u64) / Decimal::from(lookback as u64);

        if containment >= self.config.containment_ratio {
            // Check if this is a tight range
            let avg_bar_range = if self.recent_ranges.len() >= lookback {
                let sum: Decimal = self.recent_ranges[start..].iter().copied().sum();
                sum / Decimal::from(lookback as u64)
            } else {
                range_width // fallback
            };

            let is_tight = if avg_bar_range.is_zero() {
                false
            } else {
                range_width / avg_bar_range <= self.config.tight_range_threshold
            };

            let new_range = TradingRange {
                high: max_high,
                low: min_low,
                start_index: self.bar_index.saturating_sub(lookback),
                bar_count: lookback as u32,
                is_tight,
            };

            // Replace or update existing range
            if let Some(existing) = self.active_ranges.last_mut() {
                // If the new range overlaps significantly, update it
                if (existing.high - new_range.high).abs() < range_width * dec!(0.2)
                    && (existing.low - new_range.low).abs() < range_width * dec!(0.2)
                {
                    existing.high = new_range.high;
                    existing.low = new_range.low;
                    existing.bar_count = new_range.bar_count;
                    existing.is_tight = new_range.is_tight;
                    return;
                }
            }

            self.active_ranges.push(new_range);
        }
    }

    fn update_active_ranges(&mut self, bar: &Bar) {
        // Remove ranges that have been broken (bar closes significantly outside)
        self.active_ranges.retain(|range| {
            let margin = range.width() * dec!(0.10);
            !(bar.close > range.high + margin || bar.close < range.low - margin)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Exchange, SecurityId};
    use brooks_core::timeframe::Timeframe;
    use chrono::Utc;

    fn make_bar(low: Decimal, high: Decimal, close: Decimal) -> Bar {
        Bar {
            timestamp: Utc::now(),
            open: (low + high) / Decimal::TWO,
            high,
            low,
            close,
            volume: 1000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    #[test]
    fn test_no_range_with_few_bars() {
        let mut detector = TradingRangeDetector::new(TradingRangeConfig::default());

        for _ in 0..5 {
            let bar = make_bar(dec!(3.00), dec!(3.05), dec!(3.03));
            detector.update(&bar, TrendState::TradingRange);
        }

        assert!(!detector.is_in_trading_range());
    }

    #[test]
    fn test_detect_trading_range() {
        let config = TradingRangeConfig {
            min_bars: 5,
            containment_ratio: dec!(0.70),
            tight_range_threshold: dec!(3.0),
        };
        let mut detector = TradingRangeDetector::new(config);

        // Create bars that oscillate within a range
        let closes = [
            dec!(3.02),
            dec!(3.04),
            dec!(3.01),
            dec!(3.03),
            dec!(3.02),
            dec!(3.04),
            dec!(3.01),
        ];

        for close in &closes {
            let bar = make_bar(close - dec!(0.02), close + dec!(0.02), *close);
            detector.update(&bar, TrendState::TradingRange);
        }

        assert!(detector.is_in_trading_range());
    }

    #[test]
    fn test_trading_range_properties() {
        let range = TradingRange {
            high: dec!(3.10),
            low: dec!(3.00),
            start_index: 0,
            bar_count: 20,
            is_tight: false,
        };

        assert_eq!(range.width(), dec!(0.10));
        assert_eq!(range.midpoint(), dec!(3.05));
        assert!(range.contains(dec!(3.05)));
        assert!(!range.contains(dec!(3.15)));
        assert!(range.in_upper_third(dec!(3.08)));
        assert!(range.in_lower_third(dec!(3.02)));
    }

    #[test]
    fn test_range_invalidated_on_breakout() {
        let config = TradingRangeConfig {
            min_bars: 5,
            ..Default::default()
        };
        let mut detector = TradingRangeDetector::new(config);

        // Build a range
        for _ in 0..7 {
            let bar = make_bar(dec!(3.00), dec!(3.05), dec!(3.03));
            detector.update(&bar, TrendState::TradingRange);
        }

        // Break out of range with a big move
        let breakout_bar = make_bar(dec!(3.04), dec!(3.20), dec!(3.18));
        detector.update(&breakout_bar, TrendState::BullTrend);

        assert!(!detector.is_in_trading_range());
    }
}
