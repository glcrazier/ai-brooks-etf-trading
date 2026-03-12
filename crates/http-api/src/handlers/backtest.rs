use std::sync::Arc;

use anyhow::Context;
use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use rust_decimal::Decimal;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

use brooks_backtester::{BacktestEngine, BacktestResult};
use brooks_china_market::rules::ChinaMarketRules;
use brooks_china_market::session::TradingSession;
use brooks_core::bar::Bar;
use brooks_core::market::{Exchange, SecurityId};
use brooks_core::timeframe::Timeframe;
use brooks_market_data::VecDataFeed;
use brooks_strategy::BrooksStrategy;

use crate::dto::{
    BacktestJobResponse, BacktestMetricsDto, BacktestResultsResponse, BacktestStatusResponse,
    EquityPointDto, TradeRecordDto,
};
use crate::error::ApiError;
use crate::session::{BacktestSession, SessionStatus};
use crate::state::AppState;

/// POST /api/backtest/run — submit a new backtest job.
pub async fn run_backtest(
    State(state): State<AppState>,
    Json(req): Json<crate::dto::BacktestRequest>,
) -> Result<Json<BacktestJobResponse>, ApiError> {
    // Validate data_file exists
    if !std::path::Path::new(&req.data_file).exists() {
        return Err(ApiError::BadRequest(format!(
            "data file not found: {}",
            req.data_file
        )));
    }

    let session_id = Uuid::new_v4().to_string();

    // Create session in "running" state
    let session = Arc::new(RwLock::new(BacktestSession {
        status: SessionStatus::Running,
        metrics: None,
        trades: Vec::new(),
        equity_curve: Vec::new(),
        error: None,
    }));

    // Store session
    {
        let mut sessions = state.backtest_sessions.write().await;
        sessions.insert(session_id.clone(), session.clone());
    }

    // Clone data for the background task
    let config = state.config.clone();
    let sid = session_id.clone();
    let data_file = req.data_file.clone();

    // Spawn background task to run the backtest
    tokio::task::spawn_blocking(move || {
        let result = run_backtest_sync(&config, &req, &data_file);

        // We need a runtime handle to update the async session lock
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let mut sess = session.write().await;
            match result {
                Ok(bt_result) => {
                    sess.metrics = Some(bt_result.metrics);
                    sess.trades = bt_result.trade_log.trades().to_vec();
                    sess.equity_curve = bt_result.portfolio.equity_curve().to_vec();
                    sess.status = SessionStatus::Completed;
                    info!(session_id = %sid, "Backtest completed");
                }
                Err(e) => {
                    sess.status = SessionStatus::Failed;
                    sess.error = Some(e.to_string());
                    error!(session_id = %sid, error = %e, "Backtest failed");
                }
            }
        });
    });

    Ok(Json(BacktestJobResponse {
        session_id,
        status: "running".to_string(),
        timestamp: Utc::now().to_rfc3339(),
    }))
}

/// Run backtest synchronously (called from spawn_blocking).
fn run_backtest_sync(
    config: &crate::state::ServerAppConfig,
    req: &crate::dto::BacktestRequest,
    data_file: &str,
) -> Result<BacktestResult, anyhow::Error> {
    use std::str::FromStr;

    let exchange = parse_exchange(&config.market.exchange)?;

    let security_codes = req
        .securities
        .clone()
        .unwrap_or_else(|| config.market.securities.clone());

    let securities: Vec<SecurityId> = security_codes
        .iter()
        .map(|code| SecurityId::etf(code.trim(), exchange))
        .collect();

    let timeframe = parse_timeframe(
        req.timeframe
            .as_deref()
            .unwrap_or(&config.market.primary_timeframe),
    )?;

    let mut backtest_config = config.backtest.clone();
    let mut strategy_config = config.strategy.clone();

    if let Some(ref start) = req.start_date {
        backtest_config.start_date = start.clone();
    }
    if let Some(ref end) = req.end_date {
        backtest_config.end_date = end.clone();
    }
    if let Some(capital) = req.capital {
        backtest_config.initial_capital =
            Decimal::from_str(&format!("{:.2}", capital)).unwrap_or(backtest_config.initial_capital);
        strategy_config.risk.initial_capital = backtest_config.initial_capital;
    }

    // Use first security (or only one)
    let security = securities
        .first()
        .ok_or_else(|| anyhow::anyhow!("no securities specified"))?;

    // Load bars
    let bars = load_bars_from_csv(data_file, security, timeframe)
        .with_context(|| format!("failed to load bars from {}", data_file))?;

    let mut feed = VecDataFeed::new(bars);

    // Create strategy
    let session = TradingSession::china_a_share();
    let market_rules = Box::new(ChinaMarketRules);
    let mut strategy = BrooksStrategy::new(strategy_config, session, market_rules);

    // Run backtest
    let engine = BacktestEngine::new(backtest_config);
    let result = engine
        .run(&mut feed, &mut strategy, security, timeframe)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(result)
}

/// GET /api/backtest/{session_id}/status
pub async fn backtest_status(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<BacktestStatusResponse>, ApiError> {
    let sessions = state.backtest_sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("backtest session '{}' not found", session_id)))?;

    let sess = session.read().await;
    Ok(Json(BacktestStatusResponse {
        session_id,
        status: sess.status.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    }))
}

/// GET /api/backtest/{session_id}/results — full metrics + trades.
pub async fn backtest_results(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<BacktestResultsResponse>, ApiError> {
    let sessions = state.backtest_sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("backtest session '{}' not found", session_id)))?;

    let sess = session.read().await;
    Ok(Json(BacktestResultsResponse {
        session_id,
        status: sess.status.to_string(),
        metrics: sess.metrics.as_ref().map(BacktestMetricsDto::from),
        error: sess.error.clone(),
    }))
}

/// GET /api/backtest/{session_id}/equity-curve
pub async fn backtest_equity_curve(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<EquityPointDto>>, ApiError> {
    let sessions = state.backtest_sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("backtest session '{}' not found", session_id)))?;

    let sess = session.read().await;
    let points: Vec<EquityPointDto> = sess.equity_curve.iter().map(EquityPointDto::from).collect();
    Ok(Json(points))
}

/// GET /api/backtest/{session_id}/trades
pub async fn backtest_trades(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<TradeRecordDto>>, ApiError> {
    let sessions = state.backtest_sessions.read().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("backtest session '{}' not found", session_id)))?;

    let sess = session.read().await;
    let trades: Vec<TradeRecordDto> = sess.trades.iter().map(TradeRecordDto::from).collect();
    Ok(Json(trades))
}

// ─── Utility functions ──────────────────────────────────────────────────────

fn parse_exchange(s: &str) -> Result<Exchange, anyhow::Error> {
    match s.to_uppercase().as_str() {
        "SH" => Ok(Exchange::SH),
        "SZ" => Ok(Exchange::SZ),
        _ => Err(anyhow::anyhow!("unknown exchange '{}': expected SH or SZ", s)),
    }
}

fn parse_timeframe(s: &str) -> Result<Timeframe, anyhow::Error> {
    s.parse::<Timeframe>()
        .map_err(|e| anyhow::anyhow!(e))
}

/// Load bars from a CSV file.
fn load_bars_from_csv(
    path: &str,
    security: &SecurityId,
    timeframe: Timeframe,
) -> Result<Vec<Bar>, anyhow::Error> {
    use chrono::DateTime;

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read file: {}", path))?;

    let mut bars = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if i == 0 && line.contains("timestamp") {
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 6 {
            return Err(anyhow::anyhow!("line {}: expected 6 columns, got {}", i + 1, fields.len()));
        }

        let timestamp: DateTime<chrono::Utc> = fields[0]
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

        bars.push(Bar {
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
