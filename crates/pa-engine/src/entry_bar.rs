use brooks_core::bar::Bar;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::signal_bar::SignalQuality;

/// An entry trigger generated when an entry bar confirms a signal bar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryTrigger {
    /// Price at which to enter (typically the signal bar's high/low ± 1 tick)
    pub entry_price: Decimal,
    /// Stop loss price (typically the opposite end of the signal bar)
    pub stop_price: Decimal,
    /// Risk per unit (distance from entry to stop)
    pub risk: Decimal,
    /// Quality rating inherited from the signal bar
    pub signal_quality: SignalQuality,
}

/// Detects entry bars that confirm signal bars.
///
/// In Brooks PA:
/// - Bull entry: the bar AFTER a bull signal bar goes above the signal bar's high
/// - Bear entry: the bar AFTER a bear signal bar goes below the signal bar's low
pub struct EntryBarDetector;

impl EntryBarDetector {
    pub fn new() -> Self {
        Self
    }

    /// Check if the current bar triggers a bull entry based on the preceding signal bar.
    ///
    /// A bull entry occurs when the current bar trades above the signal bar's high.
    /// Entry price = signal bar's high (buy stop order level).
    /// Stop loss = signal bar's low.
    pub fn check_bull_entry(
        &self,
        current: &Bar,
        signal_bar: &Bar,
        quality: SignalQuality,
    ) -> Option<EntryTrigger> {
        // Current bar must go above the signal bar's high
        if current.high > signal_bar.high {
            let entry_price = signal_bar.high;
            let stop_price = signal_bar.low;
            let risk = entry_price - stop_price;

            if risk > Decimal::ZERO {
                return Some(EntryTrigger {
                    entry_price,
                    stop_price,
                    risk,
                    signal_quality: quality,
                });
            }
        }
        None
    }

    /// Check if the current bar triggers a bear entry based on the preceding signal bar.
    ///
    /// A bear entry occurs when the current bar trades below the signal bar's low.
    /// Entry price = signal bar's low (sell stop order level).
    /// Stop loss = signal bar's high.
    pub fn check_bear_entry(
        &self,
        current: &Bar,
        signal_bar: &Bar,
        quality: SignalQuality,
    ) -> Option<EntryTrigger> {
        // Current bar must go below the signal bar's low
        if current.low < signal_bar.low {
            let entry_price = signal_bar.low;
            let stop_price = signal_bar.high;
            let risk = stop_price - entry_price;

            if risk > Decimal::ZERO {
                return Some(EntryTrigger {
                    entry_price,
                    stop_price,
                    risk,
                    signal_quality: quality,
                });
            }
        }
        None
    }
}

impl Default for EntryBarDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn test_bull_entry_triggered() {
        let detector = EntryBarDetector::new();
        let signal_bar = make_bar(dec!(3.00), dec!(3.08), dec!(2.95), dec!(3.07));
        let entry_bar = make_bar(dec!(3.07), dec!(3.12), dec!(3.05), dec!(3.10));

        let trigger = detector.check_bull_entry(&entry_bar, &signal_bar, SignalQuality::Strong);
        assert!(trigger.is_some());

        let t = trigger.unwrap();
        assert_eq!(t.entry_price, dec!(3.08));
        assert_eq!(t.stop_price, dec!(2.95));
        assert_eq!(t.risk, dec!(0.13));
        assert_eq!(t.signal_quality, SignalQuality::Strong);
    }

    #[test]
    fn test_bull_entry_not_triggered() {
        let detector = EntryBarDetector::new();
        let signal_bar = make_bar(dec!(3.00), dec!(3.08), dec!(2.95), dec!(3.07));
        // Entry bar doesn't go above signal bar's high
        let entry_bar = make_bar(dec!(3.05), dec!(3.07), dec!(3.02), dec!(3.06));

        let trigger = detector.check_bull_entry(&entry_bar, &signal_bar, SignalQuality::Moderate);
        assert!(trigger.is_none());
    }

    #[test]
    fn test_bear_entry_triggered() {
        let detector = EntryBarDetector::new();
        let signal_bar = make_bar(dec!(3.08), dec!(3.10), dec!(3.00), dec!(3.01));
        let entry_bar = make_bar(dec!(3.01), dec!(3.03), dec!(2.96), dec!(2.98));

        let trigger = detector.check_bear_entry(&entry_bar, &signal_bar, SignalQuality::Moderate);
        assert!(trigger.is_some());

        let t = trigger.unwrap();
        assert_eq!(t.entry_price, dec!(3.00));
        assert_eq!(t.stop_price, dec!(3.10));
        assert_eq!(t.risk, dec!(0.10));
    }

    #[test]
    fn test_bear_entry_not_triggered() {
        let detector = EntryBarDetector::new();
        let signal_bar = make_bar(dec!(3.08), dec!(3.10), dec!(3.00), dec!(3.01));
        // Entry bar stays above signal bar's low
        let entry_bar = make_bar(dec!(3.02), dec!(3.05), dec!(3.00), dec!(3.03));

        let trigger = detector.check_bear_entry(&entry_bar, &signal_bar, SignalQuality::Strong);
        assert!(trigger.is_none());
    }
}
