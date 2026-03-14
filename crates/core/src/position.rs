use chrono::{DateTime, FixedOffset, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::market::{Direction, SecurityId};

/// An open trading position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub security: SecurityId,
    pub direction: Direction,
    pub quantity: u64,
    pub entry_price: Decimal,
    pub current_price: Decimal,
    pub stop_loss: Decimal,
    pub take_profit: Option<Decimal>,
    pub opened_at: DateTime<Utc>,
}

impl Position {
    /// Calculate unrealized PnL
    pub fn unrealized_pnl(&self) -> Decimal {
        let diff = self.current_price - self.entry_price;
        let signed = match self.direction {
            Direction::Long => diff,
            Direction::Short => -diff,
        };
        signed * Decimal::from(self.quantity)
    }

    /// Calculate unrealized PnL as a percentage of entry
    pub fn unrealized_pnl_pct(&self) -> Decimal {
        if self.entry_price.is_zero() {
            return Decimal::ZERO;
        }
        let diff = self.current_price - self.entry_price;
        let signed = match self.direction {
            Direction::Long => diff,
            Direction::Short => -diff,
        };
        signed / self.entry_price * Decimal::ONE_HUNDRED
    }

    /// Update the current price and return the new unrealized PnL
    pub fn update_price(&mut self, price: Decimal) -> Decimal {
        self.current_price = price;
        self.unrealized_pnl()
    }

    /// Whether the stop loss has been hit
    pub fn is_stop_hit(&self, price: Decimal) -> bool {
        match self.direction {
            Direction::Long => price <= self.stop_loss,
            Direction::Short => price >= self.stop_loss,
        }
    }

    /// Whether the take profit has been hit
    pub fn is_target_hit(&self, price: Decimal) -> bool {
        match (self.direction, self.take_profit) {
            (Direction::Long, Some(tp)) => price >= tp,
            (Direction::Short, Some(tp)) => price <= tp,
            _ => false,
        }
    }

    /// Total notional value of the position at current price
    pub fn notional_value(&self) -> Decimal {
        self.current_price * Decimal::from(self.quantity)
    }

    /// Total notional value at entry
    pub fn entry_value(&self) -> Decimal {
        self.entry_price * Decimal::from(self.quantity)
    }

    /// Whether this position has settled under T+1 rules.
    ///
    /// A position is T+1 settled when the current CST date is strictly
    /// after the CST date on which the position was opened. This means
    /// shares bought today cannot be sold until the next trading day.
    pub fn is_t1_settled(&self, as_of: DateTime<Utc>) -> bool {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let open_date = self.opened_at.with_timezone(&cst).date_naive();
        let current_date = as_of.with_timezone(&cst).date_naive();
        current_date > open_date
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};
    use crate::market::Exchange;
    use rust_decimal_macros::dec;

    fn make_long_position() -> Position {
        Position {
            security: SecurityId::etf("510050", Exchange::SH),
            direction: Direction::Long,
            quantity: 1000,
            entry_price: dec!(3.100),
            current_price: dec!(3.150),
            stop_loss: dec!(3.050),
            take_profit: Some(dec!(3.250)),
            opened_at: Utc::now(),
        }
    }

    fn make_short_position() -> Position {
        Position {
            security: SecurityId::etf("510050", Exchange::SH),
            direction: Direction::Short,
            quantity: 1000,
            entry_price: dec!(3.100),
            current_price: dec!(3.050),
            stop_loss: dec!(3.150),
            take_profit: Some(dec!(2.950)),
            opened_at: Utc::now(),
        }
    }

    #[test]
    fn test_long_unrealized_pnl_profit() {
        let pos = make_long_position();
        // (3.150 - 3.100) * 1000 = 50
        assert_eq!(pos.unrealized_pnl(), dec!(50));
    }

    #[test]
    fn test_long_unrealized_pnl_loss() {
        let mut pos = make_long_position();
        pos.current_price = dec!(3.050);
        // (3.050 - 3.100) * 1000 = -50
        assert_eq!(pos.unrealized_pnl(), dec!(-50));
    }

    #[test]
    fn test_short_unrealized_pnl_profit() {
        let pos = make_short_position();
        // -(3.050 - 3.100) * 1000 = 50
        assert_eq!(pos.unrealized_pnl(), dec!(50));
    }

    #[test]
    fn test_short_unrealized_pnl_loss() {
        let mut pos = make_short_position();
        pos.current_price = dec!(3.150);
        // -(3.150 - 3.100) * 1000 = -50
        assert_eq!(pos.unrealized_pnl(), dec!(-50));
    }

    #[test]
    fn test_stop_hit_long() {
        let pos = make_long_position();
        assert!(pos.is_stop_hit(dec!(3.050)));
        assert!(pos.is_stop_hit(dec!(3.000)));
        assert!(!pos.is_stop_hit(dec!(3.100)));
    }

    #[test]
    fn test_stop_hit_short() {
        let pos = make_short_position();
        assert!(pos.is_stop_hit(dec!(3.150)));
        assert!(pos.is_stop_hit(dec!(3.200)));
        assert!(!pos.is_stop_hit(dec!(3.100)));
    }

    #[test]
    fn test_target_hit_long() {
        let pos = make_long_position();
        assert!(pos.is_target_hit(dec!(3.250)));
        assert!(pos.is_target_hit(dec!(3.300)));
        assert!(!pos.is_target_hit(dec!(3.200)));
    }

    #[test]
    fn test_target_hit_short() {
        let pos = make_short_position();
        assert!(pos.is_target_hit(dec!(2.950)));
        assert!(pos.is_target_hit(dec!(2.900)));
        assert!(!pos.is_target_hit(dec!(3.000)));
    }

    #[test]
    fn test_no_take_profit() {
        let mut pos = make_long_position();
        pos.take_profit = None;
        assert!(!pos.is_target_hit(dec!(100.0)));
    }

    #[test]
    fn test_notional_value() {
        let pos = make_long_position();
        assert_eq!(pos.notional_value(), dec!(3150)); // 3.150 * 1000
        assert_eq!(pos.entry_value(), dec!(3100)); // 3.100 * 1000
    }

    #[test]
    fn test_update_price() {
        let mut pos = make_long_position();
        let pnl = pos.update_price(dec!(3.200));
        assert_eq!(pnl, dec!(100)); // (3.200 - 3.100) * 1000
        assert_eq!(pos.current_price, dec!(3.200));
    }

    #[test]
    fn test_t1_settled_next_day() {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        // Opened at 2024-06-10 10:00 CST
        let opened = cst
            .with_ymd_and_hms(2024, 6, 10, 10, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let mut pos = make_long_position();
        pos.opened_at = opened;

        // Check at 2024-06-11 09:30 CST (next day) -> settled
        let next_day = cst
            .with_ymd_and_hms(2024, 6, 11, 9, 30, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        assert!(pos.is_t1_settled(next_day));
    }

    #[test]
    fn test_t1_not_settled_same_day() {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        // Opened at 2024-06-10 10:00 CST
        let opened = cst
            .with_ymd_and_hms(2024, 6, 10, 10, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let mut pos = make_long_position();
        pos.opened_at = opened;

        // Check at 2024-06-10 14:55 CST (same day) -> NOT settled
        let same_day = cst
            .with_ymd_and_hms(2024, 6, 10, 14, 55, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        assert!(!pos.is_t1_settled(same_day));
    }

    #[test]
    fn test_t1_not_settled_same_day_utc_next_day() {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        // Opened at 2024-06-10 23:00 CST (which is 2024-06-10 15:00 UTC)
        let opened = cst
            .with_ymd_and_hms(2024, 6, 10, 23, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let mut pos = make_long_position();
        pos.opened_at = opened;

        // Check at 2024-06-10 23:59 CST (same CST day, but next UTC day)
        let same_cst_day = cst
            .with_ymd_and_hms(2024, 6, 10, 23, 59, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        assert!(!pos.is_t1_settled(same_cst_day));
    }
}
