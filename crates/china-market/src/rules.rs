use brooks_core::market::{SecurityId, SecurityType};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Upper and lower price limits for a trading day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLimits {
    pub upper_limit: Decimal,
    pub lower_limit: Decimal,
}

/// Trait for market-specific trading rules
pub trait MarketRules: Send + Sync {
    /// Can this security be sold on the same day it was bought?
    fn allows_intraday_round_trip(&self, security: &SecurityId) -> bool;

    /// Price limit as a decimal (e.g., 0.10 for 10%)
    fn price_limit_pct(&self, security: &SecurityId) -> Decimal;

    /// Calculate upper/lower price limits for today based on previous close
    fn price_limits(&self, security: &SecurityId, prev_close: Decimal) -> PriceLimits;

    /// Minimum order quantity (lot size)
    fn min_lot_size(&self, security: &SecurityId) -> u64;

    /// Minimum price tick
    fn tick_size(&self, security: &SecurityId) -> Decimal;

    /// Round a price to the nearest valid tick
    fn round_to_tick(&self, security: &SecurityId, price: Decimal) -> Decimal;
}

/// China A-share and ETF market rules
pub struct ChinaMarketRules;

impl MarketRules for ChinaMarketRules {
    fn allows_intraday_round_trip(&self, _security: &SecurityId) -> bool {
        // All securities in China A-share market follow T+1 settlement:
        // shares bought today cannot be sold until the next trading day.
        // This applies to both ETFs and stocks.
        false
    }

    fn price_limit_pct(&self, security: &SecurityId) -> Decimal {
        match security.security_type {
            // Standard ETFs: 10% price limit
            // Note: Some cross-border ETFs and bond ETFs may differ;
            // this can be refined with security_info later
            SecurityType::ETF => dec!(0.10),
            // Main board stocks: 10%
            // ChiNext/STAR board: 20% (handled via security_info later)
            SecurityType::Stock => dec!(0.10),
        }
    }

    fn price_limits(&self, security: &SecurityId, prev_close: Decimal) -> PriceLimits {
        let pct = self.price_limit_pct(security);
        let tick = self.tick_size(security);

        let upper = round_to_tick(prev_close * (Decimal::ONE + pct), tick);
        let lower = round_to_tick(prev_close * (Decimal::ONE - pct), tick);

        PriceLimits {
            upper_limit: upper,
            lower_limit: lower,
        }
    }

    fn min_lot_size(&self, _security: &SecurityId) -> u64 {
        // 1 lot = 100 shares/units for both ETFs and stocks
        100
    }

    fn tick_size(&self, _security: &SecurityId) -> Decimal {
        // 0.001 yuan for both ETFs and stocks
        dec!(0.001)
    }

    fn round_to_tick(&self, security: &SecurityId, price: Decimal) -> Decimal {
        let tick = self.tick_size(security);
        round_to_tick(price, tick)
    }
}

/// Round a price down to the nearest tick
fn round_to_tick(price: Decimal, tick: Decimal) -> Decimal {
    if tick.is_zero() {
        return price;
    }
    (price / tick).floor() * tick
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;

    fn rules() -> ChinaMarketRules {
        ChinaMarketRules
    }

    fn etf() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    fn stock() -> SecurityId {
        SecurityId::stock("600519", Exchange::SH)
    }

    #[test]
    fn test_etf_follows_t1() {
        // ETFs in China follow T+1 settlement
        assert!(!rules().allows_intraday_round_trip(&etf()));
    }

    #[test]
    fn test_stock_follows_t1() {
        // Stocks in China follow T+1 settlement
        assert!(!rules().allows_intraday_round_trip(&stock()));
    }

    #[test]
    fn test_price_limit_pct() {
        assert_eq!(rules().price_limit_pct(&etf()), dec!(0.10));
        assert_eq!(rules().price_limit_pct(&stock()), dec!(0.10));
    }

    #[test]
    fn test_price_limits() {
        let limits = rules().price_limits(&etf(), dec!(3.000));
        assert_eq!(limits.upper_limit, dec!(3.300));
        assert_eq!(limits.lower_limit, dec!(2.700));
    }

    #[test]
    fn test_price_limits_rounding() {
        // prev_close = 3.456
        // upper = 3.456 * 1.10 = 3.8016 -> rounded to tick 0.001 -> 3.801
        // lower = 3.456 * 0.90 = 3.1104 -> rounded to tick 0.001 -> 3.110
        let limits = rules().price_limits(&etf(), dec!(3.456));
        assert_eq!(limits.upper_limit, dec!(3.801));
        assert_eq!(limits.lower_limit, dec!(3.110));
    }

    #[test]
    fn test_min_lot_size() {
        assert_eq!(rules().min_lot_size(&etf()), 100);
        assert_eq!(rules().min_lot_size(&stock()), 100);
    }

    #[test]
    fn test_tick_size() {
        assert_eq!(rules().tick_size(&etf()), dec!(0.001));
    }

    #[test]
    fn test_round_to_tick() {
        assert_eq!(rules().round_to_tick(&etf(), dec!(3.1234)), dec!(3.123));
        assert_eq!(rules().round_to_tick(&etf(), dec!(3.1239)), dec!(3.123));
        assert_eq!(rules().round_to_tick(&etf(), dec!(3.100)), dec!(3.100));
    }
}
