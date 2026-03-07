use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::str::FromStr;
use tracing::info;

use brooks_backtester::{BacktestEngine, BacktestMetrics, BacktestResult, TradeLog, TradeRecord};
use brooks_china_market::rules::ChinaMarketRules;
use brooks_china_market::session::TradingSession;
use brooks_market_data::VecDataFeed;
use brooks_strategy::BrooksStrategy;

use crate::cli::BacktestArgs;
use crate::commands::{parse_exchange, parse_securities, parse_timeframe, AppConfig};

/// Run the backtest command.
pub fn run(args: &BacktestArgs, config: &mut AppConfig) -> Result<()> {
    // Apply CLI overrides to config
    if let Some(ref start) = args.start_date {
        config.backtest.start_date = start.clone();
    }
    if let Some(ref end) = args.end_date {
        config.backtest.end_date = end.clone();
    }
    if let Some(capital) = args.capital {
        config.backtest.initial_capital =
            Decimal::from_str(&format!("{:.2}", capital)).unwrap_or(config.backtest.initial_capital);
        config.strategy.risk.initial_capital = config.backtest.initial_capital;
    }

    let exchange = parse_exchange(&config.market.exchange)?;

    // Determine securities to backtest
    let security_codes: Vec<String> = if let Some(ref s) = args.securities {
        s.split(',').map(|c| c.trim().to_string()).collect()
    } else {
        config.market.securities.clone()
    };
    let securities = parse_securities(&security_codes, exchange);

    // Determine timeframe
    let timeframe = if let Some(ref tf) = args.timeframe {
        parse_timeframe(tf)?
    } else {
        parse_timeframe(&config.market.primary_timeframe)?
    };

    info!(
        "Starting backtest: {} securities, timeframe={}, period={} to {}",
        securities.len(),
        timeframe,
        config.backtest.start_date,
        config.backtest.end_date,
    );

    // For each security, run a backtest
    for security in &securities {
        info!("Backtesting {} ...", security);

        // Create data feed
        let mut feed = create_data_feed(args, config, security, timeframe)?;

        // Create strategy
        let session = TradingSession::china_a_share();
        let market_rules = Box::new(ChinaMarketRules);
        let mut strategy = BrooksStrategy::new(config.strategy.clone(), session, market_rules);

        // Create and run engine
        let engine = BacktestEngine::new(config.backtest.clone());
        let result = engine
            .run(&mut feed, &mut strategy, security, timeframe)
            .with_context(|| format!("backtest failed for {}", security))?;

        // Display results
        print_results(security, timeframe, &result);

        // Optionally write JSON output
        if let Some(ref output_path) = args.output_file {
            write_json_output(output_path, security, &result)
                .with_context(|| format!("failed to write output to {}", output_path))?;
            println!("\nResults written to: {}", output_path);
        }
    }

    Ok(())
}

/// Create a data feed for backtesting.
/// If --data-file is specified, load from CSV. Otherwise, create an empty feed
/// (in a real deployment, this would fetch from Futu).
fn create_data_feed(
    args: &BacktestArgs,
    _config: &AppConfig,
    security: &brooks_core::market::SecurityId,
    timeframe: brooks_core::timeframe::Timeframe,
) -> Result<VecDataFeed> {
    if let Some(ref data_file) = args.data_file {
        let bars = load_bars_from_csv(data_file, security, timeframe)
            .with_context(|| format!("failed to load bars from {}", data_file))?;
        info!("Loaded {} bars from {}", bars.len(), data_file);
        Ok(VecDataFeed::new(bars))
    } else {
        // Without a data file or Futu connection, we create an empty feed.
        // In production, this would connect to Futu and fetch historical bars.
        anyhow::bail!(
            "No data source specified. Use --data-file to provide CSV data, \
             or ensure FutuOpenD is running for live data fetching."
        );
    }
}

/// Load bars from a CSV file (public for use by other commands).
/// Uses a default security (510050.SH ETF) and timeframe (5min) for bar metadata.
pub fn load_bars_from_csv_public(path: &str) -> Result<Vec<brooks_core::bar::Bar>> {
    let default_security = brooks_core::market::SecurityId::etf("510050", brooks_core::market::Exchange::SH);
    let default_tf = brooks_core::timeframe::Timeframe::Minute5;
    load_bars_from_csv(path, &default_security, default_tf)
}

/// Load bars from a CSV file.
/// Expected columns: timestamp,open,high,low,close,volume
fn load_bars_from_csv(
    path: &str,
    security: &brooks_core::market::SecurityId,
    timeframe: brooks_core::timeframe::Timeframe,
) -> Result<Vec<brooks_core::bar::Bar>> {
    use chrono::{DateTime, Utc};

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read file: {}", path))?;

    let mut bars = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        // Skip header
        if i == 0 && line.contains("timestamp") {
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 6 {
            anyhow::bail!("line {}: expected 6 columns, got {}", i + 1, fields.len());
        }

        let timestamp: DateTime<Utc> = fields[0]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid timestamp '{}'", i + 1, fields[0]))?;
        let open: Decimal = fields[1]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid open '{}'", i + 1, fields[1]))?;
        let high: Decimal = fields[2]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid high '{}'", i + 1, fields[2]))?;
        let low: Decimal = fields[3]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid low '{}'", i + 1, fields[3]))?;
        let close: Decimal = fields[4]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid close '{}'", i + 1, fields[4]))?;
        let volume: u64 = fields[5]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid volume '{}'", i + 1, fields[5]))?;

        bars.push(brooks_core::bar::Bar {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
            timeframe,
            security: security.clone(),
        });
    }

    Ok(bars)
}

/// Print backtest results to the console in a formatted table.
fn print_results(
    security: &brooks_core::market::SecurityId,
    timeframe: brooks_core::timeframe::Timeframe,
    result: &BacktestResult,
) {
    let m = &result.metrics;

    println!();
    println!("═══════════════════════════════════════════════════════════════════════");
    println!("  BACKTEST RESULTS: {} ({})", security, timeframe);
    println!("═══════════════════════════════════════════════════════════════════════");
    println!();
    print_metrics(m);
    println!();

    if !result.trade_log.trades().is_empty() {
        print_trade_summary(&result.trade_log);
    } else {
        println!("  No trades were executed.");
    }

    println!();
    println!("═══════════════════════════════════════════════════════════════════════");
}

/// Print performance metrics.
fn print_metrics(m: &BacktestMetrics) {
    println!(
        "  Initial Capital:         ¥{:.2}",
        m.initial_capital
    );
    println!(
        "  Final Equity:            ¥{:.2}",
        m.final_equity
    );
    println!(
        "  Total PnL:               ¥{:.2}",
        m.total_pnl
    );
    println!(
        "  Total Return:            {:.2}%",
        m.total_return_pct
    );
    println!("───────────────────────────────────────────────────────────────────────");
    println!("  Total Trades:            {}", m.total_trades);
    println!(
        "  Winners:                 {} ({:.1}%)",
        m.num_winners, m.win_rate * 100.0
    );
    println!(
        "  Losers:                  {} ({:.1}%)",
        m.num_losers,
        if m.total_trades > 0 {
            (m.num_losers as f64 / m.total_trades as f64) * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Profit Factor:           {:.2}",
        m.profit_factor
    );
    println!("───────────────────────────────────────────────────────────────────────");
    println!(
        "  Avg Win:                 ¥{:.2}",
        m.avg_win
    );
    println!(
        "  Avg Loss:                ¥{:.2}",
        m.avg_loss
    );
    println!(
        "  Max Consecutive Losses:  {}",
        m.max_consecutive_losses
    );
    println!("───────────────────────────────────────────────────────────────────────");
    println!(
        "  Sharpe Ratio (annual):   {:.2}",
        m.sharpe_ratio
    );
    println!(
        "  Sortino Ratio (annual):  {:.2}",
        m.sortino_ratio
    );
    println!(
        "  Max Drawdown:            {:.2}%",
        m.max_drawdown_pct * 100.0
    );
}

/// Print a summary of trades (top 10 by PnL).
fn print_trade_summary(log: &TradeLog) {
    println!("  TOP TRADES (by absolute PnL):");
    println!("  ─────────────────────────────────────────────────────────────────");
    println!(
        "  {:>3}  {:>10}  {:>10}  {:>10}  {:>8}  {:>6}  Signal",
        "#", "Entry", "Exit", "PnL", "PnL%", "Dir"
    );
    println!("  ─────────────────────────────────────────────────────────────────");

    let mut sorted_trades: Vec<&TradeRecord> = log.trades().iter().collect();
    sorted_trades.sort_by(|a, b| {
        b.realized_pnl
            .abs()
            .partial_cmp(&a.realized_pnl.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, trade) in sorted_trades.iter().take(10).enumerate() {
        let entry_date = trade.entry_time.format("%Y-%m-%d");
        let exit_date = trade.exit_time.format("%Y-%m-%d");
        let pnl_sign = if trade.realized_pnl >= Decimal::ZERO {
            "+"
        } else {
            ""
        };
        println!(
            "  {:>3}  {:>10}  {:>10}  {:>+10.2}  {:>7.2}%  {:>6}  {:?}",
            i + 1,
            entry_date,
            exit_date,
            trade.realized_pnl,
            trade.pnl_pct(),
            format!("{:?}", trade.direction),
            trade.signal_type,
        );
        let _ = pnl_sign; // suppress unused warning
    }
}

/// Write backtest results to a JSON file.
fn write_json_output(
    path: &str,
    security: &brooks_core::market::SecurityId,
    result: &BacktestResult,
) -> Result<()> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct JsonReport {
        security: String,
        initial_capital: String,
        final_equity: String,
        total_pnl: String,
        total_return_pct: String,
        max_drawdown_pct: f64,
        sharpe_ratio: f64,
        sortino_ratio: f64,
        win_rate: f64,
        profit_factor: f64,
        total_trades: usize,
        num_winners: usize,
        num_losers: usize,
        avg_win: String,
        avg_loss: String,
        max_consecutive_losses: usize,
    }

    let m = &result.metrics;
    let report = JsonReport {
        security: security.to_string(),
        initial_capital: m.initial_capital.to_string(),
        final_equity: m.final_equity.to_string(),
        total_pnl: m.total_pnl.to_string(),
        total_return_pct: m.total_return_pct.to_string(),
        max_drawdown_pct: m.max_drawdown_pct,
        sharpe_ratio: m.sharpe_ratio,
        sortino_ratio: m.sortino_ratio,
        win_rate: m.win_rate,
        profit_factor: m.profit_factor,
        total_trades: m.total_trades,
        num_winners: m.num_winners,
        num_losers: m.num_losers,
        avg_win: m.avg_win.to_string(),
        avg_loss: m.avg_loss.to_string(),
        max_consecutive_losses: m.max_consecutive_losses,
    };

    let json = serde_json::to_string_pretty(&report)
        .context("failed to serialize backtest report")?;
    std::fs::write(path, json).context("failed to write JSON output file")?;

    Ok(())
}
