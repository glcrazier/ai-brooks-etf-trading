use brooks_core::market::Direction;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::trend::SwingPoint;

/// Type of channel pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelType {
    /// Rising channel (bull trend)
    BullChannel,
    /// Falling channel (bear trend)
    BearChannel,
    /// Wide channel, often a trading range on a higher timeframe
    BroadChannel,
    /// Converging channel — a wedge pattern (potential reversal)
    Wedge,
}

/// A trend line defined by two price points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendLine {
    pub start_price: Decimal,
    pub end_price: Decimal,
    pub start_index: usize,
    pub end_index: usize,
    /// Price change per bar
    pub slope: Decimal,
}

impl TrendLine {
    /// Create a trend line from two swing points
    pub fn from_points(
        p1_price: Decimal,
        p1_index: usize,
        p2_price: Decimal,
        p2_index: usize,
    ) -> Self {
        let bar_diff = if p2_index > p1_index {
            Decimal::from((p2_index - p1_index) as u64)
        } else {
            Decimal::ONE
        };

        let slope = (p2_price - p1_price) / bar_diff;

        Self {
            start_price: p1_price,
            end_price: p2_price,
            start_index: p1_index,
            end_index: p2_index,
            slope,
        }
    }

    /// Project the trend line value at a given bar index
    pub fn value_at(&self, bar_index: usize) -> Decimal {
        let offset = Decimal::from(bar_index.saturating_sub(self.start_index) as u64);
        self.start_price + self.slope * offset
    }

    /// Whether the slope is positive
    pub fn is_rising(&self) -> bool {
        self.slope > Decimal::ZERO
    }

    /// Whether the slope is negative
    pub fn is_falling(&self) -> bool {
        self.slope < Decimal::ZERO
    }
}

/// A detected channel (two parallel or converging trend lines)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub channel_type: ChannelType,
    pub direction: Direction,
    pub upper_line: TrendLine,
    pub lower_line: TrendLine,
    pub bar_count: u32,
}

impl Channel {
    /// Width of the channel at a given bar index
    pub fn width_at(&self, bar_index: usize) -> Decimal {
        let upper = self.upper_line.value_at(bar_index);
        let lower = self.lower_line.value_at(bar_index);
        upper - lower
    }

    /// Whether the channel is converging (wedge)
    pub fn is_converging(&self) -> bool {
        matches!(self.channel_type, ChannelType::Wedge)
    }
}

/// Configuration for channel detection
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// Minimum swing points needed on each side to form a channel
    pub min_touches: usize,
    /// Maximum slope difference ratio for parallel channel detection
    pub parallel_tolerance: Decimal,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            min_touches: 2,
            parallel_tolerance: Decimal::new(30, 2), // 0.30 = 30% slope difference allowed
        }
    }
}

/// Detects channels and wedges from swing points
pub struct ChannelDetector {
    current_channel: Option<Channel>,
    config: ChannelConfig,
}

impl ChannelDetector {
    pub fn new(config: ChannelConfig) -> Self {
        Self {
            current_channel: None,
            config,
        }
    }

    /// Update channel detection with new swing points
    pub fn update(&mut self, swing_highs: &[SwingPoint], swing_lows: &[SwingPoint]) {
        if swing_highs.len() < self.config.min_touches || swing_lows.len() < self.config.min_touches
        {
            return;
        }

        // Try to form a channel from the most recent swing highs and lows
        let n_h = swing_highs.len();
        let n_l = swing_lows.len();

        let upper_line = TrendLine::from_points(
            swing_highs[n_h - 2].price,
            swing_highs[n_h - 2].bar_index,
            swing_highs[n_h - 1].price,
            swing_highs[n_h - 1].bar_index,
        );

        let lower_line = TrendLine::from_points(
            swing_lows[n_l - 2].price,
            swing_lows[n_l - 2].bar_index,
            swing_lows[n_l - 1].price,
            swing_lows[n_l - 1].bar_index,
        );

        // Determine channel type based on slopes
        let channel_type = self.classify_channel(&upper_line, &lower_line);
        let direction = if upper_line.is_rising() && lower_line.is_rising() {
            Direction::Long
        } else if upper_line.is_falling() && lower_line.is_falling() {
            Direction::Short
        } else {
            // Mixed or flat — use the lower line direction
            if lower_line.is_rising() {
                Direction::Long
            } else {
                Direction::Short
            }
        };

        let bar_count = swing_highs[n_h - 1].bar_index.saturating_sub(
            swing_lows[n_l - 2]
                .bar_index
                .min(swing_highs[n_h - 2].bar_index),
        ) as u32;

        self.current_channel = Some(Channel {
            channel_type,
            direction,
            upper_line,
            lower_line,
            bar_count,
        });
    }

    pub fn current_channel(&self) -> Option<&Channel> {
        self.current_channel.as_ref()
    }

    /// Clear the current channel (e.g., when a breakout invalidates it)
    pub fn clear(&mut self) {
        self.current_channel = None;
    }

    fn classify_channel(&self, upper: &TrendLine, lower: &TrendLine) -> ChannelType {
        let upper_abs = upper.slope.abs();
        let lower_abs = lower.slope.abs();

        // Check for convergence (wedge): slopes point in opposite directions
        // or one slope is significantly steeper than the other towards the other
        let both_rising = upper.is_rising() && lower.is_rising();
        let both_falling = upper.is_falling() && lower.is_falling();

        if !both_rising && !both_falling {
            // Lines converging or diverging
            // If upper is falling while lower is rising (or vice versa), it's a wedge
            if (upper.is_falling() && lower.is_rising())
                || (upper.is_rising() && lower.is_falling() && upper.slope < lower.slope)
            {
                return ChannelType::Wedge;
            }
        }

        // Check if slopes are roughly parallel
        let max_slope = upper_abs.max(lower_abs);
        if !max_slope.is_zero() {
            let slope_diff = (upper_abs - lower_abs).abs() / max_slope;
            if slope_diff > self.config.parallel_tolerance {
                // Converging: it's a wedge
                return ChannelType::Wedge;
            }
        }

        // Parallel channel
        if both_rising {
            ChannelType::BullChannel
        } else if both_falling {
            ChannelType::BearChannel
        } else {
            ChannelType::BroadChannel
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trend::SwingPointType;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn swing_high(price: Decimal, idx: usize) -> SwingPoint {
        SwingPoint {
            price,
            bar_index: idx,
            timestamp: Utc::now(),
            point_type: SwingPointType::High,
        }
    }

    fn swing_low(price: Decimal, idx: usize) -> SwingPoint {
        SwingPoint {
            price,
            bar_index: idx,
            timestamp: Utc::now(),
            point_type: SwingPointType::Low,
        }
    }

    #[test]
    fn test_trend_line_from_points() {
        let tl = TrendLine::from_points(dec!(3.00), 0, dec!(3.10), 10);
        assert_eq!(tl.slope, dec!(0.01)); // 0.10 / 10
        assert!(tl.is_rising());
    }

    #[test]
    fn test_trend_line_value_at() {
        let tl = TrendLine::from_points(dec!(3.00), 0, dec!(3.10), 10);
        assert_eq!(tl.value_at(5), dec!(3.05));
        assert_eq!(tl.value_at(10), dec!(3.10));
    }

    #[test]
    fn test_bull_channel_detection() {
        let mut detector = ChannelDetector::new(ChannelConfig::default());

        let highs = vec![swing_high(dec!(3.10), 5), swing_high(dec!(3.20), 15)];
        let lows = vec![swing_low(dec!(3.00), 3), swing_low(dec!(3.10), 13)];

        detector.update(&highs, &lows);
        let channel = detector.current_channel().unwrap();

        assert_eq!(channel.channel_type, ChannelType::BullChannel);
        assert_eq!(channel.direction, Direction::Long);
    }

    #[test]
    fn test_bear_channel_detection() {
        let mut detector = ChannelDetector::new(ChannelConfig::default());

        let highs = vec![swing_high(dec!(3.20), 5), swing_high(dec!(3.10), 15)];
        let lows = vec![swing_low(dec!(3.10), 3), swing_low(dec!(3.00), 13)];

        detector.update(&highs, &lows);
        let channel = detector.current_channel().unwrap();

        assert_eq!(channel.channel_type, ChannelType::BearChannel);
        assert_eq!(channel.direction, Direction::Short);
    }

    #[test]
    fn test_wedge_detection() {
        let mut detector = ChannelDetector::new(ChannelConfig::default());

        // Upper line falling, lower line rising — converging
        let highs = vec![swing_high(dec!(3.20), 5), swing_high(dec!(3.15), 15)];
        let lows = vec![swing_low(dec!(3.00), 3), swing_low(dec!(3.05), 13)];

        detector.update(&highs, &lows);
        let channel = detector.current_channel().unwrap();

        assert_eq!(channel.channel_type, ChannelType::Wedge);
    }

    #[test]
    fn test_channel_width() {
        let channel = Channel {
            channel_type: ChannelType::BullChannel,
            direction: Direction::Long,
            upper_line: TrendLine::from_points(dec!(3.10), 0, dec!(3.20), 10),
            lower_line: TrendLine::from_points(dec!(3.00), 0, dec!(3.10), 10),
            bar_count: 10,
        };

        assert_eq!(channel.width_at(0), dec!(0.10));
        assert_eq!(channel.width_at(10), dec!(0.10));
    }

    #[test]
    fn test_not_enough_swings() {
        let mut detector = ChannelDetector::new(ChannelConfig::default());
        let highs = vec![swing_high(dec!(3.10), 5)]; // Only 1
        let lows = vec![swing_low(dec!(3.00), 3), swing_low(dec!(3.05), 10)];

        detector.update(&highs, &lows);
        assert!(detector.current_channel().is_none());
    }
}
