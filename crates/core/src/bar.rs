use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::market::SecurityId;
use crate::timeframe::Timeframe;

/// Represents a single price bar (OHLCV candle)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub timestamp: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: u64,
    pub timeframe: Timeframe,
    pub security: SecurityId,
}

impl Bar {
    /// Size of the bar body (absolute difference between open and close)
    pub fn body_size(&self) -> Decimal {
        (self.close - self.open).abs()
    }

    /// Total range of the bar (high - low)
    pub fn range(&self) -> Decimal {
        self.high - self.low
    }

    /// Midpoint of the bar range
    pub fn midpoint(&self) -> Decimal {
        (self.high + self.low) / Decimal::TWO
    }

    /// Whether this is a bull (green) bar
    pub fn is_bull(&self) -> bool {
        self.close > self.open
    }

    /// Whether this is a bear (red) bar
    pub fn is_bear(&self) -> bool {
        self.close < self.open
    }

    /// Whether this is a doji bar (body smaller than threshold ratio of range)
    pub fn is_doji(&self, threshold: Decimal) -> bool {
        if self.range().is_zero() {
            return true;
        }
        self.body_size() / self.range() <= threshold
    }

    /// Upper tail (wick above the body)
    pub fn upper_tail(&self) -> Decimal {
        self.high - self.close.max(self.open)
    }

    /// Lower tail (wick below the body)
    pub fn lower_tail(&self) -> Decimal {
        self.close.min(self.open) - self.low
    }

    /// Body as a ratio of the total range (0.0 to 1.0)
    pub fn body_ratio(&self) -> Decimal {
        if self.range().is_zero() {
            return Decimal::ZERO;
        }
        self.body_size() / self.range()
    }

    /// Upper tail as a ratio of the total range
    pub fn upper_tail_ratio(&self) -> Decimal {
        if self.range().is_zero() {
            return Decimal::ZERO;
        }
        self.upper_tail() / self.range()
    }

    /// Lower tail as a ratio of the total range
    pub fn lower_tail_ratio(&self) -> Decimal {
        if self.range().is_zero() {
            return Decimal::ZERO;
        }
        self.lower_tail() / self.range()
    }

    /// Whether the close is in the upper half of the range
    pub fn closes_in_upper_half(&self) -> bool {
        self.close > self.midpoint()
    }

    /// Whether the close is in the lower half of the range
    pub fn closes_in_lower_half(&self) -> bool {
        self.close < self.midpoint()
    }

    /// Whether this bar's range completely contains the previous bar's range (outside bar)
    pub fn is_outside_bar(&self, prev: &Bar) -> bool {
        self.high > prev.high && self.low < prev.low
    }

    /// Whether this bar's range is completely within the previous bar's range (inside bar)
    pub fn is_inside_bar(&self, prev: &Bar) -> bool {
        self.high <= prev.high && self.low >= prev.low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Exchange;
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

    #[test]
    fn test_bull_bar() {
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(3.08));
        assert!(bar.is_bull());
        assert!(!bar.is_bear());
    }

    #[test]
    fn test_bear_bar() {
        let bar = make_bar(dec!(3.08), dec!(3.10), dec!(2.95), dec!(3.00));
        assert!(bar.is_bear());
        assert!(!bar.is_bull());
    }

    #[test]
    fn test_body_size() {
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(3.08));
        assert_eq!(bar.body_size(), dec!(0.08));
    }

    #[test]
    fn test_range() {
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(3.08));
        assert_eq!(bar.range(), dec!(0.15));
    }

    #[test]
    fn test_midpoint() {
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.05));
        assert_eq!(bar.midpoint(), dec!(3.00));
    }

    #[test]
    fn test_upper_tail_bull() {
        // Bull bar: open=3.00, close=3.08, high=3.10
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(3.08));
        assert_eq!(bar.upper_tail(), dec!(0.02));
    }

    #[test]
    fn test_lower_tail_bull() {
        // Bull bar: open=3.00, close=3.08, low=2.95
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.95), dec!(3.08));
        assert_eq!(bar.lower_tail(), dec!(0.05));
    }

    #[test]
    fn test_doji() {
        // Tiny body relative to range
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.01));
        assert!(bar.is_doji(dec!(0.1))); // body/range = 0.01/0.20 = 0.05 < 0.1
    }

    #[test]
    fn test_not_doji() {
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.08));
        assert!(!bar.is_doji(dec!(0.1))); // body/range = 0.08/0.20 = 0.40 > 0.1
    }

    #[test]
    fn test_inside_bar() {
        let prev = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.05));
        let curr = make_bar(dec!(3.02), dec!(3.08), dec!(2.92), dec!(3.06));
        assert!(curr.is_inside_bar(&prev));
        assert!(!curr.is_outside_bar(&prev));
    }

    #[test]
    fn test_outside_bar() {
        let prev = make_bar(dec!(3.02), dec!(3.08), dec!(2.92), dec!(3.06));
        let curr = make_bar(dec!(3.00), dec!(3.12), dec!(2.88), dec!(3.05));
        assert!(curr.is_outside_bar(&prev));
        assert!(!curr.is_inside_bar(&prev));
    }

    #[test]
    fn test_closes_in_upper_half() {
        let bar = make_bar(dec!(3.00), dec!(3.10), dec!(2.90), dec!(3.06));
        assert!(bar.closes_in_upper_half());
        assert!(!bar.closes_in_lower_half());
    }

    #[test]
    fn test_zero_range_bar() {
        let bar = make_bar(dec!(3.00), dec!(3.00), dec!(3.00), dec!(3.00));
        assert!(bar.is_doji(dec!(0.1)));
        assert_eq!(bar.body_ratio(), Decimal::ZERO);
        assert_eq!(bar.upper_tail_ratio(), Decimal::ZERO);
    }
}
