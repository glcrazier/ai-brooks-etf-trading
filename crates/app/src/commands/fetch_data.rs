use anyhow::{Context, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use tracing::info;

use brooks_market_data::{FutuMarketDataProvider, MarketDataProvider};

use crate::cli::FetchDataArgs;
use crate::commands::{parse_exchange, parse_securities, parse_timeframe, AppConfig};

/// Run the fetch-data command.
pub async fn run(args: &FetchDataArgs, config: &AppConfig) -> Result<()> {
    let exchange = parse_exchange(&config.market.exchange)?;
    let timeframe = parse_timeframe(&args.timeframe)?;

    // Determine securities
    let security_codes: Vec<String> = if let Some(ref s) = args.securities {
        s.split(',').map(|c| c.trim().to_string()).collect()
    } else {
        config.market.securities.clone()
    };
    let securities = parse_securities(&security_codes, exchange);

    // Parse dates
    let start_date = NaiveDate::parse_from_str(&args.start_date, "%Y-%m-%d")
        .with_context(|| format!("invalid start date '{}': expected YYYY-MM-DD", args.start_date))?;
    let end_date = NaiveDate::parse_from_str(&args.end_date, "%Y-%m-%d")
        .with_context(|| format!("invalid end date '{}': expected YYYY-MM-DD", args.end_date))?;

    let start = Utc
        .from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).unwrap());
    let end = Utc
        .from_utc_datetime(&end_date.and_hms_opt(23, 59, 59).unwrap());

    info!(
        "Fetching data: {} securities, timeframe={}, {} to {}",
        securities.len(),
        timeframe,
        args.start_date,
        args.end_date,
    );
    info!("Connecting to FutuOpenD at {}:{} ...", config.futu.host, config.futu.port);

    // Connect to Futu
    let provider = FutuMarketDataProvider::connect(config.futu.clone())
        .await
        .context("failed to connect to FutuOpenD")?;

    info!("Connected. Fetching historical data ...");

    // Create output directory
    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("cannot create output directory: {}", args.output_dir))?;

    for security in &securities {
        info!("Fetching {} ...", security);

        let bars = provider
            .fetch_historical_bars(security, timeframe, start, end, None)
            .await
            .with_context(|| format!("failed to fetch bars for {}", security))?;

        if bars.is_empty() {
            println!("  {}: no data returned", security);
            continue;
        }

        // Write to CSV
        let filename = format!(
            "{}/{}_{}.csv",
            args.output_dir,
            security.code,
            timeframe,
        );
        write_bars_csv(&filename, &bars)
            .with_context(|| format!("failed to write {}", filename))?;

        println!(
            "  {}: {} bars written to {}",
            security,
            bars.len(),
            filename,
        );
    }

    println!("\nData fetch complete.");
    Ok(())
}

/// Write bars to a CSV file.
fn write_bars_csv(path: &str, bars: &[brooks_core::bar::Bar]) -> Result<()> {
    let mut output = String::new();
    output.push_str("timestamp,open,high,low,close,volume\n");

    for bar in bars {
        output.push_str(&format!(
            "{},{},{},{},{},{}\n",
            bar.timestamp.to_rfc3339(),
            bar.open,
            bar.high,
            bar.low,
            bar.close,
            bar.volume,
        ));
    }

    std::fs::write(path, output).context("failed to write CSV file")?;
    Ok(())
}
