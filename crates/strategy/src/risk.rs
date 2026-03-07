use rust_decimal::Decimal;

use crate::config::RiskConfig;
use crate::error::StrategyError;

/// Position sizing and risk limit enforcement.
///
/// All calculations use `Decimal` — never `f64` for financial math.
pub struct RiskManager {
    config: RiskConfig,
    current_capital: Decimal,
    daily_starting_capital: Decimal,
    daily_realized_pnl: Decimal,
    open_position_count: usize,
}

const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

impl RiskManager {
    pub fn new(config: RiskConfig) -> Self {
        let capital = config.initial_capital;
        Self {
            config,
            current_capital: capital,
            daily_starting_capital: capital,
            daily_realized_pnl: Decimal::ZERO,
            open_position_count: 0,
        }
    }

    /// Calculate position size in lots given risk per unit and lot size.
    ///
    /// Formula:
    /// - `risk_amount = capital * max_risk_per_trade_pct / 100`
    /// - `shares = risk_amount / risk_per_unit`
    /// - `lots = floor(shares / lot_size) * lot_size`
    ///
    /// Returns 0 if the position would be smaller than one lot.
    pub fn calculate_position_size(
        &self,
        risk_per_unit: Decimal,
        lot_size: u64,
    ) -> Result<u64, StrategyError> {
        if risk_per_unit <= Decimal::ZERO {
            return Err(StrategyError::InvalidConfig(
                "risk_per_unit must be positive".to_string(),
            ));
        }

        let risk_amount = self.current_capital * self.config.max_risk_per_trade_pct / HUNDRED;
        let raw_shares = risk_amount / risk_per_unit;

        if raw_shares < Decimal::from(lot_size) {
            // Can't even afford one lot
            return Ok(0);
        }

        let lot_decimal = Decimal::from(lot_size);
        let lots = (raw_shares / lot_decimal).floor() * lot_decimal;

        // Convert to u64 — floor guarantees this is non-negative and integral
        Ok(lots.try_into().unwrap_or(0))
    }

    /// Check if a new trade is allowed (all risk checks).
    ///
    /// Returns `Ok(())` if trading is permitted, or the specific error.
    pub fn can_open_position(&self) -> Result<(), StrategyError> {
        // Check max positions
        if self.open_position_count >= self.config.max_open_positions {
            return Err(StrategyError::MaxPositionsReached {
                max: self.config.max_open_positions,
            });
        }

        // Check daily loss limit
        let loss_pct = self.daily_pnl_pct();
        if loss_pct <= -self.config.max_daily_loss_pct {
            return Err(StrategyError::DailyLossLimitReached {
                current_loss_pct: loss_pct.abs(),
                limit_pct: self.config.max_daily_loss_pct,
            });
        }

        Ok(())
    }

    /// Record a realized PnL from a closed position and decrement position count.
    pub fn record_close(&mut self, realized_pnl: Decimal) {
        self.daily_realized_pnl += realized_pnl;
        self.current_capital += realized_pnl;
        if self.open_position_count > 0 {
            self.open_position_count -= 1;
        }
    }

    /// Record a new position opened.
    pub fn record_open(&mut self) {
        self.open_position_count += 1;
    }

    /// Record a position closed (decrements count only, no PnL update).
    pub fn record_position_closed(&mut self) {
        if self.open_position_count > 0 {
            self.open_position_count -= 1;
        }
    }

    /// Update capital by a given amount (e.g., commission deduction).
    pub fn update_capital(&mut self, change: Decimal) {
        self.current_capital += change;
    }

    /// Reset daily counters (called at session open).
    pub fn reset_daily(&mut self) {
        self.daily_starting_capital = self.current_capital;
        self.daily_realized_pnl = Decimal::ZERO;
    }

    /// Current daily P&L as a percentage of starting capital.
    /// Negative means a loss.
    pub fn daily_pnl_pct(&self) -> Decimal {
        if self.daily_starting_capital.is_zero() {
            return Decimal::ZERO;
        }
        self.daily_realized_pnl / self.daily_starting_capital * HUNDRED
    }

    /// Remaining risk budget for a single trade (dollar amount).
    pub fn remaining_risk_budget(&self) -> Decimal {
        self.current_capital * self.config.max_risk_per_trade_pct / HUNDRED
    }

    pub fn current_capital(&self) -> Decimal {
        self.current_capital
    }

    pub fn open_position_count(&self) -> usize {
        self.open_position_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn default_config() -> RiskConfig {
        RiskConfig {
            max_risk_per_trade_pct: dec!(1), // 1%
            max_daily_loss_pct: dec!(3),     // 3%
            max_open_positions: 3,
            initial_capital: dec!(100000),
            min_reward_risk_ratio: dec!(1.5),
        }
    }

    #[test]
    fn test_position_size_basic() {
        let mgr = RiskManager::new(default_config());
        // risk_amount = 100_000 * 1 / 100 = 1_000
        // shares = 1_000 / 0.05 = 20_000
        // lots = floor(20_000 / 100) * 100 = 20_000
        let size = mgr.calculate_position_size(dec!(0.05), 100).unwrap();
        assert_eq!(size, 20_000);
    }

    #[test]
    fn test_position_size_rounds_down_to_lot_boundary() {
        let mgr = RiskManager::new(default_config());
        // risk_amount = 1_000
        // shares = 1_000 / 0.07 = 14285.714...
        // lots = floor(14285.714 / 100) * 100 = 14200
        let size = mgr.calculate_position_size(dec!(0.07), 100).unwrap();
        assert_eq!(size, 14200);
    }

    #[test]
    fn test_position_size_zero_when_too_expensive() {
        let config = RiskConfig {
            initial_capital: dec!(1000),
            ..default_config()
        };
        let mgr = RiskManager::new(config);
        // risk_amount = 1_000 * 1 / 100 = 10
        // shares = 10 / 5.0 = 2 — less than lot_size=100
        let size = mgr.calculate_position_size(dec!(5.0), 100).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_position_size_error_on_zero_risk() {
        let mgr = RiskManager::new(default_config());
        let result = mgr.calculate_position_size(Decimal::ZERO, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_can_open_position_ok() {
        let mgr = RiskManager::new(default_config());
        assert!(mgr.can_open_position().is_ok());
    }

    #[test]
    fn test_max_positions_reached() {
        let mut mgr = RiskManager::new(default_config());
        mgr.record_open();
        mgr.record_open();
        mgr.record_open();
        let result = mgr.can_open_position();
        assert!(matches!(
            result,
            Err(StrategyError::MaxPositionsReached { max: 3 })
        ));
    }

    #[test]
    fn test_daily_loss_limit_reached() {
        let mut mgr = RiskManager::new(default_config());
        // Lose 3% of 100_000 = -3000
        mgr.record_open();
        mgr.record_close(dec!(-3000));
        let result = mgr.can_open_position();
        assert!(matches!(
            result,
            Err(StrategyError::DailyLossLimitReached { .. })
        ));
    }

    #[test]
    fn test_daily_loss_limit_not_reached_yet() {
        let mut mgr = RiskManager::new(default_config());
        // Lose 2% of 100_000 = -2000 (below 3% limit)
        mgr.record_open();
        mgr.record_close(dec!(-2000));
        assert!(mgr.can_open_position().is_ok());
    }

    #[test]
    fn test_record_close_updates_pnl_and_capital() {
        let mut mgr = RiskManager::new(default_config());
        mgr.record_open();
        mgr.record_close(dec!(500));
        assert_eq!(mgr.current_capital(), dec!(100500));
        assert_eq!(mgr.daily_realized_pnl, dec!(500));
        assert_eq!(mgr.open_position_count(), 0);
    }

    #[test]
    fn test_record_open_increments_count() {
        let mut mgr = RiskManager::new(default_config());
        assert_eq!(mgr.open_position_count(), 0);
        mgr.record_open();
        assert_eq!(mgr.open_position_count(), 1);
        mgr.record_open();
        assert_eq!(mgr.open_position_count(), 2);
    }

    #[test]
    fn test_reset_daily() {
        let mut mgr = RiskManager::new(default_config());
        mgr.record_open();
        mgr.record_close(dec!(-1000));
        assert_eq!(mgr.daily_pnl_pct(), dec!(-1));

        mgr.reset_daily();
        assert_eq!(mgr.daily_pnl_pct(), dec!(0));
        // Capital is now 99_000, and that becomes the new daily starting capital
        assert_eq!(mgr.current_capital(), dec!(99000));
    }

    #[test]
    fn test_daily_pnl_pct() {
        let mut mgr = RiskManager::new(default_config());
        mgr.record_open();
        mgr.record_close(dec!(-1500));
        // -1500 / 100_000 * 100 = -1.5%
        assert_eq!(mgr.daily_pnl_pct(), dec!(-1.5));
    }

    #[test]
    fn test_remaining_risk_budget() {
        let mgr = RiskManager::new(default_config());
        // 100_000 * 1% = 1_000
        assert_eq!(mgr.remaining_risk_budget(), dec!(1000));
    }

    #[test]
    fn test_update_capital() {
        let mut mgr = RiskManager::new(default_config());
        mgr.update_capital(dec!(-50)); // commission
        assert_eq!(mgr.current_capital(), dec!(99950));
    }
}
