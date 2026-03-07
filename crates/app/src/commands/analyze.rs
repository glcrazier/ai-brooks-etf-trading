use anyhow::{Context, Result};
use tracing::info;

use brooks_core::market::SecurityId;
use brooks_pa_engine::analyzer::PriceActionAnalyzer;

use crate::cli::AnalyzeArgs;
use crate::commands::{parse_exchange, parse_timeframe, AppConfig};

/// Run the analyze command.
pub fn run(args: &AnalyzeArgs, config: &AppConfig) -> Result<()> {
    let timeframe = parse_timeframe(&args.timeframe)?;
    let exchange = parse_exchange(&config.market.exchange)?;

    // Determine the security to analyze
    let security = if let Some(ref code) = args.security {
        SecurityId::etf(code.trim(), exchange)
    } else if let Some(first) = config.market.securities.first() {
        SecurityId::etf(first.trim(), exchange)
    } else {
        anyhow::bail!("no security specified: use --security or configure [market].securities");
    };

    info!("Analyzing {} at {} timeframe", security, timeframe);

    // Load bars
    let bars = if let Some(ref data_file) = args.data_file {
        crate::commands::backtest::load_bars_from_csv_public(data_file)
            .with_context(|| format!("failed to load bars from {}", data_file))?
    } else {
        anyhow::bail!(
            "No data source specified. Use --data-file to provide CSV data."
        );
    };

    if bars.is_empty() {
        anyhow::bail!("No bars loaded — cannot run analysis.");
    }

    info!("Loaded {} bars", bars.len());

    // Create PA analyzer
    let pa_config = config.strategy.pa.to_pa_config();
    let mut analyzer = PriceActionAnalyzer::new(pa_config);

    // Feed all bars through the analyzer
    for bar in &bars {
        analyzer.process_bar(bar);
    }

    // Get the analysis context
    let ctx = analyzer.current_context();

    // Output results
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(ctx)
                .context("failed to serialize analysis context")?;
            println!("{}", json);
        }
        _ => {
            print_analysis(&security, timeframe, bars.len(), ctx);
        }
    }

    Ok(())
}

/// Pretty-print analysis results.
fn print_analysis(
    security: &SecurityId,
    timeframe: brooks_core::timeframe::Timeframe,
    bar_count: usize,
    ctx: &brooks_pa_engine::context::MarketContext,
) {
    println!();
    println!("═══════════════════════════════════════════════════════════════════════");
    println!("  PRICE ACTION ANALYSIS: {} ({})", security, timeframe);
    println!("═══════════════════════════════════════════════════════════════════════");
    println!();
    println!("  Bars Analyzed:           {}", bar_count);
    println!("  Trend State:             {:?}", ctx.trend);
    println!("  Trend Strength:          {:.2}", ctx.trend_strength);
    println!(
        "  Consecutive Bull Bars:   {}",
        ctx.consecutive_bull_bars
    );
    println!(
        "  Consecutive Bear Bars:   {}",
        ctx.consecutive_bear_bars
    );
    println!("  Is Climax:               {}", ctx.is_climax);

    if let Some(ref level) = ctx.nearest_support {
        println!(
            "  Nearest Support:         {} (strength={})",
            level.price, level.strength
        );
    } else {
        println!("  Nearest Support:         none");
    }

    if let Some(ref level) = ctx.nearest_resistance {
        println!(
            "  Nearest Resistance:      {} (strength={})",
            level.price, level.strength
        );
    } else {
        println!("  Nearest Resistance:      none");
    }

    println!();

    // Print recent bar classifications
    if !ctx.bar_classifications.is_empty() {
        println!("  RECENT BAR CLASSIFICATIONS (last {}):", ctx.bar_classifications.len());
        println!("  ─────────────────────────────────────────────────────────────────");
        for (i, bc) in ctx.bar_classifications.iter().rev().take(10).enumerate() {
            println!(
                "    {:>2}. {:?}",
                i + 1,
                bc,
            );
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════════════════");
}
