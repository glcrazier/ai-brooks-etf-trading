use anyhow::Context;
use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;

use brooks_core::bar::Bar;
use brooks_core::market::{Exchange, SecurityId};
use brooks_core::timeframe::Timeframe;
use brooks_pa_engine::analyzer::PriceActionAnalyzer;

use crate::dto::AnalysisRequest;
use crate::error::ApiError;
use crate::state::AppState;

/// POST /api/analysis/run — run price action analysis on CSV data.
pub async fn run_analysis(
    State(state): State<AppState>,
    Json(req): Json<AnalysisRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate data_file exists
    if !std::path::Path::new(&req.data_file).exists() {
        return Err(ApiError::BadRequest(format!(
            "data file not found: {}",
            req.data_file
        )));
    }

    let config = state.config.clone();

    // Run analysis in blocking task (CPU-bound)
    let result = tokio::task::spawn_blocking(move || run_analysis_sync(&config, &req))
        .await
        .map_err(|e| ApiError::Internal(format!("task join error: {}", e)))?
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(result))
}

fn run_analysis_sync(
    config: &crate::state::ServerAppConfig,
    req: &AnalysisRequest,
) -> Result<serde_json::Value, anyhow::Error> {
    let timeframe: Timeframe = req
        .timeframe
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let exchange = match config.market.exchange.to_uppercase().as_str() {
        "SH" => Exchange::SH,
        "SZ" => Exchange::SZ,
        other => return Err(anyhow::anyhow!("unknown exchange '{}'", other)),
    };

    let security = if let Some(ref code) = req.security {
        SecurityId::etf(code.trim(), exchange)
    } else if let Some(first) = config.market.securities.first() {
        SecurityId::etf(first.trim(), exchange)
    } else {
        return Err(anyhow::anyhow!("no security specified"));
    };

    // Load bars
    let bars = load_bars_from_csv(&req.data_file, &security, timeframe)
        .with_context(|| format!("failed to load bars from {}", req.data_file))?;

    if bars.is_empty() {
        return Err(anyhow::anyhow!("no bars loaded from data file"));
    }

    // Create analyzer
    let pa_config = config.strategy.pa.to_pa_config();
    let mut analyzer = PriceActionAnalyzer::new(pa_config);

    for bar in &bars {
        analyzer.process_bar(bar);
    }

    let ctx = analyzer.current_context();
    let json =
        serde_json::to_value(ctx).context("failed to serialize analysis context")?;

    Ok(serde_json::json!({
        "security": security.to_string(),
        "timeframe": timeframe.to_string(),
        "bars_analyzed": bars.len(),
        "context": json,
    }))
}

/// Load bars from CSV.
fn load_bars_from_csv(
    path: &str,
    security: &SecurityId,
    timeframe: Timeframe,
) -> Result<Vec<Bar>, anyhow::Error> {
    use chrono::DateTime;

    let contents =
        std::fs::read_to_string(path).with_context(|| format!("cannot read file: {}", path))?;

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
            return Err(anyhow::anyhow!(
                "line {}: expected 6 columns, got {}",
                i + 1,
                fields.len()
            ));
        }

        let timestamp: DateTime<chrono::Utc> = fields[0]
            .trim()
            .parse()
            .with_context(|| format!("line {}: invalid timestamp", i + 1))?;
        let open: Decimal = fields[1]
            .trim()
            .parse()
            .with_context(|| format!("line {}", i + 1))?;
        let high: Decimal = fields[2]
            .trim()
            .parse()
            .with_context(|| format!("line {}", i + 1))?;
        let low: Decimal = fields[3]
            .trim()
            .parse()
            .with_context(|| format!("line {}", i + 1))?;
        let close: Decimal = fields[4]
            .trim()
            .parse()
            .with_context(|| format!("line {}", i + 1))?;
        let volume: u64 = fields[5]
            .trim()
            .parse()
            .with_context(|| format!("line {}", i + 1))?;

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
