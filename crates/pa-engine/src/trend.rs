use brooks_core::bar::Bar;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Current trend state using Brooks PA classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendState {
    StrongBullTrend,
    BullTrend,
    WeakBullTrend,
    TradingRange,
    WeakBearTrend,
    BearTrend,
    StrongBearTrend,
}

impl TrendState {
    pub fn is_bull(&self) -> bool {
        matches!(
            self,
            TrendState::StrongBullTrend | TrendState::BullTrend | TrendState::WeakBullTrend
        )
    }

    pub fn is_bear(&self) -> bool {
        matches!(
            self,
            TrendState::StrongBearTrend | TrendState::BearTrend | TrendState::WeakBearTrend
        )
    }

    pub fn is_trending(&self) -> bool {
        !matches!(self, TrendState::TradingRange)
    }

    pub fn is_strong(&self) -> bool {
        matches!(
            self,
            TrendState::StrongBullTrend | TrendState::StrongBearTrend
        )
    }
}

/// A swing high or swing low point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwingPoint {
    pub price: Decimal,
    pub bar_index: usize,
    pub timestamp: DateTime<Utc>,
    pub point_type: SwingPointType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwingPointType {
    High,
    Low,
}

/// Configuration for the trend analyzer
#[derive(Debug, Clone)]
pub struct TrendConfig {
    /// Number of bars to look back for swing detection
    pub swing_lookback: usize,
    /// EMA period for trend reference
    pub ema_period: usize,
    /// Maximum bars to retain for analysis
    pub max_bars: usize,
    /// Maximum swing points to retain
    pub max_swing_points: usize,
}

impl Default for TrendConfig {
    fn default() -> Self {
        Self {
            swing_lookback: 5,
            ema_period: 20,
            max_bars: 200,
            max_swing_points: 50,
        }
    }
}

/// Manages trend detection using higher-highs/higher-lows logic
/// and exponential moving average position.
pub struct TrendAnalyzer {
    state: TrendState,
    swing_highs: Vec<SwingPoint>,
    swing_lows: Vec<SwingPoint>,
    ema: Option<Decimal>,
    ema_multiplier: Decimal,
    recent_bars: VecDeque<Bar>,
    bar_index: usize,
    config: TrendConfig,
}

impl TrendAnalyzer {
    pub fn new(config: TrendConfig) -> Self {
        let ema_multiplier =
            Decimal::TWO / (Decimal::from(config.ema_period as u64) + Decimal::ONE);
        Self {
            state: TrendState::TradingRange,
            swing_highs: Vec::new(),
            swing_lows: Vec::new(),
            ema: None,
            ema_multiplier,
            recent_bars: VecDeque::with_capacity(config.max_bars),
            bar_index: 0,
            config,
        }
    }

    /// Process a new bar and return the updated trend state
    pub fn update(&mut self, bar: &Bar) -> TrendState {
        // Update EMA
        self.update_ema(bar);

        // Store bar
        if self.recent_bars.len() >= self.config.max_bars {
            self.recent_bars.pop_front();
        }
        self.recent_bars.push_back(bar.clone());

        // Detect swing points
        self.detect_swing_points(bar);

        // Determine trend state
        self.state = self.determine_trend_state(bar);

        self.bar_index += 1;
        self.state
    }

    pub fn current_trend(&self) -> TrendState {
        self.state
    }

    pub fn is_trending(&self) -> bool {
        self.state.is_trending()
    }

    /// Trend strength as a value from 0.0 (no trend) to 1.0 (strong trend).
    /// Based on: consistency of swing pattern + EMA alignment + consecutive direction.
    pub fn trend_strength(&self) -> f64 {
        let mut strength = 0.0;

        // Factor 1: EMA alignment (is price above/below EMA consistently?)
        if let Some(ema) = self.ema {
            if let Some(last_bar) = self.recent_bars.back() {
                if (self.state.is_bull() && last_bar.close > ema)
                    || (self.state.is_bear() && last_bar.close < ema)
                {
                    strength += 0.3;
                }
            }
        }

        // Factor 2: Higher-highs/higher-lows or lower-highs/lower-lows pattern
        if self.has_consistent_swing_pattern() {
            strength += 0.4;
        }

        // Factor 3: Recent bars trending (last 5 bars mostly in same direction)
        let recent_trend_score = self.recent_bar_direction_score();
        strength += recent_trend_score * 0.3;

        strength.min(1.0)
    }

    pub fn last_swing_high(&self) -> Option<&SwingPoint> {
        self.swing_highs.last()
    }

    pub fn last_swing_low(&self) -> Option<&SwingPoint> {
        self.swing_lows.last()
    }

    pub fn swing_highs(&self) -> &[SwingPoint] {
        &self.swing_highs
    }

    pub fn swing_lows(&self) -> &[SwingPoint] {
        &self.swing_lows
    }

    pub fn ema(&self) -> Option<Decimal> {
        self.ema
    }

    pub fn bar_index(&self) -> usize {
        self.bar_index
    }

    fn update_ema(&mut self, bar: &Bar) {
        match self.ema {
            Some(prev_ema) => {
                // EMA = close * multiplier + prev_ema * (1 - multiplier)
                self.ema = Some(
                    bar.close * self.ema_multiplier
                        + prev_ema * (Decimal::ONE - self.ema_multiplier),
                );
            }
            None => {
                // Initialize EMA with first bar's close
                self.ema = Some(bar.close);
            }
        }
    }

    fn detect_swing_points(&mut self, _bar: &Bar) {
        let lookback = self.config.swing_lookback;
        let n = self.recent_bars.len();

        // We need at least 2*lookback+1 bars to confirm a swing
        if n < 2 * lookback + 1 {
            return;
        }

        // Check the bar at position (n - lookback - 1) — the candidate in the middle
        let candidate_idx = n - lookback - 1;
        let candidate = &self.recent_bars[candidate_idx];

        // Check if candidate is a swing high
        let mut is_swing_high = true;
        let mut is_swing_low = true;

        for i in 1..=lookback {
            // Bars before the candidate
            if candidate_idx >= i {
                let before = &self.recent_bars[candidate_idx - i];
                if before.high >= candidate.high {
                    is_swing_high = false;
                }
                if before.low <= candidate.low {
                    is_swing_low = false;
                }
            }
            // Bars after the candidate
            let after_idx = candidate_idx + i;
            if after_idx < n {
                let after = &self.recent_bars[after_idx];
                if after.high >= candidate.high {
                    is_swing_high = false;
                }
                if after.low <= candidate.low {
                    is_swing_low = false;
                }
            }
        }

        let swing_bar_index = self.bar_index - lookback;

        if is_swing_high {
            let sp = SwingPoint {
                price: candidate.high,
                bar_index: swing_bar_index,
                timestamp: candidate.timestamp,
                point_type: SwingPointType::High,
            };
            self.swing_highs.push(sp);
            if self.swing_highs.len() > self.config.max_swing_points {
                self.swing_highs.remove(0);
            }
        }

        if is_swing_low {
            let sp = SwingPoint {
                price: candidate.low,
                bar_index: swing_bar_index,
                timestamp: candidate.timestamp,
                point_type: SwingPointType::Low,
            };
            self.swing_lows.push(sp);
            if self.swing_lows.len() > self.config.max_swing_points {
                self.swing_lows.remove(0);
            }
        }
    }

    fn determine_trend_state(&self, bar: &Bar) -> TrendState {
        let hh_hl = self.has_higher_highs_higher_lows();
        let lh_ll = self.has_lower_highs_lower_lows();
        let above_ema = self.ema.is_some_and(|ema| bar.close > ema);
        let below_ema = self.ema.is_some_and(|ema| bar.close < ema);

        match (hh_hl, lh_ll, above_ema, below_ema) {
            // Strong bull: HH/HL pattern AND price above EMA
            (true, false, true, _) => {
                if self.recent_bar_direction_score() > 0.6 {
                    TrendState::StrongBullTrend
                } else {
                    TrendState::BullTrend
                }
            }
            // Bull trend: HH/HL pattern or above EMA
            (true, false, _, _) => TrendState::BullTrend,
            (false, false, true, _) if self.recent_bar_direction_score() > 0.4 => {
                TrendState::WeakBullTrend
            }
            // Strong bear: LH/LL pattern AND price below EMA
            (false, true, _, true) => {
                if self.recent_bar_direction_score() < -0.6 {
                    TrendState::StrongBearTrend
                } else {
                    TrendState::BearTrend
                }
            }
            // Bear trend: LH/LL pattern or below EMA
            (false, true, _, _) => TrendState::BearTrend,
            (false, false, _, true) if self.recent_bar_direction_score() < -0.4 => {
                TrendState::WeakBearTrend
            }
            // Everything else: trading range
            _ => TrendState::TradingRange,
        }
    }

    /// Check if the last two swing highs are higher and last two swing lows are higher
    fn has_higher_highs_higher_lows(&self) -> bool {
        if self.swing_highs.len() < 2 || self.swing_lows.len() < 2 {
            return false;
        }
        let sh = &self.swing_highs;
        let sl = &self.swing_lows;
        let hh = sh[sh.len() - 1].price > sh[sh.len() - 2].price;
        let hl = sl[sl.len() - 1].price > sl[sl.len() - 2].price;
        hh && hl
    }

    /// Check if the last two swing highs are lower and last two swing lows are lower
    fn has_lower_highs_lower_lows(&self) -> bool {
        if self.swing_highs.len() < 2 || self.swing_lows.len() < 2 {
            return false;
        }
        let sh = &self.swing_highs;
        let sl = &self.swing_lows;
        let lh = sh[sh.len() - 1].price < sh[sh.len() - 2].price;
        let ll = sl[sl.len() - 1].price < sl[sl.len() - 2].price;
        lh && ll
    }

    /// Check if swings follow a consistent pattern (at least 3 swing points in direction)
    fn has_consistent_swing_pattern(&self) -> bool {
        if self.state.is_bull() {
            // Check for at least 3 ascending swing lows
            if self.swing_lows.len() >= 3 {
                let n = self.swing_lows.len();
                return self.swing_lows[n - 1].price > self.swing_lows[n - 2].price
                    && self.swing_lows[n - 2].price > self.swing_lows[n - 3].price;
            }
        } else if self.state.is_bear() && self.swing_highs.len() >= 3 {
            let n = self.swing_highs.len();
            return self.swing_highs[n - 1].price < self.swing_highs[n - 2].price
                && self.swing_highs[n - 2].price < self.swing_highs[n - 3].price;
        }
        false
    }

    /// Score from -1.0 (all bear) to 1.0 (all bull) based on recent bar directions
    fn recent_bar_direction_score(&self) -> f64 {
        let count = 10.min(self.recent_bars.len());
        if count == 0 {
            return 0.0;
        }

        let mut bull = 0i32;
        let mut bear = 0i32;
        let start = self.recent_bars.len() - count;
        for i in start..self.recent_bars.len() {
            let b = &self.recent_bars[i];
            if b.is_bull() {
                bull += 1;
            } else if b.is_bear() {
                bear += 1;
            }
        }

        (bull - bear) as f64 / count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Exchange, SecurityId};
    use brooks_core::timeframe::Timeframe;
    use rust_decimal_macros::dec;

    fn make_bar_at(price_base: Decimal, is_bull: bool) -> Bar {
        let offset = dec!(0.05);
        let (open, close) = if is_bull {
            (price_base, price_base + offset)
        } else {
            (price_base + offset, price_base)
        };
        Bar {
            timestamp: Utc::now(),
            open,
            high: open.max(close) + dec!(0.01),
            low: open.min(close) - dec!(0.01),
            close,
            volume: 1000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    #[test]
    fn test_initial_state_is_trading_range() {
        let analyzer = TrendAnalyzer::new(TrendConfig::default());
        assert_eq!(analyzer.current_trend(), TrendState::TradingRange);
    }

    #[test]
    fn test_ema_initialized_on_first_bar() {
        let mut analyzer = TrendAnalyzer::new(TrendConfig::default());
        let bar = make_bar_at(dec!(3.00), true);
        analyzer.update(&bar);
        assert!(analyzer.ema().is_some());
    }

    #[test]
    fn test_bull_trend_detection() {
        let config = TrendConfig {
            swing_lookback: 2,
            ema_period: 5,
            max_bars: 100,
            max_swing_points: 20,
        };
        let mut analyzer = TrendAnalyzer::new(config);

        // Create an uptrend pattern: rising prices with some pullbacks
        let prices = [
            // Initial rise
            (dec!(3.00), true),
            (dec!(3.02), true),
            (dec!(3.04), true),
            (dec!(3.06), true),
            (dec!(3.08), true),
            // Small pullback
            (dec!(3.06), false),
            (dec!(3.05), false),
            // Higher rise
            (dec!(3.07), true),
            (dec!(3.09), true),
            (dec!(3.11), true),
            (dec!(3.13), true),
            (dec!(3.15), true),
            // Another pullback
            (dec!(3.13), false),
            (dec!(3.12), false),
            // Higher rise again
            (dec!(3.14), true),
            (dec!(3.16), true),
            (dec!(3.18), true),
            (dec!(3.20), true),
        ];

        let mut last_state = TrendState::TradingRange;
        for (price, is_bull) in &prices {
            let bar = make_bar_at(*price, *is_bull);
            last_state = analyzer.update(&bar);
        }

        assert!(
            last_state.is_bull(),
            "Expected bull trend, got: {last_state:?}"
        );
    }

    #[test]
    fn test_bear_trend_detection() {
        let config = TrendConfig {
            swing_lookback: 2,
            ema_period: 5,
            max_bars: 100,
            max_swing_points: 20,
        };
        let mut analyzer = TrendAnalyzer::new(config);

        // Create a downtrend pattern
        let prices = [
            (dec!(3.20), false),
            (dec!(3.18), false),
            (dec!(3.16), false),
            (dec!(3.14), false),
            (dec!(3.12), false),
            // Bounce
            (dec!(3.14), true),
            (dec!(3.15), true),
            // Lower drop
            (dec!(3.13), false),
            (dec!(3.11), false),
            (dec!(3.09), false),
            (dec!(3.07), false),
            (dec!(3.05), false),
            // Bounce
            (dec!(3.07), true),
            (dec!(3.08), true),
            // Lower again
            (dec!(3.06), false),
            (dec!(3.04), false),
            (dec!(3.02), false),
            (dec!(3.00), false),
        ];

        let mut last_state = TrendState::TradingRange;
        for (price, is_bull) in &prices {
            let bar = make_bar_at(*price, *is_bull);
            last_state = analyzer.update(&bar);
        }

        assert!(
            last_state.is_bear(),
            "Expected bear trend, got: {last_state:?}"
        );
    }

    #[test]
    fn test_trend_strength_bounds() {
        let mut analyzer = TrendAnalyzer::new(TrendConfig::default());
        let bar = make_bar_at(dec!(3.00), true);
        analyzer.update(&bar);
        let strength = analyzer.trend_strength();
        assert!((0.0..=1.0).contains(&strength));
    }

    #[test]
    fn test_swing_detection() {
        let config = TrendConfig {
            swing_lookback: 2,
            ema_period: 5,
            max_bars: 100,
            max_swing_points: 20,
        };
        let mut analyzer = TrendAnalyzer::new(config);

        // Create a clear swing high at bar 3 (index 2):
        // bars: up, up, UP (highest), down, down
        let bars_data = [
            (dec!(3.00), dec!(3.05), dec!(2.99), dec!(3.04)),
            (dec!(3.04), dec!(3.08), dec!(3.02), dec!(3.07)),
            (dec!(3.07), dec!(3.15), dec!(3.06), dec!(3.14)), // swing high candidate
            (dec!(3.14), dec!(3.13), dec!(3.05), dec!(3.06)),
            (dec!(3.06), dec!(3.07), dec!(2.98), dec!(3.00)),
        ];

        for (open, high, low, close) in &bars_data {
            let bar = Bar {
                timestamp: Utc::now(),
                open: *open,
                high: *high,
                low: *low,
                close: *close,
                volume: 1000,
                timeframe: Timeframe::Minute5,
                security: SecurityId::etf("510050", Exchange::SH),
            };
            analyzer.update(&bar);
        }

        assert!(
            !analyzer.swing_highs().is_empty(),
            "Should detect at least one swing high"
        );
    }
}
