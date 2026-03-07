use brooks_core::bar::Bar;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::trend::SwingPoint;

/// Type of support/resistance level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SRLevelType {
    Support,
    Resistance,
    PriorHighOfDay,
    PriorLowOfDay,
    SwingHigh,
    SwingLow,
    RoundNumber,
    MovingAverage,
}

/// A support or resistance level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SRLevel {
    pub price: Decimal,
    pub level_type: SRLevelType,
    /// Number of times price has touched this level
    pub strength: u32,
    pub first_touch: DateTime<Utc>,
    pub last_touch: DateTime<Utc>,
}

/// Detects and tracks support and resistance levels
pub struct SRDetector {
    levels: Vec<SRLevel>,
    /// Price tolerance for clustering nearby levels together
    price_cluster_tolerance: Decimal,
    /// Maximum number of levels to track
    max_levels: usize,
}

impl SRDetector {
    pub fn new(price_cluster_tolerance: Decimal, max_levels: usize) -> Self {
        Self {
            levels: Vec::new(),
            price_cluster_tolerance,
            max_levels,
        }
    }

    /// Update S/R levels based on a new bar and detected swing points
    pub fn update(&mut self, bar: &Bar, swing_points: &[SwingPoint]) {
        // Add swing highs as resistance levels
        for sp in swing_points {
            match sp.point_type {
                crate::trend::SwingPointType::High => {
                    self.add_or_strengthen(sp.price, SRLevelType::SwingHigh, sp.timestamp);
                }
                crate::trend::SwingPointType::Low => {
                    self.add_or_strengthen(sp.price, SRLevelType::SwingLow, sp.timestamp);
                }
            }
        }

        // Check if the bar touched any existing levels (strengthens them)
        self.check_touches(bar);

        // Prune old weak levels if we have too many
        self.prune_if_needed();
    }

    /// Add a level manually (e.g., prior high/low of day, round number)
    pub fn add_level(&mut self, price: Decimal, level_type: SRLevelType, timestamp: DateTime<Utc>) {
        self.add_or_strengthen(price, level_type, timestamp);
    }

    /// Find the nearest support level below the given price
    pub fn nearest_support(&self, price: Decimal) -> Option<&SRLevel> {
        self.levels
            .iter()
            .filter(|l| l.price < price)
            .max_by_key(|l| l.price)
    }

    /// Find the nearest resistance level above the given price
    pub fn nearest_resistance(&self, price: Decimal) -> Option<&SRLevel> {
        self.levels
            .iter()
            .filter(|l| l.price > price)
            .min_by_key(|l| l.price)
    }

    /// Find all levels within a price range
    pub fn levels_in_range(&self, low: Decimal, high: Decimal) -> Vec<&SRLevel> {
        self.levels
            .iter()
            .filter(|l| l.price >= low && l.price <= high)
            .collect()
    }

    /// Get all tracked levels
    pub fn all_levels(&self) -> &[SRLevel] {
        &self.levels
    }

    fn add_or_strengthen(
        &mut self,
        price: Decimal,
        level_type: SRLevelType,
        timestamp: DateTime<Utc>,
    ) {
        // Check if a nearby level already exists
        if let Some(existing) = self
            .levels
            .iter_mut()
            .find(|l| (l.price - price).abs() <= self.price_cluster_tolerance)
        {
            existing.strength += 1;
            existing.last_touch = timestamp;
            // Average the price for better accuracy
            existing.price = (existing.price + price) / Decimal::TWO;
        } else {
            self.levels.push(SRLevel {
                price,
                level_type,
                strength: 1,
                first_touch: timestamp,
                last_touch: timestamp,
            });
        }
    }

    fn check_touches(&mut self, bar: &Bar) {
        for level in &mut self.levels {
            // A bar "touches" a level if its range includes the level price
            if bar.low <= level.price && bar.high >= level.price {
                level.strength += 1;
                level.last_touch = bar.timestamp;
            }
        }
    }

    fn prune_if_needed(&mut self) {
        if self.levels.len() > self.max_levels {
            // Remove the weakest levels (lowest strength, oldest)
            self.levels.sort_by(|a, b| {
                b.strength
                    .cmp(&a.strength)
                    .then_with(|| b.last_touch.cmp(&a.last_touch))
            });
            self.levels.truncate(self.max_levels);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trend::SwingPointType;
    use brooks_core::market::{Exchange, SecurityId};
    use brooks_core::timeframe::Timeframe;
    use rust_decimal_macros::dec;

    fn make_bar(low: Decimal, high: Decimal) -> Bar {
        Bar {
            timestamp: Utc::now(),
            open: low + (high - low) / Decimal::TWO,
            high,
            low,
            close: low + (high - low) / Decimal::TWO,
            volume: 1000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    fn make_swing(price: Decimal, pt: SwingPointType) -> SwingPoint {
        SwingPoint {
            price,
            bar_index: 0,
            timestamp: Utc::now(),
            point_type: pt,
        }
    }

    #[test]
    fn test_add_swing_high_as_resistance() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        let bar = make_bar(dec!(3.00), dec!(3.10));
        let swings = vec![make_swing(dec!(3.10), SwingPointType::High)];
        detector.update(&bar, &swings);

        assert_eq!(detector.all_levels().len(), 1);
        assert_eq!(detector.all_levels()[0].level_type, SRLevelType::SwingHigh);
    }

    #[test]
    fn test_add_swing_low_as_support() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        let bar = make_bar(dec!(3.00), dec!(3.10));
        let swings = vec![make_swing(dec!(3.00), SwingPointType::Low)];
        detector.update(&bar, &swings);

        assert_eq!(detector.all_levels().len(), 1);
        assert_eq!(detector.all_levels()[0].level_type, SRLevelType::SwingLow);
    }

    #[test]
    fn test_cluster_nearby_levels() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        let bar = make_bar(dec!(3.00), dec!(3.10));

        // Two swing highs within tolerance of 0.005
        let swings1 = vec![make_swing(dec!(3.100), SwingPointType::High)];
        detector.update(&bar, &swings1);

        let swings2 = vec![make_swing(dec!(3.103), SwingPointType::High)];
        detector.update(&bar, &swings2);

        // Should cluster into one level with strength 3 (2 swings + 1 touch)
        assert_eq!(detector.all_levels().len(), 1);
        assert!(detector.all_levels()[0].strength >= 2);
    }

    #[test]
    fn test_nearest_support() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        detector.add_level(dec!(3.00), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.05), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.15), SRLevelType::Resistance, Utc::now());

        let support = detector.nearest_support(dec!(3.10));
        assert!(support.is_some());
        assert_eq!(support.unwrap().price, dec!(3.05));
    }

    #[test]
    fn test_nearest_resistance() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        detector.add_level(dec!(3.00), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.15), SRLevelType::Resistance, Utc::now());
        detector.add_level(dec!(3.25), SRLevelType::Resistance, Utc::now());

        let resistance = detector.nearest_resistance(dec!(3.10));
        assert!(resistance.is_some());
        assert_eq!(resistance.unwrap().price, dec!(3.15));
    }

    #[test]
    fn test_levels_in_range() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        detector.add_level(dec!(3.00), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.05), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.15), SRLevelType::Resistance, Utc::now());
        detector.add_level(dec!(3.25), SRLevelType::Resistance, Utc::now());

        let levels = detector.levels_in_range(dec!(3.04), dec!(3.16));
        assert_eq!(levels.len(), 2);
    }

    #[test]
    fn test_bar_touch_strengthens_level() {
        let mut detector = SRDetector::new(dec!(0.005), 50);
        detector.add_level(dec!(3.10), SRLevelType::Resistance, Utc::now());
        assert_eq!(detector.all_levels()[0].strength, 1);

        // Bar that touches the resistance level
        let bar = make_bar(dec!(3.05), dec!(3.12));
        detector.update(&bar, &[]);
        assert!(detector.all_levels()[0].strength > 1);
    }

    #[test]
    fn test_prune_keeps_strongest() {
        let mut detector = SRDetector::new(dec!(0.005), 3);

        // Add 4 levels, max is 3
        detector.add_level(dec!(3.00), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.10), SRLevelType::Support, Utc::now());
        detector.add_level(dec!(3.20), SRLevelType::Resistance, Utc::now());

        // Strengthen the first one
        let bar = make_bar(dec!(2.99), dec!(3.01));
        detector.update(&bar, &[]);

        // Add a 4th — should trigger pruning
        detector.add_level(dec!(3.30), SRLevelType::Resistance, Utc::now());
        detector.prune_if_needed();

        assert!(detector.all_levels().len() <= 3);
    }
}
