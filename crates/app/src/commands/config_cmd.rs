use anyhow::Result;

use crate::commands::AppConfig;

/// Run the validate-config command.
pub fn validate(config: &AppConfig) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    // Validate [futu] section
    if config.futu.host.is_empty() {
        errors.push("[futu] host is empty".to_string());
    }
    if config.futu.port == 0 {
        errors.push("[futu] port cannot be 0".to_string());
    }

    // Validate [market] section
    if config.market.securities.is_empty() {
        errors.push("[market] securities list is empty".to_string());
    }
    if config.market.exchange.is_empty() {
        errors.push("[market] exchange is empty".to_string());
    }
    if !["SH", "SZ"].contains(&config.market.exchange.to_uppercase().as_str()) {
        errors.push(format!(
            "[market] unknown exchange '{}': expected SH or SZ",
            config.market.exchange
        ));
    }

    // Validate [strategy.risk] section
    if config.strategy.risk.max_risk_per_trade_pct <= rust_decimal::Decimal::ZERO {
        errors.push("[strategy.risk] max_risk_per_trade_pct must be > 0".to_string());
    }
    if config.strategy.risk.max_daily_loss_pct <= rust_decimal::Decimal::ZERO {
        errors.push("[strategy.risk] max_daily_loss_pct must be > 0".to_string());
    }
    if config.strategy.risk.max_open_positions == 0 {
        errors.push("[strategy.risk] max_open_positions must be > 0".to_string());
    }
    if config.strategy.risk.initial_capital <= rust_decimal::Decimal::ZERO {
        errors.push("[strategy.risk] initial_capital must be > 0".to_string());
    }

    // Validate [strategy.pa] section
    if config.strategy.pa.ema_period == 0 {
        errors.push("[strategy.pa] ema_period must be > 0".to_string());
    }
    if config.strategy.pa.swing_lookback == 0 {
        errors.push("[strategy.pa] swing_lookback must be > 0".to_string());
    }

    // Validate [backtest] section
    if config.backtest.start_date.is_empty() {
        errors.push("[backtest] start_date is empty".to_string());
    }
    if config.backtest.end_date.is_empty() {
        errors.push("[backtest] end_date is empty".to_string());
    }

    // Report results
    if errors.is_empty() {
        println!("Configuration is valid.");
        Ok(())
    } else {
        println!("Configuration errors found:\n");
        for (i, err) in errors.iter().enumerate() {
            println!("  {}. {}", i + 1, err);
        }
        println!();
        anyhow::bail!("{} validation error(s) found", errors.len());
    }
}

/// Run the show-config command.
pub fn show(config: &AppConfig, section: Option<&str>) -> Result<()> {
    match section {
        Some("futu") => {
            println!("[futu]");
            println!("  host          = \"{}\"", config.futu.host);
            println!("  port          = {}", config.futu.port);
            println!("  client_id     = \"{}\"", config.futu.client_id);
            println!("  timeout_ms    = {}", config.futu.timeout_ms);
        }
        Some("market") => {
            println!("[market]");
            println!("  exchange          = \"{}\"", config.market.exchange);
            println!("  securities        = {:?}", config.market.securities);
            println!("  primary_timeframe = \"{}\"", config.market.primary_timeframe);
            println!("  context_timeframe = \"{}\"", config.market.context_timeframe);
        }
        Some("strategy") => {
            println!("[strategy]");
            println!("  name          = \"{}\"", config.strategy.name);
            println!("  warm_up_bars  = {}", config.strategy.warm_up_bars);
            println!();
            println!("[strategy.risk]");
            println!(
                "  max_risk_per_trade_pct = {}",
                config.strategy.risk.max_risk_per_trade_pct
            );
            println!(
                "  max_daily_loss_pct     = {}",
                config.strategy.risk.max_daily_loss_pct
            );
            println!(
                "  max_open_positions     = {}",
                config.strategy.risk.max_open_positions
            );
            println!(
                "  initial_capital        = {}",
                config.strategy.risk.initial_capital
            );
            println!(
                "  min_reward_risk_ratio  = {}",
                config.strategy.risk.min_reward_risk_ratio
            );
            println!();
            println!("[strategy.pa]");
            println!("  ema_period            = {}", config.strategy.pa.ema_period);
            println!(
                "  swing_lookback        = {}",
                config.strategy.pa.swing_lookback
            );
            println!(
                "  doji_threshold        = {}",
                config.strategy.pa.doji_threshold
            );
            println!(
                "  sr_cluster_tolerance  = {}",
                config.strategy.pa.sr_cluster_tolerance
            );
        }
        Some("backtest") => {
            println!("[backtest]");
            println!("  start_date    = \"{}\"", config.backtest.start_date);
            println!("  end_date      = \"{}\"", config.backtest.end_date);
            println!("  fill_model    = {:?}", config.backtest.fill_model);
            println!("  slippage_bps  = {}", config.backtest.slippage_bps);
            println!("  initial_capital = {}", config.backtest.initial_capital);
        }
        Some("logging") => {
            println!("[logging]");
            println!("  level   = \"{}\"", config.logging.level);
            println!("  format  = \"{}\"", config.logging.format);
            println!(
                "  file    = {}",
                config
                    .logging
                    .file
                    .as_deref()
                    .map_or("(none)".to_string(), |f| format!("\"{}\"", f))
            );
        }
        Some(other) => {
            anyhow::bail!(
                "unknown section '{}': expected one of futu, market, strategy, backtest, logging",
                other
            );
        }
        None => {
            // Show all sections
            show(config, Some("futu"))?;
            println!();
            show(config, Some("market"))?;
            println!();
            show(config, Some("strategy"))?;
            println!();
            show(config, Some("backtest"))?;
            println!();
            show(config, Some("logging"))?;
        }
    }

    Ok(())
}
