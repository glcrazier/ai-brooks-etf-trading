//! Axum application builder — wires up all routes and middleware.

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

/// Build the Axum router with all API routes.
pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        // Health & Info
        .route("/health", get(handlers::health::health))
        .route("/info", get(handlers::health::info))
        // Config
        .route("/config", get(handlers::config::get_config))
        .route("/config/validate", get(handlers::config::validate_config))
        // Backtest
        .route("/backtest/run", post(handlers::backtest::run_backtest))
        .route(
            "/backtest/{session_id}/status",
            get(handlers::backtest::backtest_status),
        )
        .route(
            "/backtest/{session_id}/results",
            get(handlers::backtest::backtest_results),
        )
        .route(
            "/backtest/{session_id}/equity-curve",
            get(handlers::backtest::backtest_equity_curve),
        )
        .route(
            "/backtest/{session_id}/trades",
            get(handlers::backtest::backtest_trades),
        )
        // Analysis
        .route("/analysis/run", post(handlers::analysis::run_analysis))
        // Data Fetch
        .route("/data/fetch", post(handlers::data::fetch_data))
        .route("/data/{job_id}/status", get(handlers::data::fetch_status))
        // Paper Trading
        .route(
            "/trading/start",
            post(handlers::trading::start_paper_trading),
        )
        .route(
            "/trading/{session_id}/status",
            get(handlers::trading::trading_status),
        )
        .route(
            "/trading/{session_id}/stop",
            post(handlers::trading::stop_paper_trading),
        );

    Router::new()
        .nest("/api", api)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
