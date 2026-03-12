use brooks_core::market::{Direction, SecurityId};
use brooks_core::signal::SignalType;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

/// A completed round-trip trade record.
#[derive(Debug, Clone, Serialize)]
pub struct TradeRecord {
    pub security: SecurityId,
    pub direction: Direction,
    pub quantity: u64,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub realized_pnl: Decimal,
    pub signal_type: SignalType,
    pub exit_reason: String,
}

impl TradeRecord {
    /// PnL as a percentage of entry value.
    pub fn pnl_pct(&self) -> Decimal {
        let entry_val = self.entry_price * Decimal::from(self.quantity);
        if entry_val.is_zero() {
            return Decimal::ZERO;
        }
        self.realized_pnl / entry_val * Decimal::ONE_HUNDRED
    }

    /// Whether this trade was profitable.
    pub fn is_winner(&self) -> bool {
        self.realized_pnl > Decimal::ZERO
    }

    /// Hold duration in seconds.
    pub fn hold_duration_secs(&self) -> i64 {
        (self.exit_time - self.entry_time).num_seconds()
    }
}

/// Collection of all completed trades during a backtest.
pub struct TradeLog {
    trades: Vec<TradeRecord>,
}

impl TradeLog {
    pub fn new() -> Self {
        Self { trades: Vec::new() }
    }

    pub fn record(&mut self, trade: TradeRecord) {
        self.trades.push(trade);
    }

    pub fn trades(&self) -> &[TradeRecord] {
        &self.trades
    }

    pub fn len(&self) -> usize {
        self.trades.len()
    }

    pub fn is_empty(&self) -> bool {
        self.trades.is_empty()
    }

    pub fn winners(&self) -> Vec<&TradeRecord> {
        self.trades.iter().filter(|t| t.is_winner()).collect()
    }

    pub fn losers(&self) -> Vec<&TradeRecord> {
        self.trades
            .iter()
            .filter(|t| !t.is_winner() && t.realized_pnl != Decimal::ZERO)
            .collect()
    }

    pub fn total_pnl(&self) -> Decimal {
        self.trades.iter().map(|t| t.realized_pnl).sum()
    }

    pub fn gross_profit(&self) -> Decimal {
        self.trades
            .iter()
            .filter(|t| t.realized_pnl > Decimal::ZERO)
            .map(|t| t.realized_pnl)
            .sum()
    }

    pub fn gross_loss(&self) -> Decimal {
        self.trades
            .iter()
            .filter(|t| t.realized_pnl < Decimal::ZERO)
            .map(|t| t.realized_pnl)
            .sum()
    }
}

impl Default for TradeLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use rust_decimal_macros::dec;

    fn make_trade(pnl: Decimal) -> TradeRecord {
        TradeRecord {
            security: SecurityId::etf("510050", Exchange::SH),
            direction: Direction::Long,
            quantity: 1000,
            entry_price: dec!(3.100),
            exit_price: dec!(3.100) + pnl / dec!(1000),
            entry_time: Utc::now(),
            exit_time: Utc::now(),
            realized_pnl: pnl,
            signal_type: SignalType::PullbackEntry,
            exit_reason: "Test".to_string(),
        }
    }

    #[test]
    fn test_trade_record_pnl_pct() {
        let t = make_trade(dec!(100));
        // entry_val = 3.100 * 1000 = 3100
        // pnl_pct = 100 / 3100 * 100 ~= 3.225...
        let pct = t.pnl_pct();
        assert!(pct > dec!(3) && pct < dec!(4));
    }

    #[test]
    fn test_is_winner() {
        assert!(make_trade(dec!(100)).is_winner());
        assert!(!make_trade(dec!(-50)).is_winner());
        assert!(!make_trade(dec!(0)).is_winner());
    }

    #[test]
    fn test_trade_log_basics() {
        let mut log = TradeLog::new();
        assert!(log.is_empty());

        log.record(make_trade(dec!(100)));
        log.record(make_trade(dec!(-50)));
        log.record(make_trade(dec!(200)));

        assert_eq!(log.len(), 3);
        assert_eq!(log.total_pnl(), dec!(250));
        assert_eq!(log.winners().len(), 2);
        assert_eq!(log.losers().len(), 1);
    }

    #[test]
    fn test_gross_profit_and_loss() {
        let mut log = TradeLog::new();
        log.record(make_trade(dec!(100)));
        log.record(make_trade(dec!(-50)));
        log.record(make_trade(dec!(200)));

        assert_eq!(log.gross_profit(), dec!(300));
        assert_eq!(log.gross_loss(), dec!(-50));
    }

    #[test]
    fn test_empty_log() {
        let log = TradeLog::new();
        assert_eq!(log.total_pnl(), Decimal::ZERO);
        assert_eq!(log.gross_profit(), Decimal::ZERO);
        assert_eq!(log.gross_loss(), Decimal::ZERO);
        assert!(log.winners().is_empty());
        assert!(log.losers().is_empty());
    }
}
