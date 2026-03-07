mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

use cli::{Args, Commands};
use commands::{init_logging, load_config};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config = load_config(&args)?;

    // Initialize logging
    init_logging(&config.logging, args.log_level.as_deref());

    // Dispatch to the appropriate command
    let mut config = config;
    match args.command {
        Commands::Backtest(ref bt_args) => {
            commands::backtest::run(bt_args, &mut config)?;
        }
        Commands::Paper(ref paper_args) => {
            commands::paper::run(paper_args, &mut config).await?;
        }
        Commands::FetchData(ref fd_args) => {
            commands::fetch_data::run(fd_args, &config).await?;
        }
        Commands::Analyze(ref analyze_args) => {
            commands::analyze::run(analyze_args, &config)?;
        }
        Commands::ValidateConfig => {
            commands::config_cmd::validate(&config)?;
        }
        Commands::ShowConfig { ref section } => {
            commands::config_cmd::show(&config, section.as_deref())?;
        }
    }

    Ok(())
}
