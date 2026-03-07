use brooks_core::bar::Bar;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Brooks PA bar type classification based on body size and tails
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarType {
    /// Large bull body, small tails — strong buying pressure
    StrongBullTrend,
    /// Bull body with moderate tails
    BullTrend,
    /// Small bull body with large tails — weak buying
    WeakBull,
    /// Tiny or no body — indecision
    Doji,
    /// Small bear body with large tails — weak selling
    WeakBear,
    /// Bear body with moderate tails
    BearTrend,
    /// Large bear body, small tails — strong selling pressure
    StrongBearTrend,
}

impl BarType {
    pub fn is_bull(&self) -> bool {
        matches!(
            self,
            BarType::StrongBullTrend | BarType::BullTrend | BarType::WeakBull
        )
    }

    pub fn is_bear(&self) -> bool {
        matches!(
            self,
            BarType::StrongBearTrend | BarType::BearTrend | BarType::WeakBear
        )
    }

    pub fn is_strong(&self) -> bool {
        matches!(self, BarType::StrongBullTrend | BarType::StrongBearTrend)
    }
}

/// Relative body size compared to recent average
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodySize {
    Large,
    Medium,
    Small,
    Doji,
}

/// Analysis of a bar's upper and lower tails
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TailAnalysis {
    /// Upper tail as a ratio of total range (0.0 to 1.0)
    pub upper_tail_ratio: Decimal,
    /// Lower tail as a ratio of total range (0.0 to 1.0)
    pub lower_tail_ratio: Decimal,
    /// Whether the upper tail is prominent (> 33% of range)
    pub has_prominent_upper_tail: bool,
    /// Whether the lower tail is prominent (> 33% of range)
    pub has_prominent_lower_tail: bool,
}

/// Gap direction relative to previous bar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapType {
    GapUp,
    GapDown,
}

/// Full classification of a single bar in Brooks PA context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarClassification {
    pub bar_type: BarType,
    pub body_relative_size: BodySize,
    pub tail_analysis: TailAnalysis,
    pub is_signal_bar: bool,
    pub is_entry_bar: bool,
    pub is_reversal_bar: bool,
    pub is_inside_bar: bool,
    pub is_outside_bar: bool,
    pub gap: Option<GapType>,
}

/// Configuration for bar analysis thresholds
#[derive(Debug, Clone)]
pub struct BarAnalysisConfig {
    /// Body/range ratio below which a bar is classified as a doji
    pub doji_threshold: Decimal,
    /// Body/range ratio above which a bar body is "strong"
    pub strong_body_threshold: Decimal,
    /// Tail/range ratio above which a tail is "prominent"
    pub prominent_tail_threshold: Decimal,
    /// Number of bars to average for relative body size
    pub lookback_period: usize,
    /// Multiplier for "large" body relative to average
    pub large_body_multiplier: Decimal,
    /// Multiplier for "small" body relative to average
    pub small_body_multiplier: Decimal,
}

impl Default for BarAnalysisConfig {
    fn default() -> Self {
        Self {
            doji_threshold: dec!(0.20),
            strong_body_threshold: dec!(0.65),
            prominent_tail_threshold: dec!(0.33),
            lookback_period: 20,
            large_body_multiplier: dec!(1.5),
            small_body_multiplier: dec!(0.5),
        }
    }
}

/// Classifies individual bars according to Brooks Price Action methodology
pub struct BarAnalyzer {
    config: BarAnalysisConfig,
    /// Recent bar body sizes for computing relative size
    recent_body_sizes: VecDeque<Decimal>,
}

impl BarAnalyzer {
    pub fn new(config: BarAnalysisConfig) -> Self {
        Self {
            recent_body_sizes: VecDeque::with_capacity(config.lookback_period),
            config,
        }
    }

    /// Classify a bar given the previous bar (for inside/outside/gap detection).
    /// `prev_bar` is None for the very first bar.
    pub fn classify(&mut self, bar: &Bar, prev_bar: Option<&Bar>) -> BarClassification {
        let tail_analysis = self.analyze_tails(bar);
        let body_relative_size = self.relative_body_size(bar);
        let bar_type = self.classify_bar_type(bar, &tail_analysis, &body_relative_size);

        let is_inside_bar = prev_bar.is_some_and(|prev| bar.is_inside_bar(prev));
        let is_outside_bar = prev_bar.is_some_and(|prev| bar.is_outside_bar(prev));
        let gap = prev_bar.and_then(|prev| self.detect_gap(bar, prev));

        // A reversal bar: closes opposite to its direction with a prominent tail
        // on the reversal side. E.g., bull reversal bar has long lower tail, closes near high.
        let is_reversal_bar = self.is_reversal_bar(bar, &tail_analysis, prev_bar);

        // Track body size for future relative calculations
        self.update_body_history(bar);

        BarClassification {
            bar_type,
            body_relative_size,
            tail_analysis,
            is_signal_bar: false, // determined later by SignalBarDetector with context
            is_entry_bar: false,  // determined later by EntryBarDetector
            is_reversal_bar,
            is_inside_bar,
            is_outside_bar,
            gap,
        }
    }

    fn analyze_tails(&self, bar: &Bar) -> TailAnalysis {
        let upper = bar.upper_tail_ratio();
        let lower = bar.lower_tail_ratio();
        TailAnalysis {
            upper_tail_ratio: upper,
            lower_tail_ratio: lower,
            has_prominent_upper_tail: upper >= self.config.prominent_tail_threshold,
            has_prominent_lower_tail: lower >= self.config.prominent_tail_threshold,
        }
    }

    fn relative_body_size(&self, bar: &Bar) -> BodySize {
        let body = bar.body_size();
        let range = bar.range();

        // If the range is zero or near zero, it's a doji
        if range.is_zero() || bar.body_ratio() <= self.config.doji_threshold {
            return BodySize::Doji;
        }

        // Compare to recent average body size
        if self.recent_body_sizes.is_empty() {
            // No history yet — classify by body ratio alone
            return self.classify_body_by_ratio(bar);
        }

        let avg_body: Decimal = self.recent_body_sizes.iter().copied().sum::<Decimal>()
            / Decimal::from(self.recent_body_sizes.len() as u64);

        if avg_body.is_zero() {
            return self.classify_body_by_ratio(bar);
        }

        if body >= avg_body * self.config.large_body_multiplier {
            BodySize::Large
        } else if body <= avg_body * self.config.small_body_multiplier {
            BodySize::Small
        } else {
            BodySize::Medium
        }
    }

    fn classify_body_by_ratio(&self, bar: &Bar) -> BodySize {
        let ratio = bar.body_ratio();
        if ratio <= self.config.doji_threshold {
            BodySize::Doji
        } else if ratio >= self.config.strong_body_threshold {
            BodySize::Large
        } else if ratio <= dec!(0.35) {
            BodySize::Small
        } else {
            BodySize::Medium
        }
    }

    fn classify_bar_type(&self, bar: &Bar, tails: &TailAnalysis, body_size: &BodySize) -> BarType {
        if matches!(body_size, BodySize::Doji) {
            return BarType::Doji;
        }

        let is_bull = bar.is_bull();
        let body_ratio = bar.body_ratio();
        let has_small_tails = !tails.has_prominent_upper_tail && !tails.has_prominent_lower_tail;

        if is_bull {
            if body_ratio >= self.config.strong_body_threshold && has_small_tails {
                BarType::StrongBullTrend
            } else if tails.has_prominent_upper_tail || tails.has_prominent_lower_tail {
                BarType::WeakBull
            } else {
                BarType::BullTrend
            }
        } else {
            // Bear or exactly equal (treated as bear)
            if body_ratio >= self.config.strong_body_threshold && has_small_tails {
                BarType::StrongBearTrend
            } else if tails.has_prominent_upper_tail || tails.has_prominent_lower_tail {
                BarType::WeakBear
            } else {
                BarType::BearTrend
            }
        }
    }

    fn detect_gap(&self, bar: &Bar, prev: &Bar) -> Option<GapType> {
        if bar.low > prev.high {
            Some(GapType::GapUp)
        } else if bar.high < prev.low {
            Some(GapType::GapDown)
        } else {
            None
        }
    }

    fn is_reversal_bar(&self, bar: &Bar, tails: &TailAnalysis, prev: Option<&Bar>) -> bool {
        let Some(prev) = prev else {
            return false;
        };

        // Bull reversal bar: previous bar was bear, this bar has prominent lower tail
        // and closes in upper half
        if prev.is_bear() && tails.has_prominent_lower_tail && bar.closes_in_upper_half() {
            return true;
        }

        // Bear reversal bar: previous bar was bull, this bar has prominent upper tail
        // and closes in lower half
        if prev.is_bull() && tails.has_prominent_upper_tail && bar.closes_in_lower_half() {
            return true;
        }

        false
    }

    fn update_body_history(&mut self, bar: &Bar) {
        let body = bar.body_size();
        if self.recent_body_sizes.len() >= self.config.lookback_period {
            self.recent_body_sizes.pop_front();
        }
        self.recent_body_sizes.push_back(body);
    }

    /// Get the average body size of recent bars
    pub fn average_body_size(&self) -> Decimal {
        if self.recent_body_sizes.is_empty() {
            return Decimal::ZERO;
        }
        self.recent_body_sizes.iter().copied().sum::<Decimal>()
            / Decimal::from(self.recent_body_sizes.len() as u64)
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

    fn analyzer() -> BarAnalyzer {
        BarAnalyzer::new(BarAnalysisConfig::default())
    }

    #[test]
    fn test_strong_bull_trend_bar() {
        let mut a = analyzer();
        // Large bull body, small tails: open=3.00, high=3.10, low=2.99, close=3.09
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.99), dec!(3.09));
        let c = a.classify(&bar, None);
        assert_eq!(c.bar_type, BarType::StrongBullTrend);
        assert!(c.bar_type.is_bull());
        assert!(c.bar_type.is_strong());
    }

    #[test]
    fn test_strong_bear_trend_bar() {
        let mut a = analyzer();
        let bar = make_bar(dec!(3.09), dec!(3.10), dec!(2.99), dec!(3.00));
        let c = a.classify(&bar, None);
        assert_eq!(c.bar_type, BarType::StrongBearTrend);
        assert!(c.bar_type.is_bear());
        assert!(c.bar_type.is_strong());
    }

    #[test]
    fn test_doji_bar() {
        let mut a = analyzer();
        // Very small body relative to range
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.01));
        let c = a.classify(&bar, None);
        assert_eq!(c.bar_type, BarType::Doji);
    }

    #[test]
    fn test_weak_bull_with_prominent_tails() {
        let mut a = analyzer();
        // Bull bar with prominent lower tail
        // range = 0.20, body = 0.06 (30%), lower_tail = 0.10 (50%)
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.06));
        let c = a.classify(&bar, None);
        assert_eq!(c.bar_type, BarType::WeakBull);
    }

    #[test]
    fn test_inside_bar() {
        let mut a = analyzer();
        let prev = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.05));
        let curr = make_bar(dec!(3.02), dec!(3.06), dec!(2.94), dec!(3.04));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert!(c.is_inside_bar);
        assert!(!c.is_outside_bar);
    }

    #[test]
    fn test_outside_bar() {
        let mut a = analyzer();
        let prev = make_bar(dec!(3.02), dec!(3.06), dec!(2.94), dec!(3.04));
        let curr = make_bar(dec!(3.00), dec!(3.12), dec!(2.88), dec!(3.05));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert!(c.is_outside_bar);
        assert!(!c.is_inside_bar);
    }

    #[test]
    fn test_gap_up() {
        let mut a = analyzer();
        let prev = make_bar(dec!(3.00), dec!(3.05), dec!(2.95), dec!(3.04));
        let curr = make_bar(dec!(3.10), dec!(3.15), dec!(3.06), dec!(3.12));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert_eq!(c.gap, Some(GapType::GapUp));
    }

    #[test]
    fn test_gap_down() {
        let mut a = analyzer();
        let prev = make_bar(dec!(3.10), dec!(3.15), dec!(3.06), dec!(3.08));
        let curr = make_bar(dec!(3.00), dec!(3.04), dec!(2.95), dec!(2.98));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert_eq!(c.gap, Some(GapType::GapDown));
    }

    #[test]
    fn test_no_gap() {
        let mut a = analyzer();
        let prev = make_bar(dec!(3.00), dec!(3.05), dec!(2.95), dec!(3.04));
        let curr = make_bar(dec!(3.02), dec!(3.08), dec!(2.98), dec!(3.06));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert_eq!(c.gap, None);
    }

    #[test]
    fn test_bull_reversal_bar() {
        let mut a = analyzer();
        // Previous bear bar
        let prev = make_bar(dec!(3.05), dec!(3.06), dec!(2.95), dec!(2.96));
        // Current bar: prominent lower tail, closes in upper half
        // range=0.15, lower_tail = 3.00-2.90 = 0.10 (67%), close > midpoint
        let curr = make_bar(dec!(3.00), dec!(3.05), dec!(2.90), dec!(3.03));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert!(c.is_reversal_bar);
    }

    #[test]
    fn test_bear_reversal_bar() {
        let mut a = analyzer();
        // Previous bull bar
        let prev = make_bar(dec!(2.96), dec!(3.06), dec!(2.95), dec!(3.05));
        // Current bar: prominent upper tail, closes in lower half
        // range=0.15, upper_tail = 3.10-3.00 = 0.10 (67%), close < midpoint
        let curr = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(2.97));
        let _ = a.classify(&prev, None);
        let c = a.classify(&curr, Some(&prev));
        assert!(c.is_reversal_bar);
    }

    #[test]
    fn test_relative_body_size_with_history() {
        let mut a = analyzer();
        // Feed some bars with small bodies to establish a baseline
        for _ in 0..10 {
            let bar = make_bar(dec!(3.00), dec!(3.05), dec!(2.95), dec!(3.02));
            a.classify(&bar, None);
        }
        // Now a bar with a much larger body should be classified as Large
        let big = make_bar(dec!(3.00), dec!(3.12), dec!(2.99), dec!(3.11));
        let c = a.classify(&big, None);
        assert_eq!(c.body_relative_size, BodySize::Large);
    }

    #[test]
    fn test_zero_range_bar() {
        let mut a = analyzer();
        let bar = make_bar(dec!(3.00), dec!(3.00), dec!(3.00), dec!(3.00));
        let c = a.classify(&bar, None);
        assert_eq!(c.bar_type, BarType::Doji);
        assert_eq!(c.body_relative_size, BodySize::Doji);
    }
}
