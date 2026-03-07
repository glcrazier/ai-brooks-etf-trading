use brooks_core::market::Direction;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::trend::SwingPoint;

/// A price leg — a directional move between two swing points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLeg {
    pub start_price: Decimal,
    pub end_price: Decimal,
    pub start_index: usize,
    pub end_index: usize,
    pub bar_count: u32,
    pub direction: Direction,
}

impl PriceLeg {
    /// Magnitude of the leg (absolute price change)
    pub fn magnitude(&self) -> Decimal {
        (self.end_price - self.start_price).abs()
    }
}

/// Recognized price action patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PricePattern {
    /// Two-leg move: leg1 → pullback → leg2 projected from pullback end
    MeasuredMove {
        leg1_start: Decimal,
        leg1_end: Decimal,
        pullback_end: Decimal,
        projected_target: Decimal,
    },
    /// Two-legged pullback within a trend
    TwoLeggedPullback {
        first_leg: PriceLeg,
        second_leg: PriceLeg,
        direction: Direction,
    },
    /// Two swing highs at approximately the same level
    DoubleTop {
        first_high: Decimal,
        second_high: Decimal,
    },
    /// Two swing lows at approximately the same level
    DoubleBottom {
        first_low: Decimal,
        second_low: Decimal,
    },
    /// Converging highs and lows (wedge/triangle)
    Wedge {
        highs: Vec<Decimal>,
        lows: Vec<Decimal>,
        direction: Direction,
    },
}

/// Configuration for pattern detection
#[derive(Debug, Clone)]
pub struct PatternConfig {
    /// Tolerance for double top/bottom matching (as fraction of price)
    pub double_level_tolerance: Decimal,
    /// Minimum leg ratio for measured move (leg2/leg1)
    pub measured_move_min_ratio: Decimal,
    /// Maximum leg ratio for measured move
    pub measured_move_max_ratio: Decimal,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            double_level_tolerance: dec!(0.003), // 0.3%
            measured_move_min_ratio: dec!(0.60),
            measured_move_max_ratio: dec!(1.40),
        }
    }
}

/// Detects Brooks PA patterns from swing points
pub struct PatternDetector {
    detected_patterns: Vec<PricePattern>,
    config: PatternConfig,
    /// Maximum patterns to retain
    max_patterns: usize,
}

impl PatternDetector {
    pub fn new(config: PatternConfig) -> Self {
        Self {
            detected_patterns: Vec::new(),
            config,
            max_patterns: 20,
        }
    }

    /// Update pattern detection with current swing points
    pub fn update(&mut self, swing_highs: &[SwingPoint], swing_lows: &[SwingPoint]) {
        self.detected_patterns.clear();

        self.detect_double_tops(swing_highs);
        self.detect_double_bottoms(swing_lows);
        self.detect_measured_moves(swing_highs, swing_lows);
        self.detect_two_legged_pullbacks(swing_highs, swing_lows);

        if self.detected_patterns.len() > self.max_patterns {
            self.detected_patterns.truncate(self.max_patterns);
        }
    }

    pub fn patterns(&self) -> &[PricePattern] {
        &self.detected_patterns
    }

    fn detect_double_tops(&mut self, swing_highs: &[SwingPoint]) {
        if swing_highs.len() < 2 {
            return;
        }

        let n = swing_highs.len();
        let h1 = &swing_highs[n - 2];
        let h2 = &swing_highs[n - 1];

        let avg = (h1.price + h2.price) / Decimal::TWO;
        if avg.is_zero() {
            return;
        }

        let diff_ratio = (h1.price - h2.price).abs() / avg;
        if diff_ratio <= self.config.double_level_tolerance {
            self.detected_patterns.push(PricePattern::DoubleTop {
                first_high: h1.price,
                second_high: h2.price,
            });
        }
    }

    fn detect_double_bottoms(&mut self, swing_lows: &[SwingPoint]) {
        if swing_lows.len() < 2 {
            return;
        }

        let n = swing_lows.len();
        let l1 = &swing_lows[n - 2];
        let l2 = &swing_lows[n - 1];

        let avg = (l1.price + l2.price) / Decimal::TWO;
        if avg.is_zero() {
            return;
        }

        let diff_ratio = (l1.price - l2.price).abs() / avg;
        if diff_ratio <= self.config.double_level_tolerance {
            self.detected_patterns.push(PricePattern::DoubleBottom {
                first_low: l1.price,
                second_low: l2.price,
            });
        }
    }

    fn detect_measured_moves(&mut self, swing_highs: &[SwingPoint], swing_lows: &[SwingPoint]) {
        // Bull measured move: Low1 → High1 → Low2 → projected High2
        // where High2 - Low2 ≈ High1 - Low1
        if !swing_highs.is_empty() && swing_lows.len() >= 2 {
            let n_l = swing_lows.len();
            let n_h = swing_highs.len();

            let low1 = &swing_lows[n_l - 2];
            let high1 = &swing_highs[n_h - 1];
            let low2 = &swing_lows[n_l - 1];

            // Ensure chronological order: low1 → high1 → low2
            if low1.bar_index < high1.bar_index && high1.bar_index < low2.bar_index {
                let leg1 = high1.price - low1.price;
                if leg1 > Decimal::ZERO {
                    let projected = low2.price + leg1;
                    self.detected_patterns.push(PricePattern::MeasuredMove {
                        leg1_start: low1.price,
                        leg1_end: high1.price,
                        pullback_end: low2.price,
                        projected_target: projected,
                    });
                }
            }
        }

        // Bear measured move: High1 → Low1 → High2 → projected Low2
        if !swing_lows.is_empty() && swing_highs.len() >= 2 {
            let n_l = swing_lows.len();
            let n_h = swing_highs.len();

            let high1 = &swing_highs[n_h - 2];
            let low1 = &swing_lows[n_l - 1];
            let high2 = &swing_highs[n_h - 1];

            if high1.bar_index < low1.bar_index && low1.bar_index < high2.bar_index {
                let leg1 = high1.price - low1.price;
                if leg1 > Decimal::ZERO {
                    let projected = high2.price - leg1;
                    self.detected_patterns.push(PricePattern::MeasuredMove {
                        leg1_start: high1.price,
                        leg1_end: low1.price,
                        pullback_end: high2.price,
                        projected_target: projected,
                    });
                }
            }
        }
    }

    fn detect_two_legged_pullbacks(
        &mut self,
        swing_highs: &[SwingPoint],
        swing_lows: &[SwingPoint],
    ) {
        // Two-legged bull pullback within a bear move:
        // In a bull trend, a pullback has two down legs separated by a bounce
        // Pattern: High → Low1 → bounce_high → Low2 (lower low or similar low)
        if swing_highs.len() >= 2 && swing_lows.len() >= 2 {
            let n_h = swing_highs.len();
            let n_l = swing_lows.len();

            let h1 = &swing_highs[n_h - 2];
            let l1 = &swing_lows[n_l - 2];
            let h2 = &swing_highs[n_h - 1];
            let l2 = &swing_lows[n_l - 1];

            // Bear two-legged pullback (in bull trend): h1 → l1 → h2 → l2
            if h1.bar_index < l1.bar_index
                && l1.bar_index < h2.bar_index
                && h2.bar_index < l2.bar_index
                && h2.price < h1.price
            {
                let first_leg = PriceLeg {
                    start_price: h1.price,
                    end_price: l1.price,
                    start_index: h1.bar_index,
                    end_index: l1.bar_index,
                    bar_count: (l1.bar_index - h1.bar_index) as u32,
                    direction: Direction::Short,
                };
                let second_leg = PriceLeg {
                    start_price: h2.price,
                    end_price: l2.price,
                    start_index: h2.bar_index,
                    end_index: l2.bar_index,
                    bar_count: (l2.bar_index - h2.bar_index) as u32,
                    direction: Direction::Short,
                };

                self.detected_patterns
                    .push(PricePattern::TwoLeggedPullback {
                        first_leg,
                        second_leg,
                        direction: Direction::Short,
                    });
            }

            // Bull two-legged pullback (in bear trend): l1 → h1 → l2 → h2
            let l1 = &swing_lows[n_l - 2];
            let h1 = &swing_highs[n_h - 2];
            let l2 = &swing_lows[n_l - 1];
            let h2 = &swing_highs[n_h - 1];

            if l1.bar_index < h1.bar_index
                && h1.bar_index < l2.bar_index
                && l2.bar_index < h2.bar_index
                && l2.price > l1.price
            {
                let first_leg = PriceLeg {
                    start_price: l1.price,
                    end_price: h1.price,
                    start_index: l1.bar_index,
                    end_index: h1.bar_index,
                    bar_count: (h1.bar_index - l1.bar_index) as u32,
                    direction: Direction::Long,
                };
                let second_leg = PriceLeg {
                    start_price: l2.price,
                    end_price: h2.price,
                    start_index: l2.bar_index,
                    end_index: h2.bar_index,
                    bar_count: (h2.bar_index - l2.bar_index) as u32,
                    direction: Direction::Long,
                };

                self.detected_patterns
                    .push(PricePattern::TwoLeggedPullback {
                        first_leg,
                        second_leg,
                        direction: Direction::Long,
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trend::SwingPointType;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn sh(price: Decimal, idx: usize) -> SwingPoint {
        SwingPoint {
            price,
            bar_index: idx,
            timestamp: Utc::now(),
            point_type: SwingPointType::High,
        }
    }

    fn sl(price: Decimal, idx: usize) -> SwingPoint {
        SwingPoint {
            price,
            bar_index: idx,
            timestamp: Utc::now(),
            point_type: SwingPointType::Low,
        }
    }

    #[test]
    fn test_double_top() {
        let mut detector = PatternDetector::new(PatternConfig::default());
        let highs = vec![sh(dec!(3.100), 5), sh(dec!(3.102), 15)]; // within 0.3% tolerance
        let lows = vec![sl(dec!(3.00), 3), sl(dec!(3.00), 13)];

        detector.update(&highs, &lows);

        let has_dt = detector
            .patterns()
            .iter()
            .any(|p| matches!(p, PricePattern::DoubleTop { .. }));
        assert!(has_dt);
    }

    #[test]
    fn test_no_double_top_when_far_apart() {
        let mut detector = PatternDetector::new(PatternConfig::default());
        let highs = vec![sh(dec!(3.100), 5), sh(dec!(3.200), 15)]; // too far apart
        let lows = vec![];

        detector.update(&highs, &lows);

        let has_dt = detector
            .patterns()
            .iter()
            .any(|p| matches!(p, PricePattern::DoubleTop { .. }));
        assert!(!has_dt);
    }

    #[test]
    fn test_double_bottom() {
        let mut detector = PatternDetector::new(PatternConfig::default());
        let highs = vec![];
        let lows = vec![sl(dec!(3.000), 5), sl(dec!(3.002), 15)];

        detector.update(&highs, &lows);

        let has_db = detector
            .patterns()
            .iter()
            .any(|p| matches!(p, PricePattern::DoubleBottom { .. }));
        assert!(has_db);
    }

    #[test]
    fn test_measured_move_bull() {
        let mut detector = PatternDetector::new(PatternConfig::default());
        // Low1=3.00@idx2, High1=3.10@idx5, Low2=3.05@idx8
        // Projected = 3.05 + (3.10 - 3.00) = 3.15
        let highs = vec![sh(dec!(3.10), 5)];
        let lows = vec![sl(dec!(3.00), 2), sl(dec!(3.05), 8)];

        detector.update(&highs, &lows);

        let has_mm = detector.patterns().iter().any(|p| {
            matches!(
                p,
                PricePattern::MeasuredMove {
                    projected_target,
                    ..
                } if *projected_target == dec!(3.15)
            )
        });
        assert!(has_mm);
    }

    #[test]
    fn test_price_leg_magnitude() {
        let leg = PriceLeg {
            start_price: dec!(3.00),
            end_price: dec!(3.10),
            start_index: 0,
            end_index: 5,
            bar_count: 5,
            direction: Direction::Long,
        };
        assert_eq!(leg.magnitude(), dec!(0.10));
    }
}
