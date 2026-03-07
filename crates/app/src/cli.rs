use clap::{Parser, Subcommand};

/// Brooks ETF Trading System - Al Brooks Price Action for Chinese markets
#[derive(Parser, Debug)]
#[command(name = "brooks-trader")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/default.toml")]
    pub config: String,

    /// Log level override (trace, debug, info, warn, error)
    #[arg(long)]
    pub log_level: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run a historical backtest
    Backtest(BacktestArgs),

    /// Start paper trading with live Futu data
    Paper(PaperArgs),

    /// Fetch historical data from Futu
    FetchData(FetchDataArgs),

    /// Run price action analysis on historical data
    Analyze(AnalyzeArgs),

    /// Validate a configuration file
    ValidateConfig,

    /// Display the loaded configuration
    ShowConfig {
        /// Filter by section name (futu, market, strategy, backtest, logging)
        #[arg(long)]
        section: Option<String>,
    },
}

/// Arguments for the backtest command
#[derive(Parser, Debug)]
pub struct BacktestArgs {
    /// Override start date (YYYY-MM-DD)
    #[arg(long)]
    pub start_date: Option<String>,

    /// Override end date (YYYY-MM-DD)
    #[arg(long)]
    pub end_date: Option<String>,

    /// Securities to backtest (comma-separated, e.g. "510050,510300")
    #[arg(long)]
    pub securities: Option<String>,

    /// Override initial capital
    #[arg(long)]
    pub capital: Option<f64>,

    /// Override primary timeframe (1min, 5min, 15min, 30min, 60min, daily, weekly)
    #[arg(long)]
    pub timeframe: Option<String>,

    /// Write results to JSON file
    #[arg(long)]
    pub output_file: Option<String>,

    /// Load bars from a CSV file instead of Futu
    #[arg(long)]
    pub data_file: Option<String>,
}

/// Arguments for the paper trading command
#[derive(Parser, Debug)]
pub struct PaperArgs {
    /// Securities to trade (comma-separated)
    #[arg(long)]
    pub securities: Option<String>,

    /// Override initial capital
    #[arg(long)]
    pub capital: Option<f64>,
}

/// Arguments for the fetch-data command
#[derive(Parser, Debug)]
pub struct FetchDataArgs {
    /// Securities to fetch (comma-separated, e.g. "510050,510300")
    #[arg(long)]
    pub securities: Option<String>,

    /// Timeframe (1min, 5min, 15min, 30min, 60min, daily, weekly)
    #[arg(long, default_value = "5min")]
    pub timeframe: String,

    /// Start date (YYYY-MM-DD)
    #[arg(long)]
    pub start_date: String,

    /// End date (YYYY-MM-DD)
    #[arg(long)]
    pub end_date: String,

    /// Output directory for CSV files
    #[arg(long, default_value = "./data")]
    pub output_dir: String,
}

/// Arguments for the analyze command
#[derive(Parser, Debug)]
pub struct AnalyzeArgs {
    /// Load bars from a CSV file
    #[arg(long)]
    pub data_file: Option<String>,

    /// Security code (e.g. "510050")
    #[arg(long)]
    pub security: Option<String>,

    /// Timeframe (1min, 5min, 15min, 30min, 60min, daily, weekly)
    #[arg(long, default_value = "5min")]
    pub timeframe: String,

    /// Output format: "text" or "json"
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses() {
        // Verify the CLI definition is valid
        Args::command().debug_assert();
    }

    #[test]
    fn test_backtest_subcommand() {
        let args = Args::parse_from([
            "brooks-trader",
            "--config",
            "test.toml",
            "backtest",
            "--start-date",
            "2024-01-01",
            "--end-date",
            "2024-06-30",
        ]);
        assert_eq!(args.config, "test.toml");
        match args.command {
            Commands::Backtest(bt) => {
                assert_eq!(bt.start_date.as_deref(), Some("2024-01-01"));
                assert_eq!(bt.end_date.as_deref(), Some("2024-06-30"));
            }
            _ => panic!("expected Backtest command"),
        }
    }

    #[test]
    fn test_validate_config_subcommand() {
        let args = Args::parse_from(["brooks-trader", "validate-config"]);
        assert!(matches!(args.command, Commands::ValidateConfig));
    }

    #[test]
    fn test_show_config_with_section() {
        let args = Args::parse_from(["brooks-trader", "show-config", "--section", "strategy"]);
        match args.command {
            Commands::ShowConfig { section } => {
                assert_eq!(section.as_deref(), Some("strategy"));
            }
            _ => panic!("expected ShowConfig command"),
        }
    }

    #[test]
    fn test_fetch_data_defaults() {
        let args = Args::parse_from([
            "brooks-trader",
            "fetch-data",
            "--start-date",
            "2024-01-01",
            "--end-date",
            "2024-12-31",
        ]);
        match args.command {
            Commands::FetchData(fd) => {
                assert_eq!(fd.timeframe, "5min");
                assert_eq!(fd.output_dir, "./data");
            }
            _ => panic!("expected FetchData command"),
        }
    }
}
