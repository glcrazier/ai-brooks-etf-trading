use brooks_core::bar::Bar;
use brooks_core::market::Direction;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::support_resistance::SRLevel;
use crate::trend::TrendState;

/// Type of breakout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakoutType {
    SupportResistance,
    TradingRange,
    Channel,
    TrendLine,
}

/// Strength assessment of a breakout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakoutStrength {
    /// Large breakout bar, closes far beyond the level
    Strong,
    /// Moderate bar, closes somewhat beyond the level
    Moderate,
    /// Small bar, barely closes beyond the level
    Weak,
}

/// Tracks the follow-through after a breakout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FollowThrough {
    /// Breakout just occurred, awaiting confirmation
    Pending,
    /// Breakout confirmed — price held beyond the level for N bars
    Confirmed { bars_held: u32 },
    /// Breakout failed — price reversed back through the level
    Failed { reversal_bar: usize },
}

/// A detected breakout event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakoutEvent {
    pub breakout_type: BreakoutType,
    pub direction: Direction,
    /// The price level that was broken
    pub level: Decimal,
    /// Bar index where the breakout occurred
    pub bar_index: usize,
    pub strength: BreakoutStrength,
    pub follow_through: FollowThrough,
}

/// Configuration for breakout detection
#[derive(Debug, Clone)]
pub struct BreakoutConfig {
    /// Number of bars to wait before confirming/failing a breakout
    pub confirmation_bars: u32,
    /// Minimum close beyond the level (as fraction of bar range) to count as breakout
    pub min_close_beyond_pct: Decimal,
}

impl Default for BreakoutConfig {
    fn default() -> Self {
        Self {
            confirmation_bars: 3,
            min_close_beyond_pct: Decimal::new(25, 2), // 0.25 = 25% of bar range beyond level
        }
    }
}

/// Detects breakouts and failed breakouts of support/resistance levels
pub struct BreakoutDetector {
    /// Active (pending or confirmed) breakouts
    active_breakouts: Vec<BreakoutEvent>,
    /// Recently failed breakouts (kept for signal generation)
    recent_failed: Vec<BreakoutEvent>,
    bar_index: usize,
    config: BreakoutConfig,
    /// Max failed breakouts to retain
    max_failed_history: usize,
}

impl BreakoutDetector {
    pub fn new(config: BreakoutConfig) -> Self {
        Self {
            active_breakouts: Vec::new(),
            recent_failed: Vec::new(),
            bar_index: 0,
            config,
            max_failed_history: 20,
        }
    }

    /// Update with a new bar. Checks for new breakouts and updates existing ones.
    pub fn update(&mut self, bar: &Bar, sr_levels: &[SRLevel], _trend: TrendState) {
        // Update follow-through on existing pending breakouts
        self.update_pending_breakouts(bar);

        // Check for new breakouts of S/R levels
        self.check_new_breakouts(bar, sr_levels);

        self.bar_index += 1;
    }

    pub fn active_breakouts(&self) -> &[BreakoutEvent] {
        &self.active_breakouts
    }

    pub fn recent_failed_breakouts(&self) -> &[BreakoutEvent] {
        &self.recent_failed
    }

    /// Check if any pending breakout has been confirmed or failed
    fn update_pending_breakouts(&mut self, bar: &Bar) {
        let confirmation_bars = self.config.confirmation_bars;
        let bar_index = self.bar_index;

        let mut newly_failed = Vec::new();

        for breakout in &mut self.active_breakouts {
            if let FollowThrough::Pending = breakout.follow_through {
                let bars_since = bar_index.saturating_sub(breakout.bar_index) as u32;

                // Check if the bar has reversed back through the level
                let reversed = match breakout.direction {
                    Direction::Long => bar.close < breakout.level,
                    Direction::Short => bar.close > breakout.level,
                };

                if reversed {
                    breakout.follow_through = FollowThrough::Failed {
                        reversal_bar: bar_index,
                    };
                    newly_failed.push(breakout.clone());
                } else if bars_since >= confirmation_bars {
                    breakout.follow_through = FollowThrough::Confirmed {
                        bars_held: bars_since,
                    };
                }
            }
        }

        // Move failed breakouts to the failed list
        for failed in newly_failed {
            self.recent_failed.push(failed);
            if self.recent_failed.len() > self.max_failed_history {
                self.recent_failed.remove(0);
            }
        }

        // Remove failed breakouts from active list
        self.active_breakouts
            .retain(|b| !matches!(b.follow_through, FollowThrough::Failed { .. }));
    }

    fn check_new_breakouts(&mut self, bar: &Bar, sr_levels: &[SRLevel]) {
        for level in sr_levels {
            // Check for upward breakout (close above resistance)
            if bar.close > level.price && bar.open <= level.price {
                let strength = self.assess_strength(bar, level.price, Direction::Long);
                self.active_breakouts.push(BreakoutEvent {
                    breakout_type: BreakoutType::SupportResistance,
                    direction: Direction::Long,
                    level: level.price,
                    bar_index: self.bar_index,
                    strength,
                    follow_through: FollowThrough::Pending,
                });
            }

            // Check for downward breakout (close below support)
            if bar.close < level.price && bar.open >= level.price {
                let strength = self.assess_strength(bar, level.price, Direction::Short);
                self.active_breakouts.push(BreakoutEvent {
                    breakout_type: BreakoutType::SupportResistance,
                    direction: Direction::Short,
                    level: level.price,
                    bar_index: self.bar_index,
                    strength,
                    follow_through: FollowThrough::Pending,
                });
            }
        }
    }

    fn assess_strength(&self, bar: &Bar, level: Decimal, direction: Direction) -> BreakoutStrength {
        let range = bar.range();
        if range.is_zero() {
            return BreakoutStrength::Weak;
        }

        let close_beyond = match direction {
            Direction::Long => bar.close - level,
            Direction::Short => level - bar.close,
        };

        let beyond_ratio = close_beyond / range;

        if beyond_ratio >= Decimal::new(60, 2) {
            BreakoutStrength::Strong
        } else if beyond_ratio >= Decimal::new(30, 2) {
            BreakoutStrength::Moderate
        } else {
            BreakoutStrength::Weak
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support_resistance::SRLevelType;
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

    fn make_sr_level(price: Decimal, lt: SRLevelType) -> SRLevel {
        SRLevel {
            price,
            level_type: lt,
            strength: 3,
            first_touch: Utc::now(),
            last_touch: Utc::now(),
        }
    }

    #[test]
    fn test_detect_upward_breakout() {
        let mut detector = BreakoutDetector::new(BreakoutConfig::default());
        let levels = vec![make_sr_level(dec!(3.10), SRLevelType::Resistance)];

        // Bar opens below resistance, closes above it
        let bar = make_bar(dec!(3.08), dec!(3.15), dec!(3.07), dec!(3.14));
        detector.update(&bar, &levels, TrendState::BullTrend);

        assert_eq!(detector.active_breakouts().len(), 1);
        assert_eq!(detector.active_breakouts()[0].direction, Direction::Long);
        assert!(matches!(
            detector.active_breakouts()[0].follow_through,
            FollowThrough::Pending
        ));
    }

    #[test]
    fn test_detect_downward_breakout() {
        let mut detector = BreakoutDetector::new(BreakoutConfig::default());
        let levels = vec![make_sr_level(dec!(3.00), SRLevelType::Support)];

        // Bar opens above support, closes below it
        let bar = make_bar(dec!(3.02), dec!(3.03), dec!(2.94), dec!(2.95));
        detector.update(&bar, &levels, TrendState::BearTrend);

        assert_eq!(detector.active_breakouts().len(), 1);
        assert_eq!(detector.active_breakouts()[0].direction, Direction::Short);
    }

    #[test]
    fn test_breakout_confirmation() {
        let config = BreakoutConfig {
            confirmation_bars: 2,
            ..Default::default()
        };
        let mut detector = BreakoutDetector::new(config);
        let levels = vec![make_sr_level(dec!(3.10), SRLevelType::Resistance)];

        // Breakout bar
        let bar1 = make_bar(dec!(3.08), dec!(3.15), dec!(3.07), dec!(3.14));
        detector.update(&bar1, &levels, TrendState::BullTrend);
        assert_eq!(detector.active_breakouts().len(), 1);

        // Follow-through bar 1 — still above level
        let bar2 = make_bar(dec!(3.14), dec!(3.18), dec!(3.12), dec!(3.16));
        detector.update(&bar2, &[], TrendState::BullTrend);

        // Follow-through bar 2 — confirmed after 2 bars
        let bar3 = make_bar(dec!(3.16), dec!(3.20), dec!(3.14), dec!(3.18));
        detector.update(&bar3, &[], TrendState::BullTrend);

        assert!(matches!(
            detector.active_breakouts()[0].follow_through,
            FollowThrough::Confirmed { .. }
        ));
    }

    #[test]
    fn test_failed_breakout() {
        let config = BreakoutConfig {
            confirmation_bars: 3,
            ..Default::default()
        };
        let mut detector = BreakoutDetector::new(config);
        let levels = vec![make_sr_level(dec!(3.10), SRLevelType::Resistance)];

        // Breakout bar
        let bar1 = make_bar(dec!(3.08), dec!(3.15), dec!(3.07), dec!(3.14));
        detector.update(&bar1, &levels, TrendState::BullTrend);

        // Reversal bar — closes back below the level
        let bar2 = make_bar(dec!(3.14), dec!(3.15), dec!(3.06), dec!(3.08));
        detector.update(&bar2, &[], TrendState::BullTrend);

        // Failed breakout should be moved to recent_failed
        assert_eq!(detector.active_breakouts().len(), 0);
        assert_eq!(detector.recent_failed_breakouts().len(), 1);
        assert!(matches!(
            detector.recent_failed_breakouts()[0].follow_through,
            FollowThrough::Failed { .. }
        ));
    }

    #[test]
    fn test_no_breakout_when_no_cross() {
        let mut detector = BreakoutDetector::new(BreakoutConfig::default());
        let levels = vec![make_sr_level(dec!(3.10), SRLevelType::Resistance)];

        // Bar stays below resistance
        let bar = make_bar(dec!(3.05), dec!(3.08), dec!(3.03), dec!(3.07));
        detector.update(&bar, &levels, TrendState::TradingRange);

        assert!(detector.active_breakouts().is_empty());
    }

    #[test]
    fn test_breakout_strength() {
        let mut detector = BreakoutDetector::new(BreakoutConfig::default());
        let levels = vec![make_sr_level(dec!(3.10), SRLevelType::Resistance)];

        // Strong breakout: close far beyond level
        // range = 3.20 - 3.08 = 0.12, close_beyond = 3.18 - 3.10 = 0.08, ratio = 0.67
        let bar = make_bar(dec!(3.08), dec!(3.20), dec!(3.08), dec!(3.18));
        detector.update(&bar, &levels, TrendState::BullTrend);

        assert_eq!(
            detector.active_breakouts()[0].strength,
            BreakoutStrength::Strong
        );
    }
}
