use brooks_core::bar::Bar;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::bar_analysis::BarClassification;

/// Tracks sequences of consecutive bars in the same direction.
///
/// In Brooks PA, counting consecutive bars in the same direction is fundamental:
/// - A strong trend has many consecutive trend bars
/// - A climax is often identified by an excessive number of consecutive bars
/// - Bar counting helps identify two-legged pullbacks and measured moves
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarCounter {
    /// Number of consecutive bull bars in the current sequence
    consecutive_bull: u32,
    /// Number of consecutive bear bars in the current sequence
    consecutive_bear: u32,
    /// Total bars since the last direction change
    bars_since_last_reversal: u32,
    /// Highest high within the current sequence
    high_watermark: Decimal,
    /// Lowest low within the current sequence
    low_watermark: Decimal,
    /// Total bar count processed
    total_bars: u64,
    /// Threshold for climax detection (consecutive bars)
    climax_threshold: u32,
}

impl BarCounter {
    pub fn new(climax_threshold: u32) -> Self {
        Self {
            consecutive_bull: 0,
            consecutive_bear: 0,
            bars_since_last_reversal: 0,
            high_watermark: Decimal::ZERO,
            low_watermark: Decimal::MAX,
            total_bars: 0,
            climax_threshold,
        }
    }

    /// Update the counter with a new bar and its classification
    pub fn update(&mut self, bar: &Bar, classification: &BarClassification) {
        self.total_bars += 1;

        if classification.bar_type.is_bull() {
            if self.consecutive_bear > 0 {
                // Direction changed from bear to bull
                self.consecutive_bear = 0;
                self.bars_since_last_reversal = 0;
                self.high_watermark = bar.high;
                self.low_watermark = bar.low;
            }
            self.consecutive_bull += 1;
        } else if classification.bar_type.is_bear() {
            if self.consecutive_bull > 0 {
                // Direction changed from bull to bear
                self.consecutive_bull = 0;
                self.bars_since_last_reversal = 0;
                self.high_watermark = bar.high;
                self.low_watermark = bar.low;
            }
            self.consecutive_bear += 1;
        }
        // Doji bars don't reset the count but don't increment either

        self.bars_since_last_reversal += 1;

        // Update watermarks
        if bar.high > self.high_watermark {
            self.high_watermark = bar.high;
        }
        if bar.low < self.low_watermark {
            self.low_watermark = bar.low;
        }
    }

    pub fn consecutive_bull_count(&self) -> u32 {
        self.consecutive_bull
    }

    pub fn consecutive_bear_count(&self) -> u32 {
        self.consecutive_bear
    }

    pub fn bars_since_last_reversal(&self) -> u32 {
        self.bars_since_last_reversal
    }

    pub fn high_watermark(&self) -> Decimal {
        self.high_watermark
    }

    pub fn low_watermark(&self) -> Decimal {
        self.low_watermark
    }

    pub fn total_bars(&self) -> u64 {
        self.total_bars
    }

    /// A climax occurs when there are too many consecutive bars in one direction,
    /// suggesting exhaustion. This is a warning of a potential reversal.
    pub fn is_climax(&self) -> bool {
        self.consecutive_bull >= self.climax_threshold
            || self.consecutive_bear >= self.climax_threshold
    }

    /// Whether we're currently in a bull sequence
    pub fn in_bull_sequence(&self) -> bool {
        self.consecutive_bull > 0
    }

    /// Whether we're currently in a bear sequence
    pub fn in_bear_sequence(&self) -> bool {
        self.consecutive_bear > 0
    }

    /// Range of the current sequence (high watermark - low watermark)
    pub fn sequence_range(&self) -> Decimal {
        if self.high_watermark < self.low_watermark {
            Decimal::ZERO
        } else {
            self.high_watermark - self.low_watermark
        }
    }

    /// Reset the counter
    pub fn reset(&mut self) {
        self.consecutive_bull = 0;
        self.consecutive_bear = 0;
        self.bars_since_last_reversal = 0;
        self.high_watermark = Decimal::ZERO;
        self.low_watermark = Decimal::MAX;
        self.total_bars = 0;
    }
}

impl Default for BarCounter {
    fn default() -> Self {
        Self::new(8) // Default: 8+ consecutive bars = climax
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar_analysis::{BarAnalysisConfig, BarAnalyzer};
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

    fn make_bull_bar() -> Bar {
        make_bar(dec!(3.00), dec!(3.10), dec!(2.99), dec!(3.09))
    }

    fn make_bear_bar() -> Bar {
        make_bar(dec!(3.09), dec!(3.10), dec!(2.99), dec!(3.00))
    }

    #[test]
    fn test_consecutive_bull_bars() {
        let mut counter = BarCounter::default();
        let mut analyzer = BarAnalyzer::new(BarAnalysisConfig::default());

        let bar1 = make_bull_bar();
        let c1 = analyzer.classify(&bar1, None);
        counter.update(&bar1, &c1);
        assert_eq!(counter.consecutive_bull_count(), 1);
        assert_eq!(counter.consecutive_bear_count(), 0);

        let bar2 = make_bull_bar();
        let c2 = analyzer.classify(&bar2, Some(&bar1));
        counter.update(&bar2, &c2);
        assert_eq!(counter.consecutive_bull_count(), 2);
        assert!(counter.in_bull_sequence());
    }

    #[test]
    fn test_direction_change_resets_count() {
        let mut counter = BarCounter::default();
        let mut analyzer = BarAnalyzer::new(BarAnalysisConfig::default());

        let bull = make_bull_bar();
        let c_bull = analyzer.classify(&bull, None);
        counter.update(&bull, &c_bull);
        assert_eq!(counter.consecutive_bull_count(), 1);

        let bear = make_bear_bar();
        let c_bear = analyzer.classify(&bear, Some(&bull));
        counter.update(&bear, &c_bear);
        assert_eq!(counter.consecutive_bull_count(), 0);
        assert_eq!(counter.consecutive_bear_count(), 1);
        assert!(counter.in_bear_sequence());
    }

    #[test]
    fn test_climax_detection() {
        let mut counter = BarCounter::new(3); // Low threshold for testing
        let mut analyzer = BarAnalyzer::new(BarAnalysisConfig::default());

        assert!(!counter.is_climax());

        let mut prev: Option<Bar> = None;
        for _ in 0..3 {
            let bar = make_bull_bar();
            let c = analyzer.classify(&bar, prev.as_ref());
            counter.update(&bar, &c);
            prev = Some(bar);
        }
        assert!(counter.is_climax());
        assert_eq!(counter.consecutive_bull_count(), 3);
    }

    #[test]
    fn test_watermarks() {
        let mut counter = BarCounter::default();
        let mut analyzer = BarAnalyzer::new(BarAnalysisConfig::default());

        let bar1 = make_bar(dec!(3.00), dec!(3.05), dec!(2.95), dec!(3.04));
        let c1 = analyzer.classify(&bar1, None);
        counter.update(&bar1, &c1);

        let bar2 = make_bar(dec!(3.04), dec!(3.12), dec!(3.00), dec!(3.10));
        let c2 = analyzer.classify(&bar2, Some(&bar1));
        counter.update(&bar2, &c2);

        assert_eq!(counter.high_watermark(), dec!(3.12));
        // low_watermark initially MAX, updated to 2.95 on first bar
        assert_eq!(counter.low_watermark(), dec!(2.95));
    }

    #[test]
    fn test_total_bars() {
        let mut counter = BarCounter::default();
        let mut analyzer = BarAnalyzer::new(BarAnalysisConfig::default());

        for _ in 0..5 {
            let bar = make_bull_bar();
            let c = analyzer.classify(&bar, None);
            counter.update(&bar, &c);
        }
        assert_eq!(counter.total_bars(), 5);
    }

    #[test]
    fn test_reset() {
        let mut counter = BarCounter::default();
        let mut analyzer = BarAnalyzer::new(BarAnalysisConfig::default());

        let bar = make_bull_bar();
        let c = analyzer.classify(&bar, None);
        counter.update(&bar, &c);

        counter.reset();
        assert_eq!(counter.consecutive_bull_count(), 0);
        assert_eq!(counter.consecutive_bear_count(), 0);
        assert_eq!(counter.total_bars(), 0);
    }
}
