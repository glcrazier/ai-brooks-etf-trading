use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use rust_decimal::Decimal;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use brooks_china_market::rules::ChinaMarketRules;
use brooks_china_market::session::TradingSession;
use brooks_core::event::MarketEvent;
use brooks_core::market::{Exchange, SecurityId};
use brooks_market_data::{FutuMarketDataProvider, MarketDataProvider};
use brooks_strategy::{BrooksStrategy, Strategy, StrategyAction};

use crate::dto::{PaperTradingStartRequest, PaperTradingStatusResponse, PaperTradingStopResponse};
use crate::error::ApiError;
use crate::session::{PaperTradingSession, SessionStatus};
use crate::state::AppState;

/// POST /api/trading/start — start a paper trading session.
pub async fn start_paper_trading(
    State(state): State<AppState>,
    Json(req): Json<PaperTradingStartRequest>,
) -> Result<Json<PaperTradingStatusResponse>, ApiError> {
    let config = state.config.clone();

    let exchange = match config.market.exchange.to_uppercase().as_str() {
        "SH" => Exchange::SH,
        "SZ" => Exchange::SZ,
        other => return Err(ApiError::BadRequest(format!("unknown exchange '{}'", other))),
    };

    let security_codes = req
        .securities
        .clone()
        .unwrap_or_else(|| config.market.securities.clone());

    if security_codes.is_empty() {
        return Err(ApiError::BadRequest("no securities specified".to_string()));
    }

    let mut strategy_config = config.strategy.clone();
    if let Some(capital) = req.capital {
        strategy_config.risk.initial_capital =
            Decimal::try_from(capital).unwrap_or(strategy_config.risk.initial_capital);
    }

    let capital = strategy_config.risk.initial_capital;
    let session_id = Uuid::new_v4().to_string();

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let session = Arc::new(RwLock::new(PaperTradingSession {
        status: SessionStatus::Running,
        securities: security_codes.clone(),
        capital,
        cancel_token: cancel_tx,
        error: None,
    }));

    {
        let mut sessions = state.paper_sessions.write().await;
        sessions.insert(session_id.clone(), session.clone());
    }

    let sid = session_id.clone();
    let futu_config = config.futu.clone();

    // Spawn the event loop
    tokio::spawn(async move {
        let result = run_paper_loop(
            futu_config,
            strategy_config,
            &security_codes,
            exchange,
            cancel_rx,
        )
        .await;

        let mut sess = session.write().await;
        match result {
            Ok(()) => {
                sess.status = SessionStatus::Stopped;
                info!(session_id = %sid, "Paper trading stopped");
            }
            Err(e) => {
                sess.status = SessionStatus::Failed;
                sess.error = Some(e.to_string());
                error!(session_id = %sid, error = %e, "Paper trading failed");
            }
        }
    });

    Ok(Json(PaperTradingStatusResponse {
        session_id,
        status: "running".to_string(),
        securities: req
            .securities
            .unwrap_or_else(|| config.market.securities.clone()),
        capital,
        timestamp: Utc::now().to_rfc3339(),
    }))
}

async fn run_paper_loop(
    futu_config: brooks_market_data::FutuConfig,
    strategy_config: brooks_strategy::StrategyConfig,
    security_codes: &[String],
    exchange: Exchange,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), anyhow::Error> {
    use anyhow::Context;

    let securities: Vec<SecurityId> = security_codes
        .iter()
        .map(|code| SecurityId::etf(code.trim(), exchange))
        .collect();

    let primary_tf = strategy_config.primary_timeframe;

    let timeframes = vec![primary_tf];

    let provider = FutuMarketDataProvider::connect(futu_config)
        .await
        .context("failed to connect to FutuOpenD")?;

    let mut rx = provider
        .subscribe(&securities, &timeframes)
        .await
        .context("failed to subscribe to market data")?;

    let session = TradingSession::china_a_share();
    let market_rules = Box::new(ChinaMarketRules);
    let mut strategy = BrooksStrategy::new(strategy_config, session, market_rules);

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                match &event {
                    MarketEvent::BarUpdate { security, bar, timeframe } => {
                        info!(
                            "[{}] {} bar: O={} H={} L={} C={} V={}",
                            security, timeframe, bar.open, bar.high, bar.low, bar.close, bar.volume
                        );
                        match strategy.on_event(&event) {
                            Ok(actions) => {
                                for action in &actions {
                                    log_action(action);
                                }
                            }
                            Err(e) => {
                                error!("Strategy error: {}", e);
                            }
                        }
                    }
                    MarketEvent::SessionOpen { .. } | MarketEvent::SessionClose { .. } => {
                        if let Err(e) = strategy.on_event(&event) {
                            warn!("Strategy error on session event: {}", e);
                        }
                    }
                    _ => {
                        info!("Event: {:?}", event);
                    }
                }
            }
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    info!("Paper trading cancelled by user");
                    break;
                }
            }
        }
    }

    Ok(())
}

fn log_action(action: &StrategyAction) {
    match action {
        StrategyAction::SubmitOrder { order, signal } => {
            info!(
                "ORDER: {:?} {} x {} @ {:?} | signal={:?} conf={:.2}",
                order.direction, order.security, order.quantity, order.order_type,
                signal.signal_type, signal.confidence,
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
        StrategyAction::ModifyOrder { order_id, new_price, new_stop } => {
            info!("MODIFY: {:?} price={:?} stop={:?}", order_id, new_price, new_stop);
        }
    }
}

/// GET /api/trading/{session_id}/status
pub async fn trading_status(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<PaperTradingStatusResponse>, ApiError> {
    let sessions = state.paper_sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("trading session '{}' not found", session_id)))?;

    let sess = session.read().await;
    Ok(Json(PaperTradingStatusResponse {
        session_id,
        status: sess.status.to_string(),
        securities: sess.securities.clone(),
        capital: sess.capital,
        timestamp: Utc::now().to_rfc3339(),
    }))
}

/// POST /api/trading/{session_id}/stop — stop a paper trading session.
pub async fn stop_paper_trading(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<PaperTradingStopResponse>, ApiError> {
    let sessions = state.paper_sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("trading session '{}' not found", session_id)))?;

    let sess = session.read().await;
    if sess.status != SessionStatus::Running {
        return Err(ApiError::Conflict(format!(
            "session '{}' is not running (status: {})",
            session_id, sess.status
        )));
    }

    // Signal the cancel token
    let _ = sess.cancel_token.send(true);

    Ok(Json(PaperTradingStopResponse {
        session_id,
        status: "stopping".to_string(),
        message: "Paper trading session stop signal sent".to_string(),
    }))
}
