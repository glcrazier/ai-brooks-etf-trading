use std::sync::Arc;

use anyhow::Context;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{NaiveDate, TimeZone, Utc};
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

use brooks_core::market::{Exchange, SecurityId};
use brooks_core::timeframe::Timeframe;
use brooks_market_data::{FutuMarketDataProvider, MarketDataProvider};

use crate::dto::{DataFetchFileResult, DataFetchJobResponse, DataFetchRequest, DataFetchStatusResponse};
use crate::error::ApiError;
use crate::session::{DataFetchSession, SessionStatus};
use crate::state::AppState;

/// POST /api/data/fetch — start an async data fetch job.
pub async fn fetch_data(
    State(state): State<AppState>,
    Json(req): Json<DataFetchRequest>,
) -> Result<Json<DataFetchJobResponse>, ApiError> {
    // Validate dates
    NaiveDate::parse_from_str(&req.start_date, "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("invalid start_date '{}': expected YYYY-MM-DD", req.start_date)))?;
    NaiveDate::parse_from_str(&req.end_date, "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("invalid end_date '{}': expected YYYY-MM-DD", req.end_date)))?;

    // Validate timeframe
    let _tf: Timeframe = req
        .timeframe
        .parse()
        .map_err(|e: String| ApiError::BadRequest(format!("invalid timeframe: {}", e)))?;

    let job_id = Uuid::new_v4().to_string();

    let session = Arc::new(RwLock::new(DataFetchSession {
        status: SessionStatus::Running,
        files: Vec::new(),
        error: None,
    }));

    {
        let mut sessions = state.data_fetch_sessions.write().await;
        sessions.insert(job_id.clone(), session.clone());
    }

    let config = state.config.clone();
    let jid = job_id.clone();

    // Spawn async background task
    tokio::spawn(async move {
        let result = run_fetch_async(&config, &req).await;

        let mut sess = session.write().await;
        match result {
            Ok(files) => {
                sess.files = files;
                sess.status = SessionStatus::Completed;
                info!(job_id = %jid, "Data fetch completed");
            }
            Err(e) => {
                sess.status = SessionStatus::Failed;
                sess.error = Some(e.to_string());
                error!(job_id = %jid, error = %e, "Data fetch failed");
            }
        }
    });

    Ok(Json(DataFetchJobResponse {
        job_id,
        status: "running".to_string(),
        timestamp: Utc::now().to_rfc3339(),
    }))
}

async fn run_fetch_async(
    config: &crate::state::ServerAppConfig,
    req: &DataFetchRequest,
) -> Result<Vec<DataFetchFileResult>, anyhow::Error> {
    let exchange = match config.market.exchange.to_uppercase().as_str() {
        "SH" => Exchange::SH,
        "SZ" => Exchange::SZ,
        other => return Err(anyhow::anyhow!("unknown exchange '{}'", other)),
    };

    let timeframe: Timeframe = req
        .timeframe
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let security_codes = req
        .securities
        .clone()
        .unwrap_or_else(|| config.market.securities.clone());

    let securities: Vec<SecurityId> = security_codes
        .iter()
        .map(|code| SecurityId::etf(code.trim(), exchange))
        .collect();

    let start_date = NaiveDate::parse_from_str(&req.start_date, "%Y-%m-%d")?;
    let end_date = NaiveDate::parse_from_str(&req.end_date, "%Y-%m-%d")?;
    let start = Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).unwrap());
    let end = Utc.from_utc_datetime(&end_date.and_hms_opt(23, 59, 59).unwrap());

    // Connect to Futu
    let provider = FutuMarketDataProvider::connect(config.futu.clone())
        .await
        .context("failed to connect to FutuOpenD")?;

    // Create output directory
    std::fs::create_dir_all(&req.output_dir)
        .with_context(|| format!("cannot create output directory: {}", req.output_dir))?;

    let mut files = Vec::new();

    for security in &securities {
        let bars = provider
            .fetch_historical_bars(security, timeframe, start, end, None)
            .await
            .with_context(|| format!("failed to fetch bars for {}", security))?;

        if bars.is_empty() {
            continue;
        }

        let filename = format!(
            "{}/{}_{}.csv",
            req.output_dir, security.code, timeframe,
        );

        // Write CSV
        let mut output = String::new();
        output.push_str("timestamp,open,high,low,close,volume\n");
        for bar in &bars {
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
        std::fs::write(&filename, output)
            .with_context(|| format!("failed to write {}", filename))?;

        files.push(DataFetchFileResult {
            security: security.to_string(),
            bar_count: bars.len(),
            file_path: filename,
        });
    }

    Ok(files)
}

/// GET /api/data/{job_id}/status
pub async fn fetch_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<DataFetchStatusResponse>, ApiError> {
    let sessions = state.data_fetch_sessions.read().await;
    let session = sessions
        .get(&job_id)
        .ok_or_else(|| ApiError::NotFound(format!("data fetch job '{}' not found", job_id)))?;

    let sess = session.read().await;
    Ok(Json(DataFetchStatusResponse {
        job_id,
        status: sess.status.to_string(),
        files: sess.files.clone(),
        error: sess.error.clone(),
    }))
}
