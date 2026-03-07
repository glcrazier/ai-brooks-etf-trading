use brooks_core::bar::Bar;
use brooks_core::market::Direction;
use brooks_core::signal::{SignalContext, SignalType};
use brooks_pa_engine::analyzer::PriceActionAnalyzer;
use brooks_pa_engine::context::MarketContext;
use brooks_pa_engine::entry_bar::EntryTrigger;
use brooks_pa_engine::signal_bar::SignalQuality;
use rust_decimal::Decimal;

use crate::stop_target::{StopTarget, StopTargetCalculator};

/// A signal candidate before risk checks are applied.
#[derive(Debug, Clone)]
pub struct SignalCandidate {
    pub direction: Direction,
    pub signal_type: SignalType,
    pub entry_price: Decimal,
    pub stop_target: StopTarget,
    pub confidence: f64,
    pub htf_context: SignalContext,
    pub trigger: EntryTrigger,
}

/// Converts PA engine output into trading signal candidates.
///
/// Implements the Brooks Price Action decision pipeline:
/// signal bar → entry bar → classify → stop/target → confidence → filter.
pub struct SignalGenerator {
    stop_target_calc: StopTargetCalculator,
    min_confidence: f64,
    /// Previous bar for entry bar checking
    prev_bar: Option<Bar>,
}

impl SignalGenerator {
    pub fn new(min_reward_risk: Decimal) -> Self {
        Self {
            stop_target_calc: StopTargetCalculator::new(min_reward_risk),
            min_confidence: 0.3,
            prev_bar: None,
        }
    }

    /// Evaluate the current bar against the PA context and generate signal candidates.
    ///
    /// Decision pipeline:
    /// 1. Check if previous bar was a signal bar
    /// 2. Check if current bar triggers an entry
    /// 3. Classify signal type from trend context
    /// 4. Calculate stop/target
    /// 5. Compute confidence score
    /// 6. Filter by minimum confidence and R:R
    pub fn evaluate(
        &mut self,
        bar: &Bar,
        context: &MarketContext,
        htf_context: Option<&MarketContext>,
        analyzer: &PriceActionAnalyzer,
    ) -> Vec<SignalCandidate> {
        let mut candidates = Vec::new();

        // Need at least 2 classifications (signal bar + entry bar)
        if context.bar_classifications.len() < 2 {
            self.prev_bar = Some(bar.clone());
            return candidates;
        }

        let prev_classification = &context.bar_classifications[context.bar_classifications.len() - 2];

        // Only proceed if previous bar was a signal bar
        if !prev_classification.is_signal_bar {
            self.prev_bar = Some(bar.clone());
            return candidates;
        }

        let Some(prev_bar) = &self.prev_bar else {
            self.prev_bar = Some(bar.clone());
            return candidates;
        };

        // Determine the signal bar's direction and quality
        let detector = analyzer.signal_bar_detector();

        // Check bull signal -> bull entry
        if prev_classification.bar_type.is_bull() {
            let quality = detector.bull_signal_quality(prev_bar, context);
            if let Some(trigger) =
                analyzer
                    .entry_bar_detector()
                    .check_bull_entry(bar, prev_bar, quality)
            {
                if let Some(candidate) = self.build_candidate(
                    Direction::Long,
                    trigger,
                    quality,
                    context,
                    htf_context,
                ) {
                    candidates.push(candidate);
                }
            }
        }

        // Check bear signal -> bear entry
        if prev_classification.bar_type.is_bear() {
            let quality = detector.bear_signal_quality(prev_bar, context);
            if let Some(trigger) =
                analyzer
                    .entry_bar_detector()
                    .check_bear_entry(bar, prev_bar, quality)
            {
                if let Some(candidate) = self.build_candidate(
                    Direction::Short,
                    trigger,
                    quality,
                    context,
                    htf_context,
                ) {
                    candidates.push(candidate);
                }
            }
        }

        self.prev_bar = Some(bar.clone());
        candidates
    }

    /// Reset state (for new trading day).
    pub fn reset(&mut self) {
        self.prev_bar = None;
    }

    /// Build a candidate if it meets minimum confidence and R:R requirements.
    fn build_candidate(
        &self,
        direction: Direction,
        trigger: EntryTrigger,
        quality: SignalQuality,
        context: &MarketContext,
        htf_context: Option<&MarketContext>,
    ) -> Option<SignalCandidate> {
        let signal_type = classify_signal_type(direction, context);
        let stop_target =
            self.stop_target_calc
                .calculate(&trigger, direction, context);

        // Check minimum R:R
        if !self.stop_target_calc.meets_min_rr(&stop_target) {
            return None;
        }

        let confidence = compute_confidence(quality, &signal_type, context, htf_context);

        // Check minimum confidence
        if confidence < self.min_confidence {
            return None;
        }

        let htf_ctx = build_htf_signal_context(htf_context);

        Some(SignalCandidate {
            direction,
            signal_type,
            entry_price: trigger.entry_price,
            stop_target,
            confidence,
            htf_context: htf_ctx,
            trigger,
        })
    }
}

/// Classify the signal type based on trend state and market context.
fn classify_signal_type(direction: Direction, context: &MarketContext) -> SignalType {
    let trend = &context.trend;
    let is_with_trend = match direction {
        Direction::Long => trend.is_bull(),
        Direction::Short => trend.is_bear(),
    };

    // Reversal: climax + counter-trend signal
    if context.is_climax && !is_with_trend {
        return SignalType::TrendReversal;
    }

    // Counter-trend swing at S/R
    if !is_with_trend {
        let at_sr = match direction {
            Direction::Long => context.near_support(),
            Direction::Short => context.near_resistance(),
        };
        if at_sr {
            return SignalType::CounterTrendSwing;
        }
    }

    // Trading range breakout
    if context.in_trading_range && !context.active_breakouts.is_empty() {
        return SignalType::TradingRangeBreakout;
    }

    // Failed breakout (breakout + reversal)
    if !context.active_breakouts.is_empty() && context.bar_classifications.back().is_some_and(|c| c.is_reversal_bar) {
        return SignalType::FailedBreakoutEntry;
    }

    if is_with_trend && trend.is_trending() {
        // Strong trend with pullback pattern
        if trend.is_strong() {
            return SignalType::WithTrendScalp;
        }

        // Pullback in moderate trend
        let had_pullback = match direction {
            Direction::Long => context.consecutive_bear_bars >= 1,
            Direction::Short => context.consecutive_bull_bars >= 1,
        };
        if had_pullback {
            return SignalType::PullbackEntry;
        }

        return SignalType::TrendContinuation;
    }

    // Default
    SignalType::BreakoutEntry
}

/// Compute a confidence score for a signal candidate.
///
/// Starts at 0.5 and adjusts based on quality, context, and HTF alignment.
fn compute_confidence(
    quality: SignalQuality,
    signal_type: &SignalType,
    context: &MarketContext,
    htf_context: Option<&MarketContext>,
) -> f64 {
    let mut confidence: f64 = 0.5;

    // Signal quality
    match quality {
        SignalQuality::Strong => confidence += 0.15,
        SignalQuality::Moderate => confidence += 0.05,
        SignalQuality::Weak => {}
    }

    // HTF alignment
    if let Some(htf) = htf_context {
        let primary_bull = context.trend.is_bull();
        let htf_bull = htf.trend.is_bull();
        if primary_bull == htf_bull {
            confidence += 0.10;
        }
    }

    // Near S/R level (higher probability setups)
    if context.near_support() || context.near_resistance() {
        confidence += 0.05;
    }

    // Strong trend (trend_strength > 0.7)
    if context.trend_strength > 0.7 {
        confidence += 0.10;
    }

    // Climax exhaustion risk
    if context.is_climax {
        confidence -= 0.10;
    }

    // Counter-trend penalty
    if matches!(
        signal_type,
        SignalType::CounterTrendSwing | SignalType::TrendReversal
    ) {
        confidence -= 0.15;
    }

    confidence.clamp(0.0, 1.0)
}

/// Build the HTF signal context for the final Signal.
fn build_htf_signal_context(htf_context: Option<&MarketContext>) -> SignalContext {
    match htf_context {
        Some(htf) => {
            let htf_trend_direction = if htf.trend.is_bull() {
                Some(Direction::Long)
            } else if htf.trend.is_bear() {
                Some(Direction::Short)
            } else {
                None
            };

            let htf_aligned = htf_trend_direction.is_some();

            let mut htf_key_levels = Vec::new();
            if let Some(sr) = &htf.nearest_support {
                htf_key_levels.push(sr.price);
            }
            if let Some(sr) = &htf.nearest_resistance {
                htf_key_levels.push(sr.price);
            }

            SignalContext {
                htf_trend_direction,
                htf_aligned,
                htf_key_levels,
                description: format!("HTF trend: {:?}", htf.trend),
            }
        }
        None => SignalContext {
            htf_trend_direction: None,
            htf_aligned: false,
            htf_key_levels: Vec::new(),
            description: "No HTF context".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_pa_engine::bar_analysis::{BarClassification, BarType, BodySize, TailAnalysis};
    use brooks_pa_engine::trend::TrendState;
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;

    fn default_tail() -> TailAnalysis {
        TailAnalysis {
            upper_tail_ratio: dec!(0.1),
            lower_tail_ratio: dec!(0.1),
            has_prominent_upper_tail: false,
            has_prominent_lower_tail: false,
        }
    }

    fn non_signal_classification() -> BarClassification {
        BarClassification {
            bar_type: BarType::Doji,
            body_relative_size: BodySize::Doji,
            tail_analysis: default_tail(),
            is_signal_bar: false,
            is_entry_bar: false,
            is_reversal_bar: false,
            is_inside_bar: false,
            is_outside_bar: false,
            gap: None,
        }
    }

    #[test]
    fn test_no_signal_when_prev_not_signal_bar() {
        let mut gen = SignalGenerator::new(dec!(1.5));
        let ctx = {
            let mut c = MarketContext::default();
            c.bar_classifications = VecDeque::from(vec![
                non_signal_classification(),
                non_signal_classification(),
            ]);
            c
        };
        let bar = test_bar(dec!(3.510));
        let analyzer = PriceActionAnalyzer::new(Default::default());
        let candidates = gen.evaluate(&bar, &ctx, None, &analyzer);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_classify_pullback_entry_in_bull_trend() {
        let mut ctx = MarketContext::default();
        ctx.trend = TrendState::BullTrend;
        ctx.consecutive_bear_bars = 2; // pullback
        let signal_type = classify_signal_type(Direction::Long, &ctx);
        assert_eq!(signal_type, SignalType::PullbackEntry);
    }

    #[test]
    fn test_classify_trend_continuation() {
        let mut ctx = MarketContext::default();
        ctx.trend = TrendState::BullTrend;
        ctx.consecutive_bear_bars = 0;
        let signal_type = classify_signal_type(Direction::Long, &ctx);
        assert_eq!(signal_type, SignalType::TrendContinuation);
    }

    #[test]
    fn test_classify_with_trend_scalp_in_strong_trend() {
        let mut ctx = MarketContext::default();
        ctx.trend = TrendState::StrongBullTrend;
        let signal_type = classify_signal_type(Direction::Long, &ctx);
        assert_eq!(signal_type, SignalType::WithTrendScalp);
    }

    #[test]
    fn test_classify_trend_reversal_on_climax() {
        let mut ctx = MarketContext::default();
        ctx.trend = TrendState::StrongBullTrend;
        ctx.is_climax = true;
        // Short during bull climax = reversal
        let signal_type = classify_signal_type(Direction::Short, &ctx);
        assert_eq!(signal_type, SignalType::TrendReversal);
    }

    #[test]
    fn test_classify_counter_trend_swing_at_support() {
        let mut ctx = MarketContext::default();
        ctx.trend = TrendState::BearTrend;
        ctx.nearest_support = Some(brooks_pa_engine::support_resistance::SRLevel {
            price: dec!(3.400),
            level_type: brooks_pa_engine::support_resistance::SRLevelType::Support,
            strength: 3,
            first_touch: chrono::Utc::now(),
            last_touch: chrono::Utc::now(),
        });
        // Long against bear trend at support
        let signal_type = classify_signal_type(Direction::Long, &ctx);
        assert_eq!(signal_type, SignalType::CounterTrendSwing);
    }

    #[test]
    fn test_confidence_strong_quality_boost() {
        let ctx = MarketContext::default();
        let c = compute_confidence(SignalQuality::Strong, &SignalType::PullbackEntry, &ctx, None);
        // 0.5 + 0.15 = 0.65
        assert!((c - 0.65).abs() < 0.001);
    }

    #[test]
    fn test_confidence_htf_alignment_boost() {
        let mut ctx = MarketContext::default();
        ctx.trend = TrendState::BullTrend;
        let mut htf = MarketContext::default();
        htf.trend = TrendState::BullTrend;
        let c = compute_confidence(
            SignalQuality::Moderate,
            &SignalType::PullbackEntry,
            &ctx,
            Some(&htf),
        );
        // 0.5 + 0.05 (moderate) + 0.10 (htf aligned) = 0.65
        assert!((c - 0.65).abs() < 0.001);
    }

    #[test]
    fn test_confidence_counter_trend_penalty() {
        let ctx = MarketContext::default();
        let c = compute_confidence(
            SignalQuality::Moderate,
            &SignalType::CounterTrendSwing,
            &ctx,
            None,
        );
        // 0.5 + 0.05 - 0.15 = 0.40
        assert!((c - 0.40).abs() < 0.001);
    }

    #[test]
    fn test_confidence_clamped_to_zero_one() {
        let mut ctx = MarketContext::default();
        ctx.is_climax = true;
        let c = compute_confidence(
            SignalQuality::Weak,
            &SignalType::CounterTrendSwing,
            &ctx,
            None,
        );
        // 0.5 + 0 - 0.10 - 0.15 = 0.25
        assert!(c >= 0.0 && c <= 1.0);
    }

    #[test]
    fn test_htf_signal_context_with_htf() {
        let mut htf = MarketContext::default();
        htf.trend = TrendState::BullTrend;
        let sc = build_htf_signal_context(Some(&htf));
        assert_eq!(sc.htf_trend_direction, Some(Direction::Long));
        assert!(sc.htf_aligned);
    }

    #[test]
    fn test_htf_signal_context_without_htf() {
        let sc = build_htf_signal_context(None);
        assert_eq!(sc.htf_trend_direction, None);
        assert!(!sc.htf_aligned);
    }

    fn test_bar(close: Decimal) -> Bar {
        use brooks_core::market::{Exchange, SecurityId};
        use brooks_core::timeframe::Timeframe;
        use chrono::Utc;

        Bar {
            timestamp: Utc::now(),
            open: close - dec!(0.010),
            high: close + dec!(0.010),
            low: close - dec!(0.020),
            close,
            volume: 10000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }
}
