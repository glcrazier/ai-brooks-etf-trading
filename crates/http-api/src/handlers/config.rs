use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;

use crate::dto::ConfigValidationResponse;
use crate::error::ApiError;
use crate::state::AppState;

/// GET /api/config — return current configuration as JSON.
pub async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let config = &state.config;
    let json = serde_json::json!({
        "futu": {
            "host": config.futu.host,
            "port": config.futu.port,
            "client_id": config.futu.client_id,
            "timeout_ms": config.futu.timeout_ms,
        },
        "market": {
            "exchange": config.market.exchange,
            "securities": config.market.securities,
            "primary_timeframe": config.market.primary_timeframe,
            "context_timeframe": config.market.context_timeframe,
        },
        "strategy": {
            "name": config.strategy.name,
            "warm_up_bars": config.strategy.warm_up_bars,
            "risk": {
                "max_risk_per_trade_pct": config.strategy.risk.max_risk_per_trade_pct,
                "max_daily_loss_pct": config.strategy.risk.max_daily_loss_pct,
                "max_open_positions": config.strategy.risk.max_open_positions,
                "initial_capital": config.strategy.risk.initial_capital,
                "min_reward_risk_ratio": config.strategy.risk.min_reward_risk_ratio,
            },
            "pa": {
                "ema_period": config.strategy.pa.ema_period,
                "swing_lookback": config.strategy.pa.swing_lookback,
                "doji_threshold": config.strategy.pa.doji_threshold,
                "sr_cluster_tolerance": config.strategy.pa.sr_cluster_tolerance,
            },
        },
        "backtest": {
            "start_date": config.backtest.start_date,
            "end_date": config.backtest.end_date,
            "fill_model": format!("{:?}", config.backtest.fill_model),
            "slippage_bps": config.backtest.slippage_bps,
            "initial_capital": config.backtest.initial_capital,
        },
        "http_server": {
            "host": config.http_server.host,
            "port": config.http_server.port,
        },
    });

    Ok(Json(json))
}

/// GET /api/config/validate — validate the loaded configuration.
pub async fn validate_config(
    State(state): State<AppState>,
) -> Json<ConfigValidationResponse> {
    let config = &state.config;
    let mut errors: Vec<String> = Vec::new();

    // Validate [futu]
    if config.futu.host.is_empty() {
        errors.push("[futu] host is empty".to_string());
    }
    if config.futu.port == 0 {
        errors.push("[futu] port cannot be 0".to_string());
    }

    // Validate [market]
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

    // Validate [strategy.risk]
    if config.strategy.risk.max_risk_per_trade_pct <= Decimal::ZERO {
        errors.push("[strategy.risk] max_risk_per_trade_pct must be > 0".to_string());
    }
    if config.strategy.risk.max_daily_loss_pct <= Decimal::ZERO {
        errors.push("[strategy.risk] max_daily_loss_pct must be > 0".to_string());
    }
    if config.strategy.risk.max_open_positions == 0 {
        errors.push("[strategy.risk] max_open_positions must be > 0".to_string());
    }
    if config.strategy.risk.initial_capital <= Decimal::ZERO {
        errors.push("[strategy.risk] initial_capital must be > 0".to_string());
    }

    // Validate [strategy.pa]
    if config.strategy.pa.ema_period == 0 {
        errors.push("[strategy.pa] ema_period must be > 0".to_string());
    }
    if config.strategy.pa.swing_lookback == 0 {
        errors.push("[strategy.pa] swing_lookback must be > 0".to_string());
    }

    // Validate [backtest]
    if config.backtest.start_date.is_empty() {
        errors.push("[backtest] start_date is empty".to_string());
    }
    if config.backtest.end_date.is_empty() {
        errors.push("[backtest] end_date is empty".to_string());
    }

    Json(ConfigValidationResponse {
        valid: errors.is_empty(),
        errors,
    })
}
