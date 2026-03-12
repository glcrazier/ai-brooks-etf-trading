use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::portfolio::EquityPoint;
use crate::trade_log::TradeLog;

/// Summary performance metrics for a completed backtest.
#[derive(Debug, Clone, Serialize)]
pub struct BacktestMetrics {
    pub initial_capital: Decimal,
    pub final_equity: Decimal,
    pub total_pnl: Decimal,
    pub total_return_pct: Decimal,
    pub max_drawdown_pct: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: usize,
    pub num_winners: usize,
    pub num_losers: usize,
    pub avg_win: Decimal,
    pub avg_loss: Decimal,
    pub max_consecutive_losses: usize,
}

impl BacktestMetrics {
    /// Calculate metrics from equity curve and trade log.
    pub fn calculate(
        initial_capital: Decimal,
        equity_curve: &[EquityPoint],
        trade_log: &TradeLog,
    ) -> Self {
        let final_equity = equity_curve
            .last()
            .map(|p| p.equity)
            .unwrap_or(initial_capital);

        let total_pnl = final_equity - initial_capital;
        let total_return_pct = if initial_capital.is_zero() {
            Decimal::ZERO
        } else {
            total_pnl / initial_capital * Decimal::ONE_HUNDRED
        };

        let max_drawdown_pct = Self::calc_max_drawdown(equity_curve);
        let (sharpe, sortino) = Self::calc_sharpe_sortino(equity_curve);

        let total_trades = trade_log.len();
        let winners = trade_log.winners();
        let losers = trade_log.losers();
        let num_winners = winners.len();
        let num_losers = losers.len();

        let win_rate = if total_trades > 0 {
            num_winners as f64 / total_trades as f64
        } else {
            0.0
        };

        let gross_profit = trade_log.gross_profit();
        let gross_loss = trade_log.gross_loss().abs();
        let profit_factor = if gross_loss.is_zero() {
            if gross_profit.is_zero() {
                0.0
            } else {
                f64::INFINITY
            }
        } else {
            gross_profit.to_f64().unwrap_or(0.0) / gross_loss.to_f64().unwrap_or(1.0)
        };

        let avg_win = if num_winners > 0 {
            trade_log.gross_profit() / Decimal::from(num_winners as u64)
        } else {
            Decimal::ZERO
        };

        let avg_loss = if num_losers > 0 {
            trade_log.gross_loss() / Decimal::from(num_losers as u64)
        } else {
            Decimal::ZERO
        };

        let max_consecutive_losses = Self::calc_max_consecutive_losses(trade_log);

        Self {
            initial_capital,
            final_equity,
            total_pnl,
            total_return_pct,
            max_drawdown_pct,
            sharpe_ratio: sharpe,
            sortino_ratio: sortino,
            win_rate,
            profit_factor,
            total_trades,
            num_winners,
            num_losers,
            avg_win,
            avg_loss,
            max_consecutive_losses,
        }
    }

    fn calc_max_drawdown(curve: &[EquityPoint]) -> f64 {
        if curve.is_empty() {
            return 0.0;
        }
        let mut peak = curve[0].equity;
        let mut max_dd = 0.0_f64;
        for point in curve {
            if point.equity > peak {
                peak = point.equity;
            }
            if !peak.is_zero() {
                let dd = (peak - point.equity).to_f64().unwrap_or(0.0)
                    / peak.to_f64().unwrap_or(1.0)
                    * 100.0;
                if dd > max_dd {
                    max_dd = dd;
                }
            }
        }
        max_dd
    }

    fn calc_sharpe_sortino(curve: &[EquityPoint]) -> (f64, f64) {
        if curve.len() < 2 {
            return (0.0, 0.0);
        }
        let mut returns = Vec::with_capacity(curve.len() - 1);
        for i in 1..curve.len() {
            let prev = curve[i - 1].equity.to_f64().unwrap_or(1.0);
            let curr = curve[i].equity.to_f64().unwrap_or(1.0);
            if prev != 0.0 {
                returns.push((curr - prev) / prev);
            }
        }
        if returns.is_empty() {
            return (0.0, 0.0);
        }

        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        let downside_var = returns
            .iter()
            .filter(|&&r| r < 0.0)
            .map(|r| r.powi(2))
            .sum::<f64>()
            / n;
        let downside_dev = downside_var.sqrt();

        // Annualize: assume ~252 trading days
        let periods_per_year: f64 = 252.0;
        let sharpe = if std_dev > 0.0 {
            mean / std_dev * periods_per_year.sqrt()
        } else {
            0.0
        };

        let sortino = if downside_dev > 0.0 {
            mean / downside_dev * periods_per_year.sqrt()
        } else {
            0.0
        };

        (sharpe, sortino)
    }

    fn calc_max_consecutive_losses(log: &TradeLog) -> usize {
        let mut max_streak = 0;
        let mut current = 0;
        for trade in log.trades() {
            if trade.realized_pnl < Decimal::ZERO {
                current += 1;
                if current > max_streak {
                    max_streak = current;
                }
            } else {
                current = 0;
            }
        }
        max_streak
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade_log::{TradeLog, TradeRecord};
    use brooks_core::market::{Direction, Exchange, SecurityId};
    use brooks_core::signal::SignalType;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn make_equity_curve(values: &[Decimal]) -> Vec<EquityPoint> {
        values
            .iter()
            .map(|&v| EquityPoint {
                timestamp: Utc::now(),
                equity: v,
                cash: v,
                unrealized_pnl: Decimal::ZERO,
            })
            .collect()
    }

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
    fn test_total_return() {
        let curve = make_equity_curve(&[dec!(100000), dec!(105000)]);
        let log = TradeLog::new();
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert_eq!(m.total_return_pct, dec!(5));
        assert_eq!(m.total_pnl, dec!(5000));
    }

    #[test]
    fn test_max_drawdown() {
        // Peak at 110k, trough at 99k => dd = 11/110 * 100 = 10%
        let curve = make_equity_curve(&[
            dec!(100000),
            dec!(110000),
            dec!(99000),
            dec!(105000),
        ]);
        let log = TradeLog::new();
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert!((m.max_drawdown_pct - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_win_rate_and_counts() {
        let mut log = TradeLog::new();
        log.record(make_trade(dec!(100)));
        log.record(make_trade(dec!(-50)));
        log.record(make_trade(dec!(200)));
        let curve = make_equity_curve(&[dec!(100000), dec!(100250)]);
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert_eq!(m.total_trades, 3);
        assert_eq!(m.num_winners, 2);
        assert_eq!(m.num_losers, 1);
        assert!((m.win_rate - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_profit_factor() {
        let mut log = TradeLog::new();
        log.record(make_trade(dec!(300)));
        log.record(make_trade(dec!(-100)));
        let curve = make_equity_curve(&[dec!(100000), dec!(100200)]);
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert!((m.profit_factor - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_avg_win_loss() {
        let mut log = TradeLog::new();
        log.record(make_trade(dec!(100)));
        log.record(make_trade(dec!(200)));
        log.record(make_trade(dec!(-60)));
        let curve = make_equity_curve(&[dec!(100000)]);
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert_eq!(m.avg_win, dec!(150)); // (100+200)/2
        assert_eq!(m.avg_loss, dec!(-60));
    }

    #[test]
    fn test_max_consecutive_losses() {
        let mut log = TradeLog::new();
        log.record(make_trade(dec!(100)));
        log.record(make_trade(dec!(-50)));
        log.record(make_trade(dec!(-30)));
        log.record(make_trade(dec!(-20)));
        log.record(make_trade(dec!(80)));
        log.record(make_trade(dec!(-10)));
        let curve = make_equity_curve(&[dec!(100000)]);
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert_eq!(m.max_consecutive_losses, 3);
    }

    #[test]
    fn test_empty_backtest() {
        let curve = make_equity_curve(&[dec!(100000)]);
        let log = TradeLog::new();
        let m = BacktestMetrics::calculate(dec!(100000), &curve, &log);
        assert_eq!(m.total_trades, 0);
        assert_eq!(m.total_pnl, Decimal::ZERO);
        assert_eq!(m.win_rate, 0.0);
        assert_eq!(m.profit_factor, 0.0);
        assert_eq!(m.max_consecutive_losses, 0);
    }
}
