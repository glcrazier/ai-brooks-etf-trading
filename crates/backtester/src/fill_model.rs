use brooks_core::bar::Bar;
use brooks_core::market::Direction;
use brooks_core::order::{Order, OrderType};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Result of a simulated fill.
#[derive(Debug, Clone)]
pub struct FillResult {
    pub fill_price: Decimal,
    pub fill_time: DateTime<Utc>,
}

/// Trait for simulating order fills in a backtest.
pub trait FillModel: Send {
    /// Attempt to fill the given order using the current bar's data.
    ///
    /// Returns `Some(FillResult)` if the order would be filled on this bar,
    /// `None` if the order is not triggered.
    fn try_fill(&self, order: &Order, bar: &Bar) -> Option<FillResult>;
}

/// Fill model that simulates execution at the bar's open price with slippage.
///
/// - **Market orders**: always fill at bar open + slippage.
/// - **Stop orders (buy)**: trigger if bar high >= stop_price; fill at open + slippage.
/// - **Stop orders (sell)**: trigger if bar low <= stop_price; fill at open + slippage.
/// - **Limit orders (buy)**: trigger if bar low <= limit_price; fill at open + slippage.
/// - **Limit orders (sell)**: trigger if bar high >= limit_price; fill at open + slippage.
pub struct NextBarOpenFill {
    /// Slippage as a decimal fraction (e.g., 0.0005 for 5 bps).
    slippage: Decimal,
}

impl NextBarOpenFill {
    pub fn new(slippage: Decimal) -> Self {
        Self { slippage }
    }

    /// Apply slippage adversely: buys pay more, sells receive less.
    fn apply_slippage(&self, price: Decimal, direction: Direction) -> Decimal {
        match direction {
            Direction::Long => price * (Decimal::ONE + self.slippage),
            Direction::Short => price * (Decimal::ONE - self.slippage),
        }
    }
}

impl FillModel for NextBarOpenFill {
    fn try_fill(&self, order: &Order, bar: &Bar) -> Option<FillResult> {
        let triggered = match order.order_type {
            OrderType::Market => true,
            OrderType::Stop => {
                let stop = order.stop_price.unwrap_or(Decimal::ZERO);
                match order.direction {
                    Direction::Long => bar.high >= stop,
                    Direction::Short => bar.low <= stop,
                }
            }
            OrderType::Limit => {
                let limit = order.price.unwrap_or(Decimal::ZERO);
                match order.direction {
                    Direction::Long => bar.low <= limit,
                    Direction::Short => bar.high >= limit,
                }
            }
            OrderType::StopLimit => {
                // For simplicity, treat as stop order in backtest
                let stop = order.stop_price.unwrap_or(Decimal::ZERO);
                match order.direction {
                    Direction::Long => bar.high >= stop,
                    Direction::Short => bar.low <= stop,
                }
            }
        };

        if triggered {
            let fill_price = self.apply_slippage(bar.open, order.direction);
            Some(FillResult {
                fill_price,
                fill_time: bar.timestamp,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::{Exchange, SecurityId};
    use brooks_core::timeframe::Timeframe;
    use rust_decimal_macros::dec;

    fn make_bar(open: Decimal, high: Decimal, low: Decimal, close: Decimal) -> Bar {
        Bar {
            timestamp: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 10000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    fn security() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    #[test]
    fn test_market_order_always_fills() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::market(security(), Direction::Long, 100);
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.120));

        let result = model.try_fill(&order, &bar).unwrap();
        // 3.100 * 1.0005 = 3.10155
        assert_eq!(result.fill_price, dec!(3.1015500));
    }

    #[test]
    fn test_market_order_short_slippage() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::market(security(), Direction::Short, 100);
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.090));

        let result = model.try_fill(&order, &bar).unwrap();
        // 3.100 * 0.9995 = 3.09845
        assert_eq!(result.fill_price, dec!(3.0984500));
    }

    #[test]
    fn test_stop_buy_triggered() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.120));
        // Bar high 3.150 >= stop 3.120 -> triggered
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.130));

        let result = model.try_fill(&order, &bar);
        assert!(result.is_some());
    }

    #[test]
    fn test_stop_buy_not_triggered() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::stop(security(), Direction::Long, 100, dec!(3.200));
        // Bar high 3.150 < stop 3.200 -> not triggered
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.130));

        let result = model.try_fill(&order, &bar);
        assert!(result.is_none());
    }

    #[test]
    fn test_stop_sell_triggered() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::stop(security(), Direction::Short, 100, dec!(3.090));
        // Bar low 3.080 <= stop 3.090 -> triggered
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.130));

        let result = model.try_fill(&order, &bar);
        assert!(result.is_some());
    }

    #[test]
    fn test_stop_sell_not_triggered() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::stop(security(), Direction::Short, 100, dec!(3.050));
        // Bar low 3.080 > stop 3.050 -> not triggered
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.130));

        let result = model.try_fill(&order, &bar);
        assert!(result.is_none());
    }

    #[test]
    fn test_limit_buy_triggered() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.090));
        // Bar low 3.080 <= limit 3.090 -> triggered
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.130));

        let result = model.try_fill(&order, &bar);
        assert!(result.is_some());
    }

    #[test]
    fn test_limit_buy_not_triggered() {
        let model = NextBarOpenFill::new(dec!(0.0005));
        let order = Order::limit(security(), Direction::Long, 100, dec!(3.070));
        // Bar low 3.080 > limit 3.070 -> not triggered
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.130));

        let result = model.try_fill(&order, &bar);
        assert!(result.is_none());
    }

    #[test]
    fn test_zero_slippage() {
        let model = NextBarOpenFill::new(dec!(0));
        let order = Order::market(security(), Direction::Long, 100);
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.120));

        let result = model.try_fill(&order, &bar).unwrap();
        assert_eq!(result.fill_price, dec!(3.100));
    }

    #[test]
    fn test_fill_time_uses_bar_timestamp() {
        let model = NextBarOpenFill::new(dec!(0));
        let order = Order::market(security(), Direction::Long, 100);
        let bar = make_bar(dec!(3.100), dec!(3.150), dec!(3.080), dec!(3.120));
        let expected_time = bar.timestamp;

        let result = model.try_fill(&order, &bar).unwrap();
        assert_eq!(result.fill_time, expected_time);
    }
}
