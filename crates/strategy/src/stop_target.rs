use brooks_core::market::Direction;
use brooks_pa_engine::context::MarketContext;
use brooks_pa_engine::entry_bar::EntryTrigger;
use brooks_pa_engine::trend::SwingPoint;
use rust_decimal::Decimal;

/// Calculated stop-loss and take-profit levels for a trade.
#[derive(Debug, Clone)]
pub struct StopTarget {
    pub stop_price: Decimal,
    pub target_price: Option<Decimal>,
    pub risk_per_unit: Decimal,
    pub reward_risk_ratio: Option<Decimal>,
}

/// Determines stop-loss and take-profit placement using signal bars,
/// S/R levels, swing points, and minimum reward:risk ratios.
pub struct StopTargetCalculator {
    min_reward_risk: Decimal,
}

impl StopTargetCalculator {
    pub fn new(min_reward_risk: Decimal) -> Self {
        Self { min_reward_risk }
    }

    /// Calculate stop and target for an entry trigger.
    ///
    /// **Stop placement**: uses the signal bar opposite extreme from the trigger.
    ///
    /// **Target placement priority**:
    /// 1. Next S/R level in the direction of the trade
    /// 2. Fallback: `entry_price + risk * min_rr_ratio` (or minus for shorts)
    pub fn calculate(
        &self,
        trigger: &EntryTrigger,
        direction: Direction,
        context: &MarketContext,
    ) -> StopTarget {
        let entry_price = trigger.entry_price;
        let stop_price = trigger.stop_price;
        let risk_per_unit = trigger.risk;

        // Find target from S/R levels
        let sr_target = match direction {
            Direction::Long => {
                // Look for nearest resistance above entry
                context
                    .nearest_resistance
                    .as_ref()
                    .map(|sr| sr.price)
                    .filter(|&p| p > entry_price)
            }
            Direction::Short => {
                // Look for nearest support below entry
                context
                    .nearest_support
                    .as_ref()
                    .map(|sr| sr.price)
                    .filter(|&p| p < entry_price)
            }
        };

        // If S/R target exists, use it; otherwise fallback to R:R-based target
        let target_price = sr_target.or_else(|| {
            if risk_per_unit.is_zero() {
                return None;
            }
            let rr_target = match direction {
                Direction::Long => entry_price + risk_per_unit * self.min_reward_risk,
                Direction::Short => entry_price - risk_per_unit * self.min_reward_risk,
            };
            Some(rr_target)
        });

        // Calculate actual R:R if we have a target
        let reward_risk_ratio = target_price.and_then(|target| {
            if risk_per_unit.is_zero() {
                return None;
            }
            let reward = match direction {
                Direction::Long => target - entry_price,
                Direction::Short => entry_price - target,
            };
            Some(reward / risk_per_unit)
        });

        StopTarget {
            stop_price,
            target_price,
            risk_per_unit,
            reward_risk_ratio,
        }
    }

    /// Calculate a trailing stop based on recent swing points.
    ///
    /// For longs: if a higher swing low exists above the current stop, move up.
    /// For shorts: if a lower swing high exists below the current stop, move down.
    /// Never moves the stop away from price (only toward breakeven or profit).
    ///
    /// Returns `Some(new_stop)` if the stop should be moved, `None` otherwise.
    pub fn trailing_stop(
        &self,
        direction: Direction,
        current_stop: Decimal,
        _entry_price: Decimal,
        swing_lows: &[SwingPoint],
        swing_highs: &[SwingPoint],
    ) -> Option<Decimal> {
        match direction {
            Direction::Long => {
                // Find the highest recent swing low above the current stop
                let best = swing_lows
                    .iter()
                    .filter(|sp| sp.price > current_stop)
                    .max_by(|a, b| a.price.cmp(&b.price));
                best.map(|sp| sp.price)
            }
            Direction::Short => {
                // Find the lowest recent swing high below the current stop
                let best = swing_highs
                    .iter()
                    .filter(|sp| sp.price < current_stop)
                    .min_by(|a, b| a.price.cmp(&b.price));
                best.map(|sp| sp.price)
            }
        }
    }

    /// Whether the trade meets the minimum reward:risk ratio requirement.
    pub fn meets_min_rr(&self, stop_target: &StopTarget) -> bool {
        match stop_target.reward_risk_ratio {
            Some(rr) => rr >= self.min_reward_risk,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_pa_engine::signal_bar::SignalQuality;
    use brooks_pa_engine::support_resistance::{SRLevel, SRLevelType};
    use brooks_pa_engine::trend::SwingPointType;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn make_trigger(entry: Decimal, stop: Decimal) -> EntryTrigger {
        let risk = (entry - stop).abs();
        EntryTrigger {
            entry_price: entry,
            stop_price: stop,
            risk,
            signal_quality: SignalQuality::Strong,
        }
    }

    fn empty_context() -> MarketContext {
        MarketContext::default()
    }

    fn context_with_resistance(price: Decimal) -> MarketContext {
        let mut ctx = MarketContext::default();
        ctx.nearest_resistance = Some(SRLevel {
            price,
            level_type: SRLevelType::Resistance,
            strength: 3,
            first_touch: Utc::now(),
            last_touch: Utc::now(),
        });
        ctx
    }

    fn context_with_support(price: Decimal) -> MarketContext {
        let mut ctx = MarketContext::default();
        ctx.nearest_support = Some(SRLevel {
            price,
            level_type: SRLevelType::Support,
            strength: 2,
            first_touch: Utc::now(),
            last_touch: Utc::now(),
        });
        ctx
    }

    #[test]
    fn test_long_entry_with_resistance_target() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let trigger = make_trigger(dec!(3.500), dec!(3.450)); // risk = 0.050
        let ctx = context_with_resistance(dec!(3.600));

        let st = calc.calculate(&trigger, Direction::Long, &ctx);
        assert_eq!(st.stop_price, dec!(3.450));
        assert_eq!(st.target_price, Some(dec!(3.600)));
        assert_eq!(st.risk_per_unit, dec!(0.050));
        // reward = 3.600 - 3.500 = 0.100, R:R = 0.100 / 0.050 = 2.0
        assert_eq!(st.reward_risk_ratio, Some(dec!(2.0)));
    }

    #[test]
    fn test_short_entry_with_support_target() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let trigger = make_trigger(dec!(3.500), dec!(3.560)); // risk = 0.060
        let ctx = context_with_support(dec!(3.400));

        let st = calc.calculate(&trigger, Direction::Short, &ctx);
        assert_eq!(st.stop_price, dec!(3.560));
        assert_eq!(st.target_price, Some(dec!(3.400)));
        assert_eq!(st.risk_per_unit, dec!(0.060));
        // reward = 3.500 - 3.400 = 0.100, R:R = 0.100 / 0.060 ≈ 1.666...
    }

    #[test]
    fn test_fallback_to_rr_target_when_no_sr() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let trigger = make_trigger(dec!(3.500), dec!(3.450)); // risk = 0.050
        let ctx = empty_context();

        let st = calc.calculate(&trigger, Direction::Long, &ctx);
        // target = 3.500 + 0.050 * 1.5 = 3.575
        assert_eq!(st.target_price, Some(dec!(3.575)));
        assert_eq!(st.reward_risk_ratio, Some(dec!(1.5)));
    }

    #[test]
    fn test_short_fallback_target() {
        let calc = StopTargetCalculator::new(dec!(2.0));
        let trigger = make_trigger(dec!(3.500), dec!(3.560)); // risk = 0.060
        let ctx = empty_context();

        let st = calc.calculate(&trigger, Direction::Short, &ctx);
        // target = 3.500 - 0.060 * 2.0 = 3.380
        assert_eq!(st.target_price, Some(dec!(3.380)));
        assert_eq!(st.reward_risk_ratio, Some(dec!(2.0)));
    }

    #[test]
    fn test_meets_min_rr() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let good = StopTarget {
            stop_price: dec!(3.450),
            target_price: Some(dec!(3.600)),
            risk_per_unit: dec!(0.050),
            reward_risk_ratio: Some(dec!(2.0)),
        };
        assert!(calc.meets_min_rr(&good));

        let bad = StopTarget {
            stop_price: dec!(3.450),
            target_price: Some(dec!(3.510)),
            risk_per_unit: dec!(0.050),
            reward_risk_ratio: Some(dec!(1.2)),
        };
        assert!(!calc.meets_min_rr(&bad));
    }

    #[test]
    fn test_trailing_stop_moves_up_for_long() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let swing_lows = vec![
            SwingPoint {
                price: dec!(3.460),
                bar_index: 10,
                timestamp: Utc::now(),
                point_type: SwingPointType::Low,
            },
            SwingPoint {
                price: dec!(3.480),
                bar_index: 15,
                timestamp: Utc::now(),
                point_type: SwingPointType::Low,
            },
        ];
        let new_stop = calc.trailing_stop(
            Direction::Long,
            dec!(3.450), // current stop
            dec!(3.500), // entry
            &swing_lows,
            &[],
        );
        // Should move to 3.480 (highest swing low above current stop)
        assert_eq!(new_stop, Some(dec!(3.480)));
    }

    #[test]
    fn test_trailing_stop_no_change_when_no_higher_swing() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let swing_lows = vec![SwingPoint {
            price: dec!(3.440), // below current stop
            bar_index: 10,
            timestamp: Utc::now(),
            point_type: SwingPointType::Low,
        }];
        let new_stop = calc.trailing_stop(
            Direction::Long,
            dec!(3.450),
            dec!(3.500),
            &swing_lows,
            &[],
        );
        assert_eq!(new_stop, None);
    }

    #[test]
    fn test_trailing_stop_short_moves_down() {
        let calc = StopTargetCalculator::new(dec!(1.5));
        let swing_highs = vec![SwingPoint {
            price: dec!(3.540), // below current stop of 3.560
            bar_index: 10,
            timestamp: Utc::now(),
            point_type: SwingPointType::High,
        }];
        let new_stop = calc.trailing_stop(
            Direction::Short,
            dec!(3.560), // current stop
            dec!(3.500), // entry
            &[],
            &swing_highs,
        );
        assert_eq!(new_stop, Some(dec!(3.540)));
    }
}
