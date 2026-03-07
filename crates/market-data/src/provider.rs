//! `FutuMarketDataProvider` — implements `MarketDataProvider` using Futu OpenD.
//!
//! Also provides `MockMarketDataProvider` for testing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use brooks_china_market::calendar::TradingCalendar;
use brooks_china_market::session::TradingSession;
use brooks_core::bar::Bar;
use brooks_core::event::MarketEvent;
use brooks_core::market::SecurityId;
use brooks_core::timeframe::Timeframe;
use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::aggregator::BarAggregator;
use crate::config::FutuConfig;
use crate::error::MarketDataError;
use crate::futu::client::FutuClient;
use crate::futu::messages::*;
use crate::traits::MarketDataProvider;

/// `MarketDataProvider` implementation backed by Futu OpenD.
///
/// Connects to a locally running FutuOpenD process, fetches historical bars,
/// and subscribes to real-time kline and tick updates.
pub struct FutuMarketDataProvider {
    client: Arc<FutuClient>,
    #[allow(dead_code)]
    session: TradingSession,
    #[allow(dead_code)]
    calendar: TradingCalendar,
    aggregators: RwLock<HashMap<(String, Timeframe), BarAggregator>>,
    connected: AtomicBool,
}

impl FutuMarketDataProvider {
    /// Create a new provider by connecting to Futu OpenD.
    pub async fn connect(config: FutuConfig) -> Result<Self, MarketDataError> {
        let client = FutuClient::connect(config).await?;

        Ok(Self {
            client: Arc::new(client),
            session: TradingSession::china_a_share(),
            calendar: TradingCalendar::china_2025(),
            aggregators: RwLock::new(HashMap::new()),
            connected: AtomicBool::new(true),
        })
    }

    /// Convert a security + timeframe pair to a hashmap key.
    fn aggregator_key(security: &SecurityId, timeframe: Timeframe) -> (String, Timeframe) {
        (security.to_string(), timeframe)
    }

    /// Format a DateTime as a Futu-compatible string in CST.
    fn format_futu_time(dt: DateTime<Utc>) -> String {
        let cst = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
        dt.with_timezone(&cst)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
}

#[async_trait]
impl MarketDataProvider for FutuMarketDataProvider {
    async fn fetch_historical_bars(
        &self,
        security: &SecurityId,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<Bar>, MarketDataError> {
        let begin = Self::format_futu_time(start);
        let end_str = Self::format_futu_time(end);
        let max_count = limit.map(|l| l as i32);

        self.client
            .request_history_kl(
                security,
                timeframe.as_futu_kl_type(),
                &begin,
                &end_str,
                max_count,
            )
            .await?;

        // In a full implementation, we would receive the response through the
        // push channel and correlate it with this request. For now, return empty
        // since we can't synchronously wait for the async push response.
        //
        // The backtester can use VecDataFeed with pre-loaded data.
        // Full implementation would use a oneshot channel or similar mechanism.
        warn!(
            "fetch_historical_bars: async response handling not yet implemented, returning empty"
        );
        Ok(vec![])
    }

    async fn fetch_latest_bars(
        &self,
        security: &SecurityId,
        timeframe: Timeframe,
        count: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        // Use a far-back start time and limit count
        let end = Utc::now();
        self.fetch_historical_bars(security, timeframe, end, end, Some(count))
            .await
    }

    async fn subscribe(
        &self,
        securities: &[SecurityId],
        timeframes: &[Timeframe],
    ) -> Result<mpsc::Receiver<MarketEvent>, MarketDataError> {
        // Compute Futu subscription types from timeframes
        let sub_types: Vec<i32> = timeframes
            .iter()
            .map(|tf| timeframe_to_sub_type(*tf))
            .collect();

        // Also subscribe to real-time ticks for aggregation
        let mut all_sub_types = sub_types;
        if !all_sub_types.contains(&SUB_TYPE_RT) {
            all_sub_types.push(SUB_TYPE_RT);
        }

        self.client.subscribe(securities, &all_sub_types).await?;

        // Create aggregators for each security/timeframe pair
        {
            let mut aggs = self.aggregators.write().await;
            for security in securities {
                for &timeframe in timeframes {
                    let key = Self::aggregator_key(security, timeframe);
                    aggs.entry(key)
                        .or_insert_with(|| BarAggregator::new(security.clone(), timeframe));
                }
            }
        }

        // Create event channel
        let (tx, rx) = mpsc::channel(512);

        info!(
            securities = ?securities.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            timeframes = ?timeframes,
            "Subscribed to market data"
        );

        // In a full implementation, we would spawn a task that reads from
        // the FutuClient's push receiver and converts events through aggregators.
        // The push receiver is started via `FutuClient::start_push_receiver()`.
        //
        // For now, return the channel — the event loop integration will be
        // completed when the app crate wires everything together.
        let _ = tx; // Will be used in the event loop task

        Ok(rx)
    }

    async fn unsubscribe(
        &self,
        securities: &[SecurityId],
        timeframes: &[Timeframe],
    ) -> Result<(), MarketDataError> {
        let sub_types: Vec<i32> = timeframes
            .iter()
            .map(|tf| timeframe_to_sub_type(*tf))
            .collect();
        self.client.unsubscribe(securities, &sub_types).await?;

        // Remove aggregators
        let mut aggs = self.aggregators.write().await;
        for security in securities {
            for &timeframe in timeframes {
                let key = Self::aggregator_key(security, timeframe);
                aggs.remove(&key);
            }
        }

        debug!("Unsubscribed and removed aggregators");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Mock provider for testing
// ============================================================================

/// A mock `MarketDataProvider` that returns pre-loaded bars.
///
/// Useful for unit testing strategies and the backtester without
/// needing a live Futu OpenD connection.
pub struct MockMarketDataProvider {
    bars: HashMap<(String, Timeframe), Vec<Bar>>,
}

impl MockMarketDataProvider {
    /// Create an empty mock provider.
    pub fn new() -> Self {
        Self {
            bars: HashMap::new(),
        }
    }

    /// Add bars for a specific security and timeframe.
    pub fn add_bars(&mut self, security: &SecurityId, timeframe: Timeframe, bars: Vec<Bar>) {
        let key = (security.to_string(), timeframe);
        self.bars.insert(key, bars);
    }
}

impl Default for MockMarketDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for MockMarketDataProvider {
    async fn fetch_historical_bars(
        &self,
        security: &SecurityId,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<Bar>, MarketDataError> {
        let key = (security.to_string(), timeframe);
        let bars = self.bars.get(&key).cloned().unwrap_or_default();

        // Filter by time range
        let mut filtered: Vec<Bar> = bars
            .into_iter()
            .filter(|b| b.timestamp >= start && b.timestamp <= end)
            .collect();

        if let Some(limit) = limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    async fn fetch_latest_bars(
        &self,
        security: &SecurityId,
        timeframe: Timeframe,
        count: usize,
    ) -> Result<Vec<Bar>, MarketDataError> {
        let key = (security.to_string(), timeframe);
        let bars = self.bars.get(&key).cloned().unwrap_or_default();

        let start = bars.len().saturating_sub(count);
        Ok(bars[start..].to_vec())
    }

    async fn subscribe(
        &self,
        _securities: &[SecurityId],
        _timeframes: &[Timeframe],
    ) -> Result<mpsc::Receiver<MarketEvent>, MarketDataError> {
        let (_tx, rx) = mpsc::channel(1);
        Ok(rx)
    }

    async fn unsubscribe(
        &self,
        _securities: &[SecurityId],
        _timeframes: &[Timeframe],
    ) -> Result<(), MarketDataError> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn make_bar(close: Decimal, mins_offset: i64) -> Bar {
        use chrono::Duration;
        Bar {
            timestamp: DateTime::from_timestamp(1705300800, 0).unwrap()
                + Duration::minutes(mins_offset),
            open: close - dec!(0.01),
            high: close + dec!(0.05),
            low: close - dec!(0.05),
            close,
            volume: 1000,
            timeframe: Timeframe::Minute5,
            security: SecurityId::etf("510050", Exchange::SH),
        }
    }

    #[tokio::test]
    async fn test_mock_provider_fetch_historical() {
        let security = SecurityId::etf("510050", Exchange::SH);
        let mut provider = MockMarketDataProvider::new();
        let bars = vec![
            make_bar(dec!(3.00), 0),
            make_bar(dec!(3.05), 5),
            make_bar(dec!(3.10), 10),
        ];
        provider.add_bars(&security, Timeframe::Minute5, bars);

        let start = DateTime::from_timestamp(1705300800, 0).unwrap();
        let end = start + chrono::Duration::minutes(15);

        let result = provider
            .fetch_historical_bars(&security, Timeframe::Minute5, start, end, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_provider_fetch_with_limit() {
        let security = SecurityId::etf("510050", Exchange::SH);
        let mut provider = MockMarketDataProvider::new();
        let bars = vec![
            make_bar(dec!(3.00), 0),
            make_bar(dec!(3.05), 5),
            make_bar(dec!(3.10), 10),
        ];
        provider.add_bars(&security, Timeframe::Minute5, bars);

        let start = DateTime::from_timestamp(1705300800, 0).unwrap();
        let end = start + chrono::Duration::minutes(15);

        let result = provider
            .fetch_historical_bars(&security, Timeframe::Minute5, start, end, Some(2))
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_provider_fetch_latest() {
        let security = SecurityId::etf("510050", Exchange::SH);
        let mut provider = MockMarketDataProvider::new();
        let bars = vec![
            make_bar(dec!(3.00), 0),
            make_bar(dec!(3.05), 5),
            make_bar(dec!(3.10), 10),
        ];
        provider.add_bars(&security, Timeframe::Minute5, bars);

        let result = provider
            .fetch_latest_bars(&security, Timeframe::Minute5, 2)
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].close, dec!(3.05));
        assert_eq!(result[1].close, dec!(3.10));
    }

    #[tokio::test]
    async fn test_mock_provider_empty() {
        let security = SecurityId::etf("510050", Exchange::SH);
        let provider = MockMarketDataProvider::new();

        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now();
        let result = provider
            .fetch_historical_bars(&security, Timeframe::Minute5, start, end, None)
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_mock_provider_is_connected() {
        let provider = MockMarketDataProvider::new();
        assert!(provider.is_connected());
    }

    #[tokio::test]
    async fn test_mock_provider_subscribe_unsubscribe() {
        let provider = MockMarketDataProvider::new();
        let security = SecurityId::etf("510050", Exchange::SH);

        // Should not panic
        let _rx = provider
            .subscribe(std::slice::from_ref(&security), &[Timeframe::Minute5])
            .await
            .unwrap();
        provider
            .unsubscribe(&[security], &[Timeframe::Minute5])
            .await
            .unwrap();
    }

    #[test]
    fn test_format_futu_time() {
        // 1705305600 = 2024-01-15 08:00:00 UTC = 2024-01-15 16:00:00 CST
        let dt = DateTime::from_timestamp(1705305600, 0).unwrap();
        let formatted = FutuMarketDataProvider::format_futu_time(dt);
        assert_eq!(formatted, "2024-01-15 16:00:00");
    }
}
