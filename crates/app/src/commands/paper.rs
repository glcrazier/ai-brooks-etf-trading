use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::str::FromStr;
use tracing::{error, info, warn};

use brooks_china_market::rules::ChinaMarketRules;
use brooks_china_market::session::TradingSession;
use brooks_core::event::MarketEvent;
use brooks_market_data::{FutuMarketDataProvider, MarketDataProvider};
use brooks_strategy::{BrooksStrategy, Strategy, StrategyAction};

use crate::cli::PaperArgs;
use crate::commands::{parse_exchange, parse_securities, parse_timeframe, AppConfig};

/// Run the paper trading command.
pub async fn run(args: &PaperArgs, config: &mut AppConfig) -> Result<()> {
    // Apply CLI overrides
    if let Some(capital) = args.capital {
        config.strategy.risk.initial_capital =
            Decimal::from_str(&format!("{:.2}", capital)).unwrap_or(config.strategy.risk.initial_capital);
    }

    let exchange = parse_exchange(&config.market.exchange)?;

    // Determine securities to trade
    let security_codes: Vec<String> = if let Some(ref s) = args.securities {
        s.split(',').map(|c| c.trim().to_string()).collect()
    } else {
        config.market.securities.clone()
    };
    let securities = parse_securities(&security_codes, exchange);

    let primary_tf = parse_timeframe(&config.market.primary_timeframe)?;
    let timeframes = vec![primary_tf];

    info!(
        "Starting paper trading: {} securities, timeframe={}",
        securities.len(),
        primary_tf,
    );
    info!("Connecting to FutuOpenD at {}:{} ...", config.futu.host, config.futu.port);

    // Connect to Futu
    let provider = FutuMarketDataProvider::connect(config.futu.clone())
        .await
        .context("failed to connect to FutuOpenD")?;

    info!("Connected. Subscribing to market data ...");

    // Subscribe to bar updates
    let mut rx = provider
        .subscribe(&securities, &timeframes)
        .await
        .context("failed to subscribe to market data")?;

    // Create strategy
    let session = TradingSession::china_a_share();
    let market_rules = Box::new(ChinaMarketRules);
    let mut strategy = BrooksStrategy::new(config.strategy.clone(), session, market_rules);

    println!();
    println!("═══════════════════════════════════════════════════════════════════════");
    println!("  PAPER TRADING ACTIVE");
    println!("  Securities: {:?}", security_codes);
    println!("  Capital: ¥{:.2}", config.strategy.risk.initial_capital);
    println!("  Press Ctrl+C to stop");
    println!("═══════════════════════════════════════════════════════════════════════");
    println!();

    // Main event loop
    while let Some(event) = rx.recv().await {
        match &event {
            MarketEvent::BarUpdate {
                security,
                bar,
                timeframe,
            } => {
                info!(
                    "[{}] {} bar: O={} H={} L={} C={} V={}",
                    security, timeframe, bar.open, bar.high, bar.low, bar.close, bar.volume
                );

                // Forward to strategy
                match strategy.on_event(&event) {
                    Ok(actions) => {
                        for action in &actions {
                            handle_action(action);
                        }
                    }
                    Err(e) => {
                        error!("Strategy error on bar update: {}", e);
                    }
                }
            }
            MarketEvent::SessionOpen { exchange } => {
                info!("Session opened: {:?}", exchange);
                if let Err(e) = strategy.on_event(&event) {
                    warn!("Strategy error on session open: {}", e);
                }
            }
            MarketEvent::SessionClose { exchange } => {
                info!("Session closed: {:?}", exchange);
                if let Err(e) = strategy.on_event(&event) {
                    warn!("Strategy error on session close: {}", e);
                }
            }
            _ => {
                // TickUpdate, SessionBreak events
                info!("Event: {:?}", event);
            }
        }
    }

    info!("Market data stream ended. Paper trading stopped.");
    Ok(())
}

/// Handle a strategy action during paper trading.
fn handle_action(action: &StrategyAction) {
    match action {
        StrategyAction::SubmitOrder { order, signal } => {
            info!(
                "ORDER: {:?} {} x {} @ {:?} | signal={:?} conf={:.2}",
                order.direction,
                order.security,
                order.quantity,
                order.order_type,
                signal.signal_type,
                signal.confidence,
            );
        }
        StrategyAction::CancelOrder { order_id, reason } => {
            info!("CANCEL: {:?} - {}", order_id, reason);
        }
        StrategyAction::ClosePosition { security, reason } => {
            info!("CLOSE: {} - {}", security, reason);
        }
        StrategyAction::UpdateStopLoss { security, new_stop } => {
            info!("STOP UPDATE: {} -> {}", security, new_stop);
        }
        StrategyAction::ModifyOrder {
            order_id,
            new_price,
            new_stop,
        } => {
            info!(
                "MODIFY: {:?} price={:?} stop={:?}",
                order_id, new_price, new_stop
            );
        }
    }
}
