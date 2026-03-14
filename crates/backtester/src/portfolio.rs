use std::collections::HashMap;

use brooks_core::market::{Direction, SecurityId};
use brooks_core::position::Position;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::error::BacktestError;

/// A point on the equity curve.
#[derive(Debug, Clone, Serialize)]
pub struct EquityPoint {
    pub timestamp: DateTime<Utc>,
    pub equity: Decimal,
    pub cash: Decimal,
    pub unrealized_pnl: Decimal,
}

/// Portfolio state during a backtest.
pub struct Portfolio {
    cash: Decimal,
    positions: HashMap<SecurityId, Position>,
    equity_curve: Vec<EquityPoint>,
    realized_pnl: Decimal,
}

impl Portfolio {
    pub fn new(initial_capital: Decimal) -> Self {
        Self {
            cash: initial_capital,
            positions: HashMap::new(),
            equity_curve: Vec::new(),
            realized_pnl: Decimal::ZERO,
        }
    }

    /// Open a new position. Deducts entry_value from cash.
    #[allow(clippy::too_many_arguments)]
    pub fn open_position(
        &mut self,
        security: SecurityId,
        direction: Direction,
        quantity: u64,
        fill_price: Decimal,
        stop_loss: Decimal,
        take_profit: Option<Decimal>,
        timestamp: DateTime<Utc>,
    ) -> Result<(), BacktestError> {
        let cost = fill_price * Decimal::from(quantity);
        if cost > self.cash {
            return Err(BacktestError::InsufficientFunds {
                required: cost,
                available: self.cash,
            });
        }
        self.cash -= cost;
        let position = Position {
            security: security.clone(),
            direction,
            quantity,
            entry_price: fill_price,
            current_price: fill_price,
            stop_loss,
            take_profit,
            opened_at: timestamp,
        };
        self.positions.insert(security, position);
        Ok(())
    }

    /// Close a position at the given price. Returns realized PnL.
    ///
    /// For Long positions, enforces T+1 settlement: the position cannot be
    /// closed on the same CST date it was opened.
    pub fn close_position(
        &mut self,
        security: &SecurityId,
        fill_price: Decimal,
        close_timestamp: DateTime<Utc>,
    ) -> Result<Decimal, BacktestError> {
        // T+1 validation for Long positions
        if let Some(position) = self.positions.get(security) {
            if position.direction == Direction::Long
                && !position.is_t1_settled(close_timestamp)
            {
                return Err(BacktestError::T1SettlementViolation {
                    security: security.to_string(),
                });
            }
        }

        let position = self
            .positions
            .remove(security)
            .ok_or_else(|| BacktestError::PositionNotFound(security.to_string()))?;
        let pnl = match position.direction {
            Direction::Long => {
                (fill_price - position.entry_price) * Decimal::from(position.quantity)
            }
            Direction::Short => {
                (position.entry_price - fill_price) * Decimal::from(position.quantity)
            }
        };
        // Return entry value + PnL to cash
        let proceeds = position.entry_price * Decimal::from(position.quantity) + pnl;
        self.cash += proceeds;
        self.realized_pnl += pnl;
        Ok(pnl)
    }

    /// Mark-to-market: update the current price of an open position.
    pub fn update_price(&mut self, security: &SecurityId, price: Decimal) {
        if let Some(pos) = self.positions.get_mut(security) {
            pos.update_price(price);
        }
    }

    /// Update the stop loss of an open position.
    pub fn update_stop_loss(
        &mut self,
        security: &SecurityId,
        new_stop: Decimal,
    ) -> Result<(), BacktestError> {
        let pos = self
            .positions
            .get_mut(security)
            .ok_or_else(|| BacktestError::PositionNotFound(security.to_string()))?;
        pos.stop_loss = new_stop;
        Ok(())
    }

    /// Total unrealized PnL across all open positions.
    pub fn unrealized_pnl(&self) -> Decimal {
        self.positions.values().map(|p| p.unrealized_pnl()).sum()
    }

    /// Total portfolio equity: cash + positions value.
    pub fn equity(&self) -> Decimal {
        let positions_value: Decimal = self
            .positions
            .values()
            .map(|p| p.entry_price * Decimal::from(p.quantity) + p.unrealized_pnl())
            .sum();
        self.cash + positions_value
    }

    /// Record the current equity as a snapshot.
    pub fn record_equity_snapshot(&mut self, timestamp: DateTime<Utc>) {
        let point = EquityPoint {
            timestamp,
            equity: self.equity(),
            cash: self.cash,
            unrealized_pnl: self.unrealized_pnl(),
        };
        self.equity_curve.push(point);
    }

    pub fn cash(&self) -> Decimal {
        self.cash
    }

    pub fn realized_pnl(&self) -> Decimal {
        self.realized_pnl
    }

    pub fn equity_curve(&self) -> &[EquityPoint] {
        &self.equity_curve
    }

    pub fn has_position(&self, security: &SecurityId) -> bool {
        self.positions.contains_key(security)
    }

    pub fn get_position(&self, security: &SecurityId) -> Option<&Position> {
        self.positions.get(security)
    }

    pub fn open_position_count(&self) -> usize {
        self.positions.len()
    }

    pub fn position_securities(&self) -> Vec<SecurityId> {
        self.positions.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use rust_decimal_macros::dec;

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    #[test]
    fn test_new_portfolio() {
        let p = Portfolio::new(dec!(100000));
        assert_eq!(p.cash(), dec!(100000));
        assert_eq!(p.equity(), dec!(100000));
        assert_eq!(p.open_position_count(), 0);
    }

    #[test]
    fn test_open_position_deducts_cash() {
        let mut p = Portfolio::new(dec!(100000));
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            Some(dec!(3.200)),
            Utc::now(),
        )
        .unwrap();
        // 100000 - 3.100 * 1000 = 100000 - 3100 = 96900
        assert_eq!(p.cash(), dec!(96900));
        assert!(p.has_position(&security()));
    }

    #[test]
    fn test_insufficient_funds() {
        let mut p = Portfolio::new(dec!(1000));
        let r = p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            Utc::now(),
        );
        assert!(r.is_err());
    }

    #[test]
    fn test_close_long_profit() {
        let mut p = Portfolio::new(dec!(100000));
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            Utc::now(),
        )
        .unwrap();
        // Use a timestamp from the next day so T+1 is satisfied
        let next_day = Utc::now() + chrono::Duration::days(1);
        let pnl = p.close_position(&security(), dec!(3.200), next_day).unwrap();
        assert_eq!(pnl, dec!(100)); // (3.200 - 3.100) * 1000
        assert_eq!(p.cash(), dec!(100100)); // 96900 + 3200
        assert_eq!(p.realized_pnl(), dec!(100));
    }

    #[test]
    fn test_close_long_loss() {
        let mut p = Portfolio::new(dec!(100000));
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            Utc::now(),
        )
        .unwrap();
        let next_day = Utc::now() + chrono::Duration::days(1);
        let pnl = p.close_position(&security(), dec!(3.050), next_day).unwrap();
        assert_eq!(pnl, dec!(-50));
        assert_eq!(p.cash(), dec!(99950));
    }

    #[test]
    fn test_close_short_profit() {
        let mut p = Portfolio::new(dec!(100000));
        p.open_position(
            security(),
            Direction::Short,
            1000,
            dec!(3.100),
            dec!(3.150),
            None,
            Utc::now(),
        )
        .unwrap();
        // Short positions are not subject to T+1 sell restriction
        let pnl = p.close_position(&security(), dec!(3.000), Utc::now()).unwrap();
        assert_eq!(pnl, dec!(100));
    }

    #[test]
    fn test_equity_with_unrealized() {
        let mut p = Portfolio::new(dec!(100000));
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            Utc::now(),
        )
        .unwrap();
        p.update_price(&security(), dec!(3.200));
        // cash = 96900, position_value = 3100 + 100 = 3200
        assert_eq!(p.equity(), dec!(100100));
        assert_eq!(p.unrealized_pnl(), dec!(100));
    }

    #[test]
    fn test_equity_snapshot() {
        let mut p = Portfolio::new(dec!(100000));
        p.record_equity_snapshot(Utc::now());
        assert_eq!(p.equity_curve().len(), 1);
        assert_eq!(p.equity_curve()[0].equity, dec!(100000));
    }

    #[test]
    fn test_update_stop_loss() {
        let mut p = Portfolio::new(dec!(100000));
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            Utc::now(),
        )
        .unwrap();
        p.update_stop_loss(&security(), dec!(3.080)).unwrap();
        assert_eq!(p.get_position(&security()).unwrap().stop_loss, dec!(3.080));
    }

    #[test]
    fn test_close_nonexistent() {
        let mut p = Portfolio::new(dec!(100000));
        let next_day = Utc::now() + chrono::Duration::days(1);
        assert!(p.close_position(&security(), dec!(3.100), next_day).is_err());
    }

    #[test]
    fn test_t1_blocks_same_day_long_close() {
        let mut p = Portfolio::new(dec!(100000));
        let now = Utc::now();
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            now,
        )
        .unwrap();
        // Try to close on the same day
        let result = p.close_position(&security(), dec!(3.200), now);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("T+1 settlement violation"));
        // Position should still be open
        assert!(p.has_position(&security()));
    }

    #[test]
    fn test_t1_allows_next_day_long_close() {
        let mut p = Portfolio::new(dec!(100000));
        let now = Utc::now();
        p.open_position(
            security(),
            Direction::Long,
            1000,
            dec!(3.100),
            dec!(3.050),
            None,
            now,
        )
        .unwrap();
        // Close on the next day — should succeed
        let next_day = now + chrono::Duration::days(1);
        let pnl = p.close_position(&security(), dec!(3.200), next_day).unwrap();
        assert_eq!(pnl, dec!(100));
    }

    #[test]
    fn test_t1_allows_same_day_short_close() {
        let mut p = Portfolio::new(dec!(100000));
        let now = Utc::now();
        p.open_position(
            security(),
            Direction::Short,
            1000,
            dec!(3.100),
            dec!(3.150),
            None,
            now,
        )
        .unwrap();
        // Short positions can be closed same day (T+1 only applies to Long)
        let pnl = p.close_position(&security(), dec!(3.000), now).unwrap();
        assert_eq!(pnl, dec!(100));
    }
}
