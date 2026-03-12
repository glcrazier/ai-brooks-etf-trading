use axum::extract::State;
use axum::Json;

use crate::dto::{HealthResponse, InfoResponse};
use crate::state::AppState;

/// GET /api/health
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        uptime_secs: state.start_time.elapsed().as_secs(),
    })
}

/// GET /api/info
pub async fn info() -> Json<InfoResponse> {
    Json(InfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: vec![
            "backtest".to_string(),
            "paper-trading".to_string(),
            "fetch-data".to_string(),
            "analyze".to_string(),
            "config".to_string(),
        ],
    })
}
